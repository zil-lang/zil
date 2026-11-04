//! Owning replacement definitions and affine dynamic activations. Tickets are
//! never ordinary values; endpoint authority comes only from actual storage.
use super::*;
use std::collections::BTreeMap;
use zil_ir::{PlaceProjection, ReservationId};
mod origins;

#[derive(Clone)]
pub(super) enum Base {
    Local(LocalId),
    Address(ValueId),
}
#[derive(Clone)]
pub(super) struct Definition {
    pub(super) base: Base,
    pub(super) path: Vec<PlaceProjection>,
    pub(super) ty: Type,
}
pub(super) struct Definitions {
    pub(super) entries: BTreeMap<ReservationId, Definition>,
}
fn invalid(text: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-PLACE-REPLACE",
        None,
        text,
    )]
}
impl Definitions {
    pub(super) fn new(
        module: &TypedModule,
        f: &TypedFunction,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        let mut entries = BTreeMap::new();
        for block in &f.blocks {
            b.spend(1, "replacement definition block")?;
            for i in &block.instructions {
                b.spend(1, "replacement definition instruction")?;
                let (id, base, path, root) = match &i.kind {
                    InstructionKind::ReservePlaceLocal {
                        reservation,
                        local,
                        path,
                    } => {
                        let declaration = f
                            .locals
                            .get(local.0 as usize)
                            .ok_or_else(|| invalid("reservation local is absent"))?;
                        if !declaration.mutable {
                            return Err(invalid("reservation needs actual mutable local storage"));
                        }
                        (*reservation, Base::Local(*local), path, &declaration.ty)
                    }
                    InstructionKind::ReservePlaceAddress {
                        reservation,
                        address,
                        path,
                    } => {
                        let Some(Type::Address {
                            mutable: true,
                            inner,
                        }) = f.values.get(address.0 as usize)
                        else {
                            return Err(invalid(
                                "reservation needs an actual mutable parent address",
                            ));
                        };
                        (*reservation, Base::Address(*address), path, inner.as_ref())
                    }
                    _ => continue,
                };
                if i.result.is_some() {
                    return Err(invalid(
                        "a replacement reservation cannot define an SSA value",
                    ));
                }
                if path.len() > b.partial_depth {
                    return Err(vec![Diagnostic::limit(
                        Phase::Analyze,
                        "replacement path depth exceeded",
                        Some(i.span),
                    )]);
                }
                b.type_work(root, true, "replacement complete root Type")?;
                b.spend(path.len().saturating_add(1), "replacement exact path")?;
                let mut ty = root;
                for step in path {
                    ty = match (step, ty) {
                        (PlaceProjection::Field(index), Type::Tuple(fields)) => {
                            fields.get(*index as usize)
                        }
                        (
                            PlaceProjection::Field(index),
                            Type::Package { fields, .. } | Type::Record { fields, .. },
                        ) => fields.get(*index as usize).map(|(_, t)| t),
                        (PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                            .get(p.variant as usize)
                            .and_then(|(_, fields)| fields.get(p.index as usize)),
                        (
                            PlaceProjection::ErrorArm(error_arm),
                            Type::ErrorUnion { error, result },
                        ) => Some(if *error_arm {
                            error.as_ref()
                        } else {
                            result.as_ref()
                        }),
                        _ => None,
                    }
                    .ok_or_else(|| {
                        invalid("replacement path crosses a view or invalid inline subtree")
                    })?;
                }
                if !resources::sums::owned(ty)
                    && !matches!(ty, Type::Managed(_) | Type::Package { .. })
                {
                    return Err(invalid(
                        "replacement endpoint must carry actual owning responsibility",
                    ));
                }
                resources::sums::admit(module, ty, b)?;
                b.type_work(ty, true, "replacement endpoint snapshot")?;
                b.spend(path.len(), "replacement path snapshot")?;
                b.lookup(entries.len(), "replacement unique ticket definition")?;
                if entries
                    .insert(
                        id,
                        Definition {
                            base,
                            path: path.clone(),
                            ty: ty.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(invalid(
                        "replacement ticket has more than one static definition",
                    ));
                }
            }
        }
        Ok(Self { entries })
    }
    pub(super) fn definition(
        &self,
        id: ReservationId,
        b: &mut Budget,
    ) -> CompileResult<&Definition> {
        b.lookup(self.entries.len(), "replacement ticket lookup")?;
        self.entries
            .get(&id)
            .ok_or_else(|| invalid("replacement ticket has no actual reserve definition"))
    }
    /// This only validates the affine activation protocol. The storage, loan,
    /// initialization and responsibility consumers must also check each commit.
    pub(super) fn activations(
        &self,
        f: &TypedFunction,
        reachable: &[bool],
        b: &mut Budget,
    ) -> CompileResult<Vec<Option<BTreeSet<ReservationId>>>> {
        b.spend(
            f.blocks.len().saturating_mul(2),
            "replacement activation tables",
        )?;
        let mut incoming = vec![None; f.blocks.len()];
        incoming[f.entry.0 as usize] = Some(BTreeSet::new());
        let mut todo = VecDeque::from([f.entry.0 as usize]);
        while let Some(block) = todo.pop_front() {
            let current = incoming[block]
                .as_ref()
                .ok_or_else(|| invalid("replacement activation entry missing"))?;
            b.spend(
                current.len().saturating_add(1),
                "replacement activation snapshot",
            )?;
            let mut active = current.clone();
            for i in &f.blocks[block].instructions {
                b.spend(1, "replacement activation instruction")?;
                match &i.kind {
                    InstructionKind::ReservePlaceLocal { reservation, .. }
                    | InstructionKind::ReservePlaceAddress { reservation, .. } => {
                        self.definition(*reservation, b)?;
                        b.lookup(active.len(), "replacement activation insert")?;
                        if !active.insert(*reservation) {
                            return Err(invalid("a previous ticket activation is still prepared"));
                        }
                    }
                    InstructionKind::CommitPlaceReplace { reservation, value } => {
                        let definition = self.definition(*reservation, b)?;
                        b.lookup(active.len(), "replacement activation commit")?;
                        if !active.remove(reservation) {
                            return Err(invalid("commit needs one currently prepared activation"));
                        }
                        b.type_work(&definition.ty, true, "replacement exact new owner Type")?;
                        if f.values.get(value.0 as usize) != Some(&definition.ty)
                            || i.result.is_some()
                        {
                            return Err(invalid(
                                "commit needs a complete exact-type owner and no SSA result",
                            ));
                        }
                    }
                    InstructionKind::AbortPlaceReservation { reservation } => {
                        self.definition(*reservation, b)?;
                        b.lookup(active.len(), "replacement activation abort")?;
                        if !active.remove(reservation) || i.result.is_some() {
                            return Err(invalid(
                                "abort needs one prepared activation and no SSA result",
                            ));
                        }
                    }
                    _ => {}
                }
            }
            let term = &f.blocks[block].terminator;
            let (edges, count) = match term {
                Terminator::Jump(target) => ([target.0 as usize, 0], 1),
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => ([then_block.0 as usize, else_block.0 as usize], 2),
                _ => {
                    if !active.is_empty() {
                        return Err(invalid(
                            "function exit leaves a prepared replacement activation",
                        ));
                    }
                    ([0, 0], 0)
                }
            };
            b.spend(count, "replacement successor edges")?;
            for target in edges.into_iter().take(count) {
                if !reachable[target] {
                    continue;
                }
                if let Some(prior) = &incoming[target] {
                    b.spend(
                        prior.len().saturating_add(active.len()).saturating_add(1),
                        "replacement exact activation join",
                    )?;
                    if prior != &active {
                        return Err(invalid(
                            "control-flow join disagrees about prepared replacement activations",
                        ));
                    }
                } else {
                    b.spend(active.len().saturating_add(1), "replacement edge snapshot")?;
                    incoming[target] = Some(active.clone());
                    todo.push_back(target);
                }
            }
        }
        Ok(incoming)
    }
}

/// Ordinary SSA operands are insufficient: Commit/Abort mention only a ticket,
/// while its genuine addressed base must keep its permission until settlement.
pub(super) struct Schedule {
    pub(super) incoming: Vec<Option<BTreeSet<ReservationId>>>,
    pub(super) uses: Vec<Vec<Vec<ValueId>>>,
}
impl Definitions {
    pub(super) fn schedule(
        &self,
        f: &TypedFunction,
        reachable: &[bool],
        b: &mut Budget,
    ) -> CompileResult<Schedule> {
        let incoming = self.activations(f, reachable, b)?;
        b.spend(f.blocks.len(), "replacement implicit use blocks")?;
        let mut uses = Vec::with_capacity(f.blocks.len());
        for (index, block) in f.blocks.iter().enumerate() {
            b.spend(block.instructions.len(), "replacement implicit use rows")?;
            let mut rows = Vec::with_capacity(block.instructions.len());
            let active = incoming[index].as_ref();
            b.spend(
                active.map_or(0, BTreeSet::len),
                "replacement schedule entry snapshot",
            )?;
            let mut active = active.cloned().unwrap_or_default();
            for i in &block.instructions {
                if let InstructionKind::ReservePlaceLocal { reservation, .. }
                | InstructionKind::ReservePlaceAddress { reservation, .. } = i.kind
                {
                    b.lookup(active.len(), "replacement schedule prepare")?;
                    active.insert(reservation);
                }
                // Spend before allocating/walking the active ticket row.
                b.spend(active.len(), "replacement retained address operands")?;
                let mut row = Vec::with_capacity(active.len());
                for id in &active {
                    if let Base::Address(address) = self.definition(*id, b)?.base {
                        row.push(address);
                    }
                }
                rows.push(row);
                if let InstructionKind::CommitPlaceReplace { reservation, .. }
                | InstructionKind::AbortPlaceReservation { reservation } = i.kind
                {
                    b.lookup(active.len(), "replacement schedule settle")?;
                    active.remove(&reservation);
                }
            }
            uses.push(rows);
        }
        Ok(Schedule { incoming, uses })
    }
}

#[cfg(test)]
mod tests;

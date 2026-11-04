//! Independent affine replacement-ticket definitions. No producer claim can
//! supply a ticket type, a mutable endpoint or a prepared dynamic activation.
use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use zil_ir::{PlaceProjection, ReservationId};

#[derive(Clone)]
pub(super) enum Origin {
    Local(LocalId),
    Address(ValueId),
}
#[derive(Clone)]
pub(super) struct Place {
    pub(super) origin: Origin,
    pub(super) steps: Vec<PlaceProjection>,
    pub(super) ty: Type,
}
pub(super) struct Inventory {
    pub(super) places: BTreeMap<ReservationId, Place>,
}
fn bad(span: Option<Span>, text: &str) -> Diagnostic {
    reject("E-VERIFY-PLACE-REPLACE", span, text)
}
fn lookup(b: &mut Budget, count: usize) -> Check<()> {
    let n = count
        .checked_ilog2()
        .map_or(1, |n| n as usize + 2)
        .checked_mul(8)
        .ok_or_else(|| limit("replacement lookup fee overflow"))?;
    b.spend(n)
}
impl Inventory {
    pub(super) fn reconstruct(
        module: &TypedModule,
        f: &TypedFunction,
        b: &mut Budget,
    ) -> Check<Self> {
        let mut places = BTreeMap::new();
        for block in &f.blocks {
            b.spend(1)?;
            for instruction in &block.instructions {
                b.spend(1)?;
                let span = Some(instruction.span);
                let (id, origin, steps, root) = match &instruction.kind {
                    InstructionKind::ReservePlaceLocal {
                        reservation,
                        local,
                        path,
                    } => {
                        let slot = f
                            .locals
                            .get(local.0 as usize)
                            .ok_or_else(|| bad(span, "reserve local is absent"))?;
                        if !slot.mutable {
                            return Err(bad(span, "reserve requires a writable real local"));
                        }
                        (*reservation, Origin::Local(*local), path, &slot.ty)
                    }
                    InstructionKind::ReservePlaceAddress {
                        reservation,
                        address,
                        path,
                    } => {
                        let Type::Address {
                            mutable: true,
                            inner,
                        } = value_type(f, *address, span)?
                        else {
                            return Err(bad(
                                span,
                                "reserve parent has no mutable address authority",
                            ));
                        };
                        (
                            *reservation,
                            Origin::Address(*address),
                            path,
                            inner.as_ref(),
                        )
                    }
                    _ => continue,
                };
                if instruction.result.is_some() {
                    return Err(bad(span, "a reservation is not an SSA value"));
                }
                if steps.len() > b.limits.max_call_depth {
                    return Err(limit("replacement path exceeds depth limit"));
                }
                resources::charge_payload(root, b)?;
                b.spend(steps.len().saturating_add(1))?;
                let mut cursor = root;
                for step in steps {
                    cursor = match (step, cursor) {
                        (PlaceProjection::Field(i), Type::Tuple(fields)) => fields.get(*i as usize),
                        (
                            PlaceProjection::Field(i),
                            Type::Package { fields, .. } | Type::Record { fields, .. },
                        ) => fields.get(*i as usize).map(|(_, ty)| ty),
                        (PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                            .get(p.variant as usize)
                            .and_then(|(_, fields)| fields.get(p.index as usize)),
                        (
                            PlaceProjection::ErrorArm(is_error),
                            Type::ErrorUnion { error, result },
                        ) => Some(if *is_error {
                            error.as_ref()
                        } else {
                            result.as_ref()
                        }),
                        _ => None,
                    }
                    .ok_or_else(|| bad(span, "reservation route is not an actual inline path"))?;
                }
                if !sums::owned(cursor)
                    && !matches!(cursor, Type::Managed(_) | Type::Package { .. })
                {
                    return Err(bad(
                        span,
                        "reserved endpoint must own a complete cleanup obligation",
                    ));
                }
                sums::check_type_for(module, cursor, span, b)?;
                resources::charge_payload(cursor, b)?;
                b.spend(steps.len())?;
                lookup(b, places.len())?;
                if places
                    .insert(
                        id,
                        Place {
                            origin,
                            steps: steps.clone(),
                            ty: cursor.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(bad(
                        span,
                        "ticket identity has multiple static reserve definitions",
                    ));
                }
            }
        }
        Ok(Self { places })
    }
    pub(super) fn get(&self, id: ReservationId, b: &mut Budget) -> Check<&Place> {
        lookup(b, self.places.len())?;
        self.places
            .get(&id)
            .ok_or_else(|| bad(None, "ticket references no actual reserve definition"))
    }
    pub(super) fn activation_entries(
        &self,
        f: &TypedFunction,
        graph: &FunctionGraph,
        b: &mut Budget,
    ) -> Check<Vec<Option<BTreeSet<ReservationId>>>> {
        b.spend(f.blocks.len().saturating_mul(2))?;
        let mut entries = vec![None; f.blocks.len()];
        entries[f.entry.0 as usize] = Some(BTreeSet::new());
        let mut queue = VecDeque::from([f.entry.0 as usize]);
        while let Some(block) = queue.pop_front() {
            let prior = entries[block]
                .as_ref()
                .ok_or_else(|| bad(None, "reservation entry state is absent"))?;
            b.spend(prior.len().saturating_add(1))?;
            let mut prepared = prior.clone();
            for i in &f.blocks[block].instructions {
                b.spend(1)?;
                match &i.kind {
                    InstructionKind::ReservePlaceLocal { reservation, .. }
                    | InstructionKind::ReservePlaceAddress { reservation, .. } => {
                        self.get(*reservation, b)?;
                        lookup(b, prepared.len())?;
                        if !prepared.insert(*reservation) {
                            return Err(bad(
                                Some(i.span),
                                "reserve reuses an unsettled dynamic activation",
                            ));
                        }
                    }
                    InstructionKind::CommitPlaceReplace { reservation, value } => {
                        let place = self.get(*reservation, b)?;
                        lookup(b, prepared.len())?;
                        if !prepared.remove(reservation) {
                            return Err(bad(
                                Some(i.span),
                                "commit has no prepared activation or consumes it twice",
                            ));
                        }
                        resources::charge_payload(&place.ty, b)?;
                        if value_type(f, *value, Some(i.span))? != &place.ty || i.result.is_some() {
                            return Err(bad(
                                Some(i.span),
                                "commit requires an exact complete new owner and no SSA result",
                            ));
                        }
                    }
                    InstructionKind::AbortPlaceReservation { reservation } => {
                        self.get(*reservation, b)?;
                        lookup(b, prepared.len())?;
                        if !prepared.remove(reservation) || i.result.is_some() {
                            return Err(bad(
                                Some(i.span),
                                "abort has no prepared activation or consumes it twice",
                            ));
                        }
                    }
                    _ => {}
                }
            }
            if graph.successors[block].is_empty() && !prepared.is_empty() {
                return Err(bad(
                    Some(f.span),
                    "normal exit leaks a prepared replacement activation",
                ));
            }
            b.spend(graph.successors[block].len())?;
            for &next in &graph.successors[block] {
                if !graph.reachable[next] {
                    continue;
                }
                if let Some(other) = &entries[next] {
                    b.spend(other.len().saturating_add(prepared.len()).saturating_add(1))?;
                    if other != &prepared {
                        return Err(bad(
                            Some(f.span),
                            "join combines different active replacement sets",
                        ));
                    }
                } else {
                    b.spend(prepared.len().saturating_add(1))?;
                    entries[next] = Some(prepared.clone());
                    queue.push_back(next);
                }
            }
        }
        Ok(entries)
    }
}

/// Hidden dependencies belong to the verified ticket protocol, not to claimed
/// ValueHold rows or to a forged ordinary Address result.
pub(super) struct Schedule {
    pub(super) entries: Vec<Option<BTreeSet<ReservationId>>>,
    pub(super) retained: Vec<Vec<Vec<ValueId>>>,
}
impl Inventory {
    pub(super) fn schedule(
        &self,
        f: &TypedFunction,
        graph: &FunctionGraph,
        b: &mut Budget,
    ) -> Check<Schedule> {
        let entries = self.activation_entries(f, graph, b)?;
        b.spend(f.blocks.len())?;
        let mut retained = Vec::with_capacity(f.blocks.len());
        for (block_index, block) in f.blocks.iter().enumerate() {
            b.spend(block.instructions.len())?;
            let input = entries[block_index].as_ref();
            b.spend(input.map_or(0, BTreeSet::len))?;
            let mut prepared = input.cloned().unwrap_or_default();
            let mut rows = Vec::with_capacity(block.instructions.len());
            for instruction in &block.instructions {
                if let InstructionKind::ReservePlaceLocal { reservation, .. }
                | InstructionKind::ReservePlaceAddress { reservation, .. } = instruction.kind
                {
                    lookup(b, prepared.len())?;
                    prepared.insert(reservation);
                }
                b.spend(prepared.len())?;
                let mut required = Vec::with_capacity(prepared.len());
                for id in &prepared {
                    if let Origin::Address(address) = self.get(*id, b)?.origin {
                        required.push(address);
                    }
                }
                rows.push(required);
                if let InstructionKind::CommitPlaceReplace { reservation, .. }
                | InstructionKind::AbortPlaceReservation { reservation } = instruction.kind
                {
                    lookup(b, prepared.len())?;
                    prepared.remove(&reservation);
                }
            }
            retained.push(rows);
        }
        Ok(Schedule { entries, retained })
    }
}

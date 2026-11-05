//! Actual Commit transfers one staged responsibility into an already live
//! owning position. The old destination has no second invented local debt.
use super::*;
use crate::place_replacements::{Inventory, actual};
use std::cell::RefCell;
use std::collections::BTreeMap;
mod retirement;
pub(super) use retirement::Sources;
pub(super) struct Plan {
    actual: actual::Program,
    inventories: Vec<Inventory>,
    written: Vec<BTreeSet<Root>>,
    targets: BTreeMap<usize, Vec<actual::Target>>,
    retirements: BTreeMap<usize, Vec<actual::Target>>,
    active_pairs: bool,
    pairs: RefCell<Vec<retirement::Pairs>>,
    calls: RefCell<Vec<Vec<retirement::Call>>>,
}
#[derive(Clone, Copy)]
struct Point {
    position: usize,
    block: usize,
    validate: bool,
    span: Span,
    sealed: bool,
    output: Option<usize>,
}
struct Invocation<'a> {
    entry: usize,
    arguments: &'a [ValueId],
    captures: Option<&'a actual::Captures>,
    carrier: Option<usize>,
    sources: Option<usize>,
}
fn nested_descriptor_boundary(module: &TypedModule, b: &mut Budget) -> Check<()> {
    b.spend(module.closures.len())?;
    for environment in &module.closures {
        b.spend(environment.captures.len())?;
        for field in &environment.captures {
            let inner = match field {
                Type::Closure(id) => Some(*id),
                Type::Address { inner, .. } => {
                    if let Type::Closure(id) = inner.as_ref() {
                        Some(*id)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let Some(id) = inner else {
                continue;
            };
            let target = module.closures.get(id.0 as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-RESOURCE",
                    None,
                    "nested description closure identity absent",
                )
            })?;
            b.spend(target.captures.len())?;
            if target
                .captures
                .iter()
                .any(|capture| matches!(capture, Type::Desc(_)))
            {
                return Err(unsupported(
                    "nested closure resource descriptions need their transitive carrier bridge",
                    None,
                ));
            }
        }
    }
    Ok(())
}
impl Plan {
    pub(super) fn new(
        module: &TypedModule,
        graphs: &[FunctionGraph],
        writes: &[BTreeSet<Root>],
        targets: &BTreeMap<usize, Vec<actual::Target>>,
        retirements: &BTreeMap<usize, Vec<actual::Target>>,
        b: &mut Budget,
    ) -> Check<Self> {
        nested_descriptor_boundary(module, b)?;
        let actual = actual::Program::reconstruct(module, graphs, b)?;
        b.spend(module.functions.len().saturating_add(writes.len()))?;
        let mut inventories = Vec::with_capacity(module.functions.len());
        for f in &module.functions {
            inventories.push(Inventory::reconstruct(module, f, b)?);
        }
        for row in writes {
            b.spend(row.len())?;
        }
        b.spend(targets.len())?;
        for row in targets.values() {
            for target in row {
                b.spend(target.steps.len().saturating_add(1))?;
            }
        }
        if !retirements.is_empty() {
            b.spend(retirements.len())?;
            for row in retirements.values() {
                b.spend(row.len())?;
                for target in row {
                    b.spend(target.steps.len().saturating_add(1))?;
                }
            }
        }
        let active_pairs = !targets.is_empty() || !retirements.is_empty();
        if active_pairs {
            b.spend(module.functions.len().saturating_mul(2))?;
        }
        Ok(Self {
            active_pairs,
            pairs: RefCell::new(if active_pairs {
                vec![retirement::Pairs::new(); module.functions.len()]
            } else {
                vec![]
            }),
            calls: RefCell::new(if active_pairs {
                (0..module.functions.len()).map(|_| Vec::new()).collect()
            } else {
                vec![]
            }),
            actual,
            inventories,
            written: writes.to_vec(),
            targets: targets.clone(),
            retirements: retirements.clone(),
        })
    }
    pub(super) fn extend_slots(
        &self,
        f: &TypedFunction,
        slots: &mut Slots,
        b: &mut Budget,
    ) -> Check<()> {
        slots.retirement = self.active_pairs;
        let routes = &self.actual.functions[f.id.0 as usize].values;
        for (value, row) in routes.iter().enumerate() {
            b.spend(row.len().saturating_add(1))?;
            let ty = &f.values[value];
            if slots.desc_value[value].is_none()
                && (head_address(ty) || ty.is_package_address())
                && row
                    .iter()
                    .any(|target| matches!(target.base, actual::Base::Element(_)))
            {
                slots.desc_value[value] = Some(slots.desc_count);
                slots.desc_count = slots
                    .desc_count
                    .checked_add(1)
                    .ok_or_else(|| limit("Store address description slot count overflow"))?;
            }
        }
        for block in &f.blocks {
            for i in &block.instructions {
                b.spend(1)?;
                if let InstructionKind::Store { local, value } = i.kind {
                    let row = &routes[value.0 as usize];
                    b.spend(row.len())?;
                    let ty = &f.locals[local.0 as usize].ty;
                    if slots.desc_local[local.0 as usize].is_none()
                        && (head_address(ty) || ty.is_package_address())
                        && row
                            .iter()
                            .any(|target| matches!(target.base, actual::Base::Element(_)))
                    {
                        slots.desc_local[local.0 as usize] = Some(slots.desc_count);
                        slots.desc_count = slots
                            .desc_count
                            .checked_add(1)
                            .ok_or_else(|| limit("Store local description slot count overflow"))?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn writes(&self, id: FunctionId, b: &mut Budget) -> Check<BTreeSet<Root>> {
        let row = &self.written[id.0 as usize];
        b.spend(row.len())?;
        Ok(row.clone())
    }
    fn retire(
        &self,
        transfer: &Transfer<'_>,
        state: &State,
        target: &actual::Target,
        point: Point,
        b: &mut Budget,
    ) -> Check<()> {
        if !point.validate || !self.active_pairs {
            return Ok(());
        }
        retirement::observe(self, transfer, state, target, point, b)
    }
    pub(super) fn seed_sources(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        slots: &Slots,
        state: &mut State,
        b: &mut Budget,
    ) -> Check<()> {
        retirement::seed(self, module, f, slots, state, b)
    }
    pub(super) fn source_step(
        &self,
        transfer: &Transfer<'_>,
        state: &mut State,
        instruction: &Instruction,
        b: &mut Budget,
    ) -> Check<()> {
        retirement::step(self, transfer, state, instruction, b)
    }
    pub(super) fn close_pairs(&self, module: &TypedModule, b: &mut Budget) -> Check<()> {
        retirement::close(self, module, b)
    }

    fn invalidate_store(
        &self,
        transfer: &Transfer<'_>,
        state: &State,
        parents: &BTreeSet<Root>,
        written: &mut BTreeSet<Root>,
        point: Point,
        b: &mut Budget,
    ) -> Check<()> {
        let f = transfer.f;
        if point.sealed {
            return Err(unsupported(
                "Store instance invalidation across a sealed callback scope needs its simultaneous descriptor proof",
                Some(point.span),
            ));
        }
        if parents.is_empty() {
            return Err(bad(
                point.span,
                "Store replacement has no actual parent responsibility family",
            ));
        }
        b.spend(parents.len())?;
        let nonlocal =
            |root: Root| matches!(root.parent(), Root::Parameter(_) | Root::BorrowedCapture(_));
        if parents.iter().any(|root| matches!(root.parent(), Root::OwnedCapture(_))
            || matches!(root.parent(), Root::PackageField { local, .. } if transfer.slots.view(local).is_some())) {
            return Err(unsupported("owned Store capture replacement needs its exact capture instance mapping", Some(point.span)));
        }
        if parents.iter().any(|root| nonlocal(*root))
            && (f.callable_contract.is_some() || f.callable_return_contract.is_some())
        {
            return Err(unsupported(
                "Store instance invalidation needs a public callable envelope",
                Some(point.span),
            ));
        }
        if point.validate {
            for (ty, slot) in f.values.iter().zip(&transfer.slots.desc_value).chain(
                f.locals
                    .iter()
                    .map(|local| &local.ty)
                    .zip(&transfer.slots.desc_local),
            ) {
                b.spend(1)?;
                if !(matches!(ty, Type::Desc(_) | Type::Closure(_))
                    || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_))))
                {
                    continue;
                }
                let Some(slot) = *slot else {
                    continue;
                };
                if !transfer.after[point.block][point.position][slot] {
                    continue;
                }
                b.spend(
                    state.desc[slot]
                        .len()
                        .saturating_mul(parents.len().saturating_add(1)),
                )?;
                if state.desc[slot].iter().any(|root| {
                    !matches!(root, Root::StoreParent(_))
                        && parents.iter().any(|actual| {
                            root.parent() == actual.parent()
                                || (nonlocal(*root) && nonlocal(*actual))
                        })
                }) {
                    return Err(bad(
                        point.span,
                        "Store field replacement invalidates a live element resource description",
                    ));
                }
            }
        }
        transfer.write_roots(parents, written, point.span, point.validate, b)?;
        for root in parents {
            let key = match root.parent() {
                Root::Parameter(p) => Some(StoreParentRoot::Parameter(p)),
                Root::BorrowedCapture(c) => Some(StoreParentRoot::BorrowedCapture(c)),
                _ => None,
            };
            if let Some(key) = key {
                b.spend(written.len().saturating_add(1))?;
                written.insert(Root::InvalidatedStore(key));
            }
        }
        Ok(())
    }
    fn invoke(
        &self,
        transfer: &Transfer<'_>,
        state: &State,
        written: &mut BTreeSet<Root>,
        point: Point,
        invocation: Invocation<'_>,
        b: &mut Budget,
    ) -> Check<()> {
        let f = transfer.f;
        let callee = &transfer.module.functions[invocation.entry];
        if point.validate && self.active_pairs {
            retirement::call(self, transfer, state, &invocation, point, b)?;
        }
        // A checked public envelope does not authorize looking at private rows.
        // Actual Store invalidation in such a body is independently closed by
        // invalidate_store, rather than importing a private effect at the call.
        if callee.callable_contract.is_some() || callee.callable_return_contract.is_some() {
            return Ok(());
        }
        if point.sealed {
            b.spend(transfer.effects[invocation.entry].len())?;
            if transfer.effects[invocation.entry]
                .iter()
                .any(|effect| matches!(effect, Root::InvalidatedStore(_)))
            {
                return Err(unsupported(
                    "Store instance invalidation across a sealed callback scope needs its simultaneous descriptor proof",
                    Some(point.span),
                ));
            }
        }
        b.spend(self.targets.len())?;
        let rows = self
            .targets
            .get(&invocation.entry)
            .map_or(&[][..], Vec::as_slice);
        b.spend(rows.len())?;
        for target in rows {
            b.spend(target.steps.len().saturating_add(1))?;
            let selected = actual::substitute(
                &BTreeSet::from([target.clone()]),
                invocation.arguments,
                invocation.captures,
                &self.actual.functions[f.id.0 as usize],
                b,
            )?;
            for target in selected {
                if let actual::Base::Element(element) = target.base {
                    sums::replacements::at_type(transfer.module, f, &target, b)?;
                    let parent =
                        transfer.slots.desc_value[element.0 as usize].ok_or_else(|| {
                            bad(
                                point.span,
                                "Store call target has no actual resource parent",
                            )
                        })?;
                    if point.validate {
                        description_live(state, transfer.slots, parent, point.span, b)?;
                    }
                    self.invalidate_store(transfer, state, &state.desc[parent], written, point, b)?;
                } else {
                    self.retire(transfer, state, &target, point, b)?;
                }
            }
        }
        // Empty on historical local-only/Store paths. This paid target ledger
        // is derived solely from real possible Commits, including nested calls.
        if !self.retirements.is_empty() {
            b.spend(self.retirements.len())?;
            if let Some(rows) = self.retirements.get(&invocation.entry) {
                b.spend(rows.len())?;
                for target in rows {
                    b.spend(target.steps.len().saturating_add(1))?;
                    let selected = actual::substitute(
                        &BTreeSet::from([target.clone()]),
                        invocation.arguments,
                        invocation.captures,
                        &self.actual.functions[f.id.0 as usize],
                        b,
                    )?;
                    for target in selected {
                        self.retire(transfer, state, &target, point, b)?;
                    }
                }
            }
        }
        let effects = &transfer.effects[invocation.entry];
        b.spend(effects.len())?;
        for effect in effects {
            let Root::InvalidatedStore(key) = effect else {
                continue;
            };
            let parents = match key {
                StoreParentRoot::Parameter(p) => {
                    let value = invocation
                        .arguments
                        .get(*p as usize)
                        .ok_or_else(|| bad(point.span, "Store invalidation input absent"))?;
                    let slot = transfer.slots.desc_value[value.0 as usize].ok_or_else(|| {
                        bad(
                            point.span,
                            "Store invalidation input has no actual description origin",
                        )
                    })?;
                    b.spend(state.desc[slot].len())?;
                    state.desc[slot].clone()
                }
                StoreParentRoot::BorrowedCapture(_) => {
                    let slot = invocation.carrier.ok_or_else(|| {
                        bad(point.span, "Store invalidation capture carrier absent")
                    })?;
                    b.spend(state.desc[slot].len())?;
                    state.desc[slot]
                        .iter()
                        .filter(|root| matches!(root, Root::StoreParent(_)))
                        .copied()
                        .collect()
                }
                _ => {
                    return Err(unsupported(
                        "owned Store invalidation summary mapping is under construction",
                        Some(point.span),
                    ));
                }
            };
            self.invalidate_store(transfer, state, &parents, written, point, b)?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn step(
        &self,
        transfer: &Transfer<'_>,
        state: &mut State,
        instruction: &Instruction,
        position: usize,
        block: usize,
        validate: bool,
        written: &mut BTreeSet<Root>,
        b: &mut Budget,
    ) -> Check<()> {
        let f = transfer.f;
        let point = Point {
            position,
            block,
            validate,
            span: instruction.span,
            sealed: false,
            output: instruction
                .result
                .and_then(|value| transfer.slots.desc_value[value.0 as usize]),
        };
        let facts = &self.actual.functions[f.id.0 as usize];
        let scoped = match instruction.kind {
            InstructionKind::WithForeignCallback { callback, body, .. } => {
                [Some(callback), Some(body)]
            }
            InstructionKind::WithScopedThread { body, .. }
            | InstructionKind::WithSyncCell { body, .. } => [None, Some(body)],
            _ => [None, None],
        };
        for value in scoped.into_iter().flatten() {
            self.invoke(
                transfer,
                state,
                written,
                Point {
                    sealed: true,
                    ..point
                },
                Invocation {
                    entry: effect_closure_target(
                        transfer.module,
                        &f.values[value.0 as usize],
                        instruction.span,
                    )?,
                    arguments: &[],
                    captures: Some(&facts.captures[value.0 as usize]),
                    carrier: transfer.slots.desc_value[value.0 as usize],
                    sources: transfer.slots.desc_value[value.0 as usize],
                },
                b,
            )?;
        }
        let invocation = match &instruction.kind {
            InstructionKind::Call {
                function,
                arguments,
            } => Some(Invocation {
                entry: function.0 as usize,
                arguments,
                captures: None,
                carrier: None,
                sources: None,
            }),
            InstructionKind::CallClosureShared { callee, arguments }
            | InstructionKind::CallClosureMut { callee, arguments } => {
                b.spend(facts.local_captures.len())?;
                Some(Invocation {
                    entry: effect_closure_target(
                        transfer.module,
                        &f.locals[callee.0 as usize].ty,
                        instruction.span,
                    )?,
                    arguments,
                    captures: facts.local_captures.get(callee),
                    carrier: transfer.slots.desc_local[callee.0 as usize],
                    sources: transfer.slots.desc_local[callee.0 as usize],
                })
            }
            InstructionKind::CallClosure { callee, arguments }
            | InstructionKind::CallClosureOnce { callee, arguments }
            | InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => Some(Invocation {
                entry: effect_closure_target(
                    transfer.module,
                    &f.values[callee.0 as usize],
                    instruction.span,
                )?,
                arguments,
                captures: Some(&facts.captures[callee.0 as usize]),
                carrier: transfer.slots.desc_value[callee.0 as usize],
                sources: transfer.slots.desc_value[callee.0 as usize],
            }),
            _ => None,
        };
        if let Some(invocation) = invocation {
            self.invoke(transfer, state, written, point, invocation, b)?;
        }
        let InstructionKind::CommitPlaceReplace { reservation, value } = instruction.kind else {
            return Ok(());
        };
        let f = transfer.f;
        let span = instruction.span;
        let place = self.inventories[f.id.0 as usize].get(reservation, b)?;
        let source = transfer.slots.managed_value[value.0 as usize].ok_or_else(|| {
            bad(
                span,
                "replacement RHS has no actual resource responsibility",
            )
        })?;
        if validate && state.debt[source] != LIVE {
            return Err(bad(
                span,
                "replacement RHS is absent or was consumed before Commit",
            ));
        }
        let fresh = state.fresh[source];
        for target in self.actual.selected(f, place, b)? {
            sums::replacements::at_type(transfer.module, f, &target, b)?;
            match target.base {
                actual::Base::Local(local) => {
                    let shell =
                        transfer.slots.managed_local[local.0 as usize].ok_or_else(|| {
                            bad(span, "replacement root has no actual owner responsibility")
                        })?;
                    if validate && state.debt[shell] != LIVE {
                        return Err(bad(span, "replacement owner shell is absent or moved"));
                    }
                    if let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty
                        && let Some(zil_ir::PlaceProjection::Field(index)) = target.steps.first()
                        && matches!(
                            fields[*index as usize].1,
                            Type::Managed(_) | Type::Package { .. }
                        )
                    {
                        let field =
                            transfer.slots.fields(local)[*index as usize].ok_or_else(|| {
                                bad(
                                    span,
                                    "replacement owning field has no original responsibility",
                                )
                            })?;
                        if validate && state.debt[field] != LIVE {
                            return Err(bad(
                                span,
                                "replacement owning field was already moved or cleaned",
                            ));
                        }
                        if validate {
                            no_live_description(
                                state,
                                &transfer.after[block][position],
                                Root::PackageField {
                                    local,
                                    index: *index,
                                },
                                span,
                                b,
                            )?;
                        }
                        if target.steps.len() == 1 {
                            state.fresh[field] = fresh;
                        }
                    } else if target.steps.is_empty() {
                        if validate {
                            no_live_description(
                                state,
                                &transfer.after[block][position],
                                Root::Local(local),
                                span,
                                b,
                            )?;
                        }
                        // A Package shell is not a single newly-created Managed
                        // leaf. Transfer the actual RHS field provenance, while
                        // keeping every destination obligation and cleanup age.
                        if let Some(owner) = sums::replacements::package_owner(&place.ty, b)? {
                            let fields = transfer.slots.fields(local);
                            b.spend(fields.len())?;
                            if validate
                                && fields
                                    .iter()
                                    .flatten()
                                    .any(|&field| state.debt[field] != LIVE)
                            {
                                return Err(bad(
                                    span,
                                    "whole replacement requires every old field live",
                                ));
                            }
                            let field = fields.get(owner).copied().flatten().ok_or_else(|| {
                                bad(span, "whole replacement has no actual owned field debt")
                            })?;
                            let incoming = state
                                .package_fresh
                                .get(value.0 as usize)
                                .and_then(|fields| fields.get(owner))
                                .copied()
                                .ok_or_else(|| {
                                    bad(span, "whole replacement RHS field provenance absent")
                                })?;
                            state.fresh[field] = incoming;
                            transfer
                                .slots
                                .product
                                .replace_whole(local, value, state, b)?;
                        }
                        state.fresh[shell] = fresh;
                    }
                    // Preserve shell debt, initialization, partial siblings
                    // and lexical cleanup age; the old value is replaced once.
                }
                actual::Base::Input(p) | actual::Base::InputCapture(p, _) => {
                    self.retire(transfer, state, &target, point, b)?;
                    b.spend(written.len().saturating_add(1))?;
                    written.insert(Root::Parameter(p));
                }
                actual::Base::Environment(c) => {
                    self.retire(transfer, state, &target, point, b)?;
                    if f.environment.is_some_and(|id| {
                        transfer.module.closures[id.0 as usize].kind == CallableKind::Fn
                    }) {
                        return Err(bad(
                            span,
                            "shared Fn entry cannot replace through a captured exclusive address",
                        ));
                    }
                    b.spend(written.len().saturating_add(1))?;
                    written.insert(Root::BorrowedCapture(c));
                }
                actual::Base::Element(element) => {
                    let parent =
                        transfer.slots.desc_value[element.0 as usize].ok_or_else(|| {
                            bad(
                                span,
                                "Store element has no actual parent responsibility origin",
                            )
                        })?;
                    if validate {
                        description_live(state, transfer.slots, parent, span, b)?;
                    }
                    self.invalidate_store(transfer, state, &state.desc[parent], written, point, b)?;
                    // Store retains exactly its original complete initialized
                    // prefix debt. No field debt or fresh permission is added.
                }
            }
        }
        transfer.consume(state, value, span, validate)
    }
}

#[cfg(test)]
mod tests;

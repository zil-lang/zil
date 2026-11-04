//! Replacement freshness is solved by rerunning this verifier's actual Content
//! transfer, independently of Analyze and independently of ordinary write rows.
use super::*;
use crate::place_replacements::{Inventory, actual};
use std::cell::RefCell;
type Rows = BTreeMap<actual::Target, Content>;
struct Invocation<'a> {
    entry: usize,
    arguments: &'a [ValueId],
    captures: Option<&'a actual::Captures>,
}
pub(super) struct Context<'a> {
    actual: &'a actual::Program,
    places: &'a [Inventory],
    summaries: &'a [Rows],
    written: RefCell<Rows>,
}
fn invalid(text: &str) -> Diagnostic {
    bad(None, text)
}
fn lookup(b: &mut Budget, count: usize) -> Check<()> {
    b.spend(
        count
            .checked_ilog2()
            .map_or(1, |n| n as usize + 2)
            .saturating_mul(12),
    )
}
fn merge_leaf(to: &mut Content, from: &Content, b: &mut Budget) -> Check<bool> {
    to.cost(b)?;
    from.cost(b)?;
    let (Content::Leaf(a), Content::Leaf(c)) = (&*to, from) else {
        return Err(invalid(
            "replacement needs actual complete managed leaf content",
        ));
    };
    let widths = |sources: &BTreeSet<Source>| -> Check<usize> {
        let mut total = 0usize;
        for source in sources {
            let width = match source {
                Source::Input(_, p) | Source::ReturnedInput(_, p) => p.len().saturating_add(1),
                _ => 1,
            };
            total = total
                .checked_add(width)
                .ok_or_else(|| limit("replacement content comparison width overflow"))?;
        }
        Ok(total)
    };
    let work = widths(a)?
        .checked_mul(c.len().saturating_add(1))
        .and_then(|n| {
            widths(c)
                .ok()
                .and_then(|m| m.checked_mul(a.len().saturating_add(1)))
                .and_then(|m| n.checked_add(m))
        })
        .ok_or_else(|| limit("replacement content comparison work overflow"))?;
    b.spend(work)?;
    to.merge(from, b)
}
fn merge_row(
    rows: &mut Rows,
    target: &actual::Target,
    value: &Content,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(rows.len())?;
    let mut work = target
        .steps
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(rows.len().saturating_add(1)))
        .ok_or_else(|| limit("replacement effect key work overflow"))?;
    for old in rows.keys() {
        work = work
            .checked_add(old.steps.len().saturating_add(1))
            .ok_or_else(|| limit("replacement effect route width overflow"))?;
    }
    b.spend(work)?;
    if let Some(old) = rows.get_mut(target) {
        merge_leaf(old, value, b)
    } else {
        b.spend(target.steps.len().saturating_add(1))?;
        value.cost(b)?;
        rows.insert(target.clone(), value.clone());
        Ok(true)
    }
}
pub(crate) fn at_type<'a>(
    module: &'a TypedModule,
    f: &'a TypedFunction,
    target: &actual::Target,
    b: &mut Budget,
) -> Check<&'a Type> {
    let ty = match target.base {
        actual::Base::Local(local) => &f.locals[local.0 as usize].ty,
        actual::Base::Input(p) => &f.locals[f.parameters[p as usize].0 as usize].ty,
        actual::Base::Environment(c) => {
            &module.closures[f
                .environment
                .ok_or_else(|| invalid("replacement capture has no actual environment"))?
                .0 as usize]
                .captures[c as usize]
        }
        actual::Base::InputCapture(p, c) => {
            let ty = &f.locals[f.parameters[p as usize].0 as usize].ty;
            let ty = if let Type::Address { inner, .. } = ty {
                inner.as_ref()
            } else {
                ty
            };
            let Type::Closure(id) = ty else {
                return Err(invalid("replacement input capture has no actual closure"));
            };
            &module.closures[id.0 as usize].captures[c as usize]
        }
        actual::Base::Element(value) => {
            if !matches!(target.steps.as_slice(), [PlaceProjection::Field(_)]) {
                return Err(unsupported(
                    "Store replacement requires a direct complete owning field",
                    None,
                ));
            }
            &f.values[value.0 as usize]
        }
    };
    let mut ty = if matches!(target.base, actual::Base::Local(_)) {
        ty
    } else {
        let Type::Address {
            mutable: true,
            inner,
        } = ty
        else {
            return Err(invalid(
                "replacement needs an actual mutable input/capture address",
            ));
        };
        inner.as_ref()
    };
    if !target.mutable {
        return Err(invalid("replacement cannot promote a shared actual source"));
    }
    b.spend(target.steps.len().saturating_add(1))?;
    for step in &target.steps {
        ty = match (step, ty) {
            (PlaceProjection::Field(i), Type::Tuple(fs)) => fs.get(*i as usize),
            (
                PlaceProjection::Field(i),
                Type::Package { fields, .. } | Type::Record { fields, .. },
            ) => fields.get(*i as usize).map(|(_, t)| t),
            (PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| invalid("replacement actual route crosses a view or different Type"))?;
    }
    resources::charge_payload(ty, b)?;
    if !matches!(ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Record{..})&&payload.is_copy_value())
    {
        return Err(unsupported(
            "owning replacement endpoint cleanup/hold bridge is under construction",
            None,
        ));
    }
    Ok(ty)
}
fn selected<'a>(
    value: &'a Content,
    path: &[PlaceProjection],
    validate: bool,
    b: &mut Budget,
) -> Check<&'a Content> {
    b.spend(1)?;
    if matches!(value, Content::Missing(_)) {
        return Err(invalid("replacement traverses a moved owner subtree"));
    }
    let Some((step, rest)) = path.split_first() else {
        if validate && !value.complete(b)? {
            return Err(invalid(
                "replacement endpoint is not completely initialized",
            ));
        }
        return Ok(value);
    };
    let next = match (step, value) {
        (PlaceProjection::Field(i), Content::Product(fs)) => fs.get(*i as usize),
        (PlaceProjection::Variant(p), Content::Sum(vs, _)) => {
            if validate && vs.len() != 1 {
                return Err(invalid("replacement has no exact active variant"));
            }
            vs.get(&p.variant).and_then(|fs| fs.get(p.index as usize))
        }
        (PlaceProjection::ErrorArm(e), Content::Sum(vs, _)) => {
            if validate && vs.len() != 1 {
                return Err(invalid("replacement has no exact active error arm"));
            }
            vs.get(&if *e { 0 } else { 1 }).and_then(|fs| fs.first())
        }
        _ => None,
    }
    .ok_or_else(|| invalid("replacement initialization path differs from actual content"))?;
    selected(next, rest, validate, b)
}
fn update(
    value: &mut Content,
    path: &[PlaceProjection],
    incoming: &Content,
    validate: bool,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?;
    if matches!(value, Content::Missing(_)) {
        return Err(invalid("replacement writes a moved owner subtree"));
    }
    let Some((step, rest)) = path.split_first() else {
        merge_leaf(value, incoming, b)?;
        return Ok(());
    };
    let next = match (step, value) {
        (PlaceProjection::Field(i), Content::Product(fs)) => fs.get_mut(*i as usize),
        (PlaceProjection::Variant(p), Content::Sum(vs, _)) => {
            if validate && vs.len() != 1 {
                return Err(invalid("replacement lost its active variant"));
            }
            vs.get_mut(&p.variant)
                .and_then(|fs| fs.get_mut(p.index as usize))
        }
        (PlaceProjection::ErrorArm(e), Content::Sum(vs, _)) => {
            if validate && vs.len() != 1 {
                return Err(invalid("replacement lost its active error arm"));
            }
            vs.get_mut(&if *e { 0 } else { 1 })
                .and_then(|fs| fs.first_mut())
        }
        _ => None,
    }
    .ok_or_else(|| invalid("replacement content update route is absent"))?;
    update(next, rest, incoming, validate, b)
}
impl Context<'_> {
    fn apply(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        target: &actual::Target,
        value: &Content,
        validate: bool,
        b: &mut Budget,
    ) -> Check<()> {
        at_type(flow.module, flow.f, target, b)?;
        match target.base {
            actual::Base::Local(local) => {
                if validate && state.initialized[local.0 as usize] != 2 {
                    return Err(invalid("replacement owner shell is not initialized"));
                }
                selected(&state.locals[local.0 as usize], &target.steps, validate, b)?;
                state.locals[local.0 as usize].cost(b)?;
                update(
                    Arc::make_mut(&mut state.locals[local.0 as usize]),
                    &target.steps,
                    value,
                    validate,
                    b,
                )?;
            }
            // An actual borrowed Store element is complete and retained by the
            // parent Store prefix. Replacement neither constructs an element
            // nor exports a fresh Content claim for later Store pop.
            actual::Base::Element(_) => {}
            _ => {
                if matches!(target.base, actual::Base::Environment(_))
                    && flow.f.environment.is_some_and(|id| {
                        flow.module.closures[id.0 as usize].kind == CallableKind::Fn
                    })
                {
                    return Err(invalid(
                        "shared Fn entry cannot replace through a captured exclusive address",
                    ));
                }
                merge_row(&mut self.written.borrow_mut(), target, value, b)?;
            }
        }
        Ok(())
    }
    fn call(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        invocation: Invocation<'_>,
        validate: bool,
        b: &mut Budget,
    ) -> Check<()> {
        let Invocation {
            entry,
            arguments,
            captures,
        } = invocation;
        let rows = &self.summaries[entry];
        b.spend(rows.len())?;
        for (target, payload) in rows {
            b.spend(target.steps.len().saturating_add(1))?;
            let targets = actual::substitute(
                &BTreeSet::from([target.clone()]),
                arguments,
                captures,
                &self.actual.functions[flow.f.id.0 as usize],
                b,
            )?;
            if targets.is_empty() {
                return Err(invalid(
                    "replacement call has no actual selected destination",
                ));
            }
            let incoming = flow.instantiate(payload, arguments, state, b)?;
            for target in targets {
                self.apply(flow, state, &target, &incoming, validate, b)?;
            }
        }
        Ok(())
    }
    pub(super) fn step(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        i: &Instruction,
        validate: bool,
        b: &mut Budget,
    ) -> Check<Option<Content>> {
        let f = flow.f;
        let facts = &self.actual.functions[f.id.0 as usize];
        match &i.kind {
            InstructionKind::ReservePlaceLocal { reservation, .. }
            | InstructionKind::ReservePlaceAddress { reservation, .. } => {
                let place = self.places[f.id.0 as usize].get(*reservation, b)?;
                for target in self.actual.selected(f, place, b)? {
                    let ty = at_type(flow.module, f, &target, b)?;
                    resources::charge_payload(ty, b)?;
                    if ty != &place.ty {
                        return Err(invalid("reservation Type differs from actual endpoint"));
                    }
                    if validate && let actual::Base::Local(local) = target.base {
                        if state.initialized[local.0 as usize] != 2 {
                            return Err(invalid("reservation pins an uninitialized owner"));
                        }
                        selected(&state.locals[local.0 as usize], &target.steps, true, b)?;
                    }
                }
            }
            InstructionKind::CommitPlaceReplace { reservation, value } => {
                let place = self.places[f.id.0 as usize].get(*reservation, b)?;
                let incoming = state.values[value.0 as usize].duplicate(b)?;
                for target in self.actual.selected(f, place, b)? {
                    self.apply(flow, state, &target, &incoming, validate, b)?;
                }
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                let callee = &flow.module.functions[function.0 as usize];
                if (callee.callable_contract.is_some() || callee.callable_return_contract.is_some())
                    && !self.summaries[function.0 as usize].is_empty()
                {
                    return Err(unsupported(
                        "replacement through a callable helper needs a checked public replacement envelope",
                        Some(i.span),
                    ));
                }
                self.call(
                    flow,
                    state,
                    Invocation {
                        entry: function.0 as usize,
                        arguments,
                        captures: None,
                    },
                    validate,
                    b,
                )?;
            }
            InstructionKind::CallClosureShared { callee, arguments }
            | InstructionKind::CallClosureMut { callee, arguments } => {
                let entry = resources::effect_closure_target(
                    flow.module,
                    &f.locals[callee.0 as usize].ty,
                    i.span,
                )?;
                lookup(b, facts.local_captures.len())?;
                self.call(
                    flow,
                    state,
                    Invocation {
                        entry,
                        arguments,
                        captures: facts.local_captures.get(callee),
                    },
                    validate,
                    b,
                )?;
            }
            InstructionKind::CallClosure { callee, arguments }
            | InstructionKind::CallClosureOnce { callee, arguments }
            | InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => {
                let entry = resources::effect_closure_target(
                    flow.module,
                    &f.values[callee.0 as usize],
                    i.span,
                )?;
                self.call(
                    flow,
                    state,
                    Invocation {
                        entry,
                        arguments,
                        captures: Some(&facts.captures[callee.0 as usize]),
                    },
                    validate,
                    b,
                )?;
            }
            InstructionKind::WithForeignCallback { callback, body, .. } => {
                for value in [*callback, *body] {
                    let entry = resources::effect_closure_target(
                        flow.module,
                        &f.values[value.0 as usize],
                        i.span,
                    )?;
                    self.call(
                        flow,
                        state,
                        Invocation {
                            entry,
                            arguments: &[],
                            captures: Some(&facts.captures[value.0 as usize]),
                        },
                        validate,
                        b,
                    )?;
                }
            }
            InstructionKind::WithScopedThread { body, .. }
            | InstructionKind::WithSyncCell { body, .. } => {
                let entry = resources::effect_closure_target(
                    flow.module,
                    &f.values[body.0 as usize],
                    i.span,
                )?;
                self.call(
                    flow,
                    state,
                    Invocation {
                        entry,
                        arguments: &[],
                        captures: Some(&facts.captures[body.0 as usize]),
                    },
                    validate,
                    b,
                )?;
            }
            _ => {}
        }
        Ok(None)
    }
}
pub(super) fn check(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    b: &mut Budget,
) -> Check<Proofs> {
    let actual = actual::Program::reconstruct(module, graphs, b)?;
    b.spend(module.functions.len().saturating_mul(3))?;
    let mut places = Vec::with_capacity(module.functions.len());
    let mut result = Vec::with_capacity(module.functions.len());
    let mut effects = vec![Rows::new(); module.functions.len()];
    for (f, graph) in module.functions.iter().zip(graphs) {
        let inventory = Inventory::reconstruct(module, f, b)?;
        if f.foreign.is_none() {
            inventory.activation_entries(f, graph, b)?;
        }
        places.push(inventory);
        result.push(content(&f.return_type, None, &mut Vec::new(), true, b)?);
    }
    loop {
        let mut changed = false;
        for (f, graph) in module.functions.iter().zip(graphs) {
            b.spend(1)?;
            if f.foreign.is_some() {
                continue;
            }
            let context = Context {
                actual: &actual,
                places: &places,
                summaries: &effects,
                written: RefCell::new(Rows::new()),
            };
            let (found, _) =
                run_with_replacements(module, f, graph, &result, false, b, Some(&context))?;
            let rows = context.written.into_inner();
            changed |= result[f.id.0 as usize].merge(&found, b)?;
            b.spend(rows.len())?;
            for (target, payload) in rows {
                changed |= merge_row(&mut effects[f.id.0 as usize], &target, &payload, b)?;
            }
        }
        if !changed {
            break;
        }
    }
    let mut rows = Vec::with_capacity(module.functions.len());
    for (f, graph) in module.functions.iter().zip(graphs) {
        if f.foreign.is_some() {
            rows.push(Vec::new());
            continue;
        }
        let context = Context {
            actual: &actual,
            places: &places,
            summaries: &effects,
            written: RefCell::new(Rows::new()),
        };
        rows.push(run_with_replacements(module, f, graph, &result, true, b, Some(&context))?.1);
    }
    b.spend(effects.len())?;
    let mut replacement_writes = Vec::with_capacity(effects.len());
    b.spend(effects.len())?;
    let mut replacement_targets = Vec::with_capacity(effects.len());
    for effect in effects {
        let mut writes = BTreeSet::new();
        let mut targets = Vec::new();
        b.spend(effect.len())?;
        for (target, _) in effect {
            let root = match target.base {
                actual::Base::Input(p) | actual::Base::InputCapture(p, _) => {
                    Some(resources::Root::Parameter(p))
                }
                actual::Base::Environment(c) => Some(resources::Root::BorrowedCapture(c)),
                _ => None,
            };
            if let Some(root) = root {
                lookup(b, writes.len())?;
                writes.insert(root);
                b.spend(1)?;
                targets.push(target);
            }
        }
        replacement_writes.push(writes);
        replacement_targets.push(targets);
    }
    Ok(Proofs {
        rows,
        replacement_writes,
        replacement_targets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_content_selects_and_updates_only_the_actual_error_arm() {
        for is_error in [false, true] {
            let limits = Limits::default();
            let mut budget = Budget::new(&limits);
            let original = Content::Leaf(BTreeSet::from([Source::Fresh]));
            let incoming = Content::Leaf(BTreeSet::from([Source::Input(0, vec![])]));
            // Use this kernel's actual MakeErrorUnion convention. Analyze's
            // private Node convention is intentionally not consulted here.
            let mut value = Content::Sum(
                BTreeMap::from([(if is_error { 0 } else { 1 }, vec![original.clone()])]),
                None,
            );
            let path = [PlaceProjection::ErrorArm(is_error)];
            let wrong = [PlaceProjection::ErrorArm(!is_error)];
            assert_eq!(
                selected(&value, &path, true, &mut budget).unwrap(),
                &original
            );
            assert!(selected(&value, &wrong, true, &mut budget).is_err());
            let before = value.clone();
            assert!(update(&mut value, &wrong, &incoming, true, &mut budget).is_err());
            assert_eq!(value, before);
            update(&mut value, &path, &incoming, true, &mut budget).unwrap();
            assert_eq!(
                selected(&value, &path, true, &mut budget).unwrap(),
                &Content::Leaf(BTreeSet::from([Source::Fresh, Source::Input(0, vec![]),]))
            );
        }
    }
}

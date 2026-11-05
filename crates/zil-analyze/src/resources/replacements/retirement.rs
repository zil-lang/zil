//! Exact retirement/read pairs. Symbolic inputs impose caller obligations;
//! neither different input ordinals nor a may-write row proves separation.
use super::*;
pub(super) type Pairs = BTreeSet<(Target, Target)>;

pub(super) fn needed(module: &TypedModule, b: &mut Budget) -> CompileResult<bool> {
    fee(b, module.functions.len())?;
    for f in &module.functions {
        b.lookup(b.replacement_functions.len(), "retirement producer profile")?;
        if !b.replacement_functions.contains(&f.id) {
            continue;
        }
        fee(b, f.parameters.len())?;
        let external = f.environment.is_some()
            || f.parameters.iter().any(|p| {
                matches!(
                    f.locals[p.0 as usize].ty,
                    Type::Address { mutable: true, .. }
                )
            });
        if !external {
            continue;
        }
        fee(b, f.blocks.len())?;
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            if block
                .instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn external(root: origins::Root) -> bool {
    matches!(
        root,
        origins::Root::Parameter(_)
            | origins::Root::Capture(_)
            | origins::Root::ParameterCapture(..)
    )
}
fn canonical(f: &TypedFunction, place: &Target, b: &mut Budget) -> CompileResult<Target> {
    fee(b, place.path.len().saturating_add(1))?;
    let mut place = place.clone();
    place.mutable = false;
    if let origins::Root::Local(local) = place.root {
        let block = &f.blocks[f.entry.0 as usize];
        fee(b, block.instructions.len())?;
        for i in &block.instructions {
            if let InstructionKind::BindPackageCapture {
                local: bound,
                index,
            } = i.kind
            {
                if bound == local {
                    place.root = origins::Root::Capture(index);
                    break;
                }
            }
        }
    }
    Ok(place)
}
fn root_type<'a>(
    module: &'a TypedModule,
    f: &'a TypedFunction,
    root: origins::Root,
) -> CompileResult<&'a Type> {
    let ty = match root {
        origins::Root::Local(l) => &f.locals[l.0 as usize].ty,
        origins::Root::Parameter(p) => &f.locals[f.parameters[p as usize].0 as usize].ty,
        origins::Root::Capture(c) => {
            &module.closures[f
                .environment
                .ok_or_else(|| error("retirement capture environment absent"))?
                .0 as usize]
                .captures[c as usize]
        }
        origins::Root::ParameterCapture(p, c) => {
            let mut ty = &f.locals[f.parameters[p as usize].0 as usize].ty;
            if let Type::Address { inner, .. } = ty {
                ty = inner;
            }
            let Type::Closure(id) = ty else {
                return Err(error(
                    "retirement capture parameter is not an actual closure",
                ));
            };
            &module.closures[id.0 as usize].captures[c as usize]
        }
        origins::Root::StoreElement(v) => &f.values[v.0 as usize],
    };
    Ok(if let Type::Address { inner, .. } = ty {
        inner
    } else {
        ty
    })
}
// True means provably apart. Same-root paths otherwise overlap conservatively;
// differently tagged arms do not establish disjoint resource instances.
fn apart_paths(
    module: &TypedModule,
    f: &TypedFunction,
    a: &Target,
    c: &Target,
    b: &mut Budget,
) -> CompileResult<bool> {
    let mut ty = root_type(module, f, a.root)?;
    fee(
        b,
        a.path.len().saturating_add(c.path.len()).saturating_add(1),
    )?;
    for (left, right) in a.path.iter().zip(&c.path) {
        if left != right {
            return Ok(match (left, right, ty) {
                (
                    PlaceProjection::Field(x),
                    PlaceProjection::Field(y),
                    Type::Package { fields, .. } | Type::Record { fields, .. },
                ) => x != y && (*x as usize) < fields.len() && (*y as usize) < fields.len(),
                (PlaceProjection::Field(x), PlaceProjection::Field(y), Type::Tuple(fields)) => {
                    x != y && (*x as usize) < fields.len() && (*y as usize) < fields.len()
                }
                (
                    PlaceProjection::Variant(x),
                    PlaceProjection::Variant(y),
                    Type::Enum { variants, .. },
                ) => {
                    x.variant == y.variant
                        && x.index != y.index
                        && variants.get(x.variant as usize).is_some_and(|(_, fs)| {
                            (x.index as usize) < fs.len() && (y.index as usize) < fs.len()
                        })
                }
                _ => false,
            });
        }
        ty = endpoint(ty, std::slice::from_ref(left), b)?;
    }
    Ok(false)
}
fn insert(rows: &mut Pairs, a: Target, c: Target, b: &mut Budget) -> CompileResult<bool> {
    fee(b, rows.len())?;
    let mut work = a.path.len().saturating_add(c.path.len()).saturating_add(2);
    for (x, y) in rows.iter() {
        work = work
            .checked_add(x.path.len().saturating_add(y.path.len()).saturating_add(2))
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Analyze,
                    "retirement pair comparison overflow",
                    None,
                )]
            })?;
    }
    b.product(
        work,
        rows.len().saturating_add(1),
        1,
        "exact retirement pair comparison",
    )?;
    Ok(rows.insert((a, c)))
}
fn require(
    program: &Program,
    module: &TypedModule,
    f: &TypedFunction,
    a: &Target,
    c: &Target,
    b: &mut Budget,
) -> CompileResult<bool> {
    let a = canonical(f, a, b)?;
    let c = canonical(f, c, b)?;
    if a.root == c.root {
        if apart_paths(module, f, &a, &c, b)? {
            return Ok(false);
        }
        return Err(error(
            "actual replacement may retire a resource instance whose description is required later",
        ));
    }
    if matches!(a.root, origins::Root::Local(_)) || matches!(c.root, origins::Root::Local(_)) {
        return Ok(false);
    }
    if external(a.root) || external(c.root) {
        return insert(
            &mut program.retirement_pairs.borrow_mut()[f.id.0 as usize],
            a,
            c,
            b,
        );
    }
    // Two unknown element windows are never apart merely because their SSA
    // producers differ. Store's dedicated instance pass remains authoritative.
    Err(error(
        "retirement/read storage instances have no independently proven separation",
    ))
}
fn descriptor_routes(
    program: &Program,
    flow: &Flow<'_>,
    place: Place,
    point: CheckPoint<'_>,
    b: &mut Budget,
) -> CompileResult<origins::Places> {
    let f = flow.function;
    let facts = &program.routes[f.id.0 as usize];
    let position = point
        .position
        .ok_or_else(|| error("retirement has no exact instruction position"))?;
    b.lookup(facts.positions.len(), "retirement point source state")?;
    let state = facts
        .positions
        .get(&position)
        .ok_or_else(|| error("retirement actual point state absent"))?;
    let (ty, values, slots) = match place {
        Place::Value(v) => (
            &f.values[v.0 as usize],
            Some(&facts.values[v.0 as usize]),
            Some(&facts.slots[v.0 as usize]),
        ),
        Place::Local(l) => (
            &f.locals[l.0 as usize].ty,
            state.addresses.get(&l),
            state.environments.get(&l),
        ),
        Place::Capture(c) => {
            let ty = &flow.module.closures[f
                .environment
                .ok_or_else(|| error("live descriptor capture has no environment"))?
                .0 as usize]
                .captures[c as usize];
            if matches!(ty, Type::Desc(_)) {
                fee(b, 1)?;
                return Ok(BTreeSet::from([Target {
                    root: origins::Root::Capture(c),
                    path: vec![],
                    mutable: false,
                }]));
            }
            return Ok(BTreeSet::new());
        }
        _ => return Ok(BTreeSet::new()),
    };
    let mut out = origins::Places::new();
    if matches!(ty, Type::Desc(_)) {
        let values =
            values.ok_or_else(|| error("live descriptor has no actual retirement origin"))?;
        fee(b, values.len())?;
        for value in values {
            fee(b, value.path.len().saturating_add(1))?;
            out.insert(value.clone());
        }
        if out.is_empty() {
            return Err(error("live descriptor has no actual retirement origin"));
        }
    } else {
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        if let Type::Closure(id) = ty {
            let captures = &flow.module.closures[id.0 as usize].captures;
            fee(b, captures.len())?;
            for (index, ty) in captures.iter().enumerate() {
                if matches!(ty, Type::Desc(_)) {
                    let slots = slots.ok_or_else(|| {
                        error("live closure descriptor has no actual environment slots")
                    })?;
                    b.lookup(slots.len(), "retirement actual descriptor capture")?;
                    let row = slots.get(&(index as u32)).ok_or_else(|| {
                        error("live closure descriptor capture has no actual source")
                    })?;
                    fee(b, row.len())?;
                    if row.is_empty() {
                        return Err(error(
                            "live closure descriptor capture has no actual source",
                        ));
                    }
                    for value in row {
                        fee(b, value.path.len().saturating_add(1))?;
                        out.insert(value.clone());
                    }
                }
            }
        }
    }
    Ok(out)
}
fn local_fallback(
    flow: &Flow<'_>,
    state: &State,
    target: &Target,
    point: CheckPoint<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    // No mutable external Commit producer exists in this module. Keep the
    // established local-source path, without constructing a Desc/call graph.
    let origins::Root::Local(local) = target.root else {
        return Ok(());
    };
    let retired = if let (Type::Package { .. }, Some(PlaceProjection::Field(index))) = (
        &flow.function.locals[local.0 as usize].ty,
        target.path.first(),
    ) {
        flow.package_source(local, *index, b)?
    } else {
        Source::Head(local)
    };
    let live = point
        .live
        .ok_or_else(|| error("retirement lacks live descriptions"))?;
    fee(b, live.len())?;
    for place in live {
        if matches!(place,Place::Value(v)if Some(*v)==point.output) {
            continue;
        }
        let ty = match place {
            Place::Local(l) => &flow.function.locals[l.0 as usize].ty,
            Place::Value(v) => &flow.function.values[v.0 as usize],
            _ => continue,
        };
        if !matches!(ty, Type::Desc(_) | Type::Closure(_))
            && !matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Closure(_)))
        {
            continue;
        }
        let fact = copied(state, *place, b)?;
        fee(b, fact.sources.len())?;
        if fact.sources.iter().any(|source|source.parent()==retired||matches!((retired,source.parent()),(Source::Head(owner),Source::PackageField{local,..}|Source::OwnedPackageField{local,..}|Source::ArrayElement{local,..})if owner==local)){
            return Err(error("actual replacement may retire a resource instance whose description is required later"));
        }
    }
    Ok(())
}
pub(super) fn check(
    flow: &Flow<'_>,
    state: &State,
    target: &Target,
    point: CheckPoint<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    if !point.validate || matches!(target.root, origins::Root::StoreElement(_)) {
        return Ok(());
    }
    let program = flow
        .replacements
        .ok_or_else(|| error("retirement has no actual replacement program"))?;
    if !program.exact_retirement {
        return local_fallback(flow, state, target, point, b);
    }
    let live = point
        .live
        .ok_or_else(|| error("retirement lacks live descriptions"))?;
    fee(b, live.len())?;
    for place in live {
        if matches!(place,Place::Value(v)if Some(*v)==point.output) {
            continue;
        }
        for source in descriptor_routes(program, flow, *place, point, b)? {
            require(program, flow.module, flow.function, target, &source, b)?;
        }
    }
    Ok(())
}
impl Program {
    pub(crate) fn close_retirement_pairs(
        &self,
        module: &TypedModule,
        graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !self.exact_retirement {
            return Ok(());
        }
        fee(b, self.retirement_pairs.borrow().len())?;
        if self.retirement_pairs.borrow().iter().all(Pairs::is_empty) {
            return Ok(());
        }
        loop {
            let mut changed = false;
            fee(b, module.functions.len())?;
            for (f, (_, reachable)) in module.functions.iter().zip(graphs) {
                let facts = &self.routes[f.id.0 as usize];
                fee(b, f.blocks.len())?;
                for (block_index, block) in f.blocks.iter().enumerate() {
                    if !reachable[block_index] {
                        continue;
                    }
                    fee(b, block.instructions.len())?;
                    for (position, i) in block.instructions.iter().enumerate() {
                        let state = facts.positions.get(&(block_index, position));
                        let mut calls = Vec::new();
                        match &i.kind {
                            InstructionKind::Call {
                                function,
                                arguments,
                            } => calls.push((function.0 as usize, arguments.as_slice(), None)),
                            InstructionKind::CallClosureShared { callee, arguments }
                            | InstructionKind::CallClosureMut { callee, arguments } => {
                                let entry = effect_entry(module, &f.locals[callee.0 as usize].ty)?;
                                let env = state.and_then(|s| s.environments.get(callee));
                                calls.push((entry, arguments.as_slice(), env));
                            }
                            InstructionKind::CallClosure { callee, arguments }
                            | InstructionKind::CallClosureOnce { callee, arguments }
                            | InstructionKind::CallClosureBorrowed {
                                callee, arguments, ..
                            } => calls.push((
                                effect_entry(module, &f.values[callee.0 as usize])?,
                                arguments.as_slice(),
                                Some(&facts.slots[callee.0 as usize]),
                            )),
                            InstructionKind::WithScopedThread { body, .. }
                            | InstructionKind::WithSyncCell { body, .. } => calls.push((
                                effect_entry(module, &f.values[body.0 as usize])?,
                                &[][..],
                                Some(&facts.slots[body.0 as usize]),
                            )),
                            InstructionKind::WithForeignCallback { callback, body, .. } => {
                                for value in [*callback, *body] {
                                    calls.push((
                                        effect_entry(module, &f.values[value.0 as usize])?,
                                        &[][..],
                                        Some(&facts.slots[value.0 as usize]),
                                    ));
                                }
                            }
                            _ => {}
                        }
                        for (entry, args, env) in calls {
                            let table = self.retirement_pairs.borrow();
                            let pairs = &table[entry];
                            if !pairs.is_empty()
                                && matches!(
                                    i.kind,
                                    InstructionKind::WithForeignCallback { .. }
                                        | InstructionKind::WithScopedThread { .. }
                                        | InstructionKind::WithSyncCell { .. }
                                )
                            {
                                return Err(vec![Diagnostic::unsupported(
                                    Phase::Analyze,
                                    "sealed callback retirement separation needs its simultaneous checked envelope",
                                    Some(i.span),
                                )]);
                            }
                            fee(b, pairs.len())?;
                            let mut copied = Vec::with_capacity(pairs.len());
                            for (a, c) in pairs {
                                fee(
                                    b,
                                    a.path.len().saturating_add(c.path.len()).saturating_add(2),
                                )?;
                                copied.push((a.clone(), c.clone()));
                            }
                            drop(table);
                            for (a, c) in copied {
                                let left = origins::substitute(
                                    &BTreeSet::from([a]),
                                    args,
                                    env,
                                    &facts.values,
                                    &facts.slots,
                                    b,
                                )?;
                                let right = origins::substitute(
                                    &BTreeSet::from([c]),
                                    args,
                                    env,
                                    &facts.values,
                                    &facts.slots,
                                    b,
                                )?;
                                if left.is_empty() || right.is_empty() {
                                    return Err(error(
                                        "retirement pair has no grounded actual call/capture substitution",
                                    ));
                                }
                                b.product(
                                    left.len(),
                                    right.len(),
                                    1,
                                    "retirement actual call pair cross product",
                                )?;
                                for a in &left {
                                    for c in &right {
                                        changed |= require(self, module, f, a, c, b)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                for f in &module.functions {
                    let has_pairs = !self.retirement_pairs.borrow()[f.id.0 as usize].is_empty();
                    if has_pairs
                        && (f.callable_contract.is_some() || f.callable_return_contract.is_some())
                    {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "retirement separation needs a checked public callable envelope",
                            Some(f.span),
                        )]);
                    }
                    fee(b, f.blocks.len())?;
                    for block in &f.blocks {
                        fee(b, block.instructions.len())?;
                        for i in &block.instructions {
                            if let InstructionKind::FunctionRef { function } = i.kind {
                                if !self.retirement_pairs.borrow()[function.0 as usize].is_empty() {
                                    return Err(vec![Diagnostic::unsupported(
                                        Phase::Analyze,
                                        "indirect function reference cannot erase actual retirement separation obligations",
                                        Some(i.span),
                                    )]);
                                }
                            }
                        }
                    }
                }
                return Ok(());
            }
        }
    }
}

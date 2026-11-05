//! Verifier-owned address recipes. These are reconstructed from executable
//! producers, actual input/capture positions and complete inline paths. A
//! recipe is never a loan, initialization certificate or trusted producer fact.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Base {
    Local(LocalId),
    Input(u32),
    Environment(u32),
    InputCapture(u32, u32),
    Element(ValueId),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Target {
    pub(crate) base: Base,
    pub(crate) steps: Vec<PlaceProjection>,
    pub(crate) mutable: bool,
}
pub(crate) type Targets = BTreeSet<Target>;
pub(crate) type Captures = BTreeMap<u32, Targets>;
#[derive(Default)]
pub(crate) struct Function {
    pub(crate) values: Vec<Targets>,
    pub(crate) captures: Vec<Captures>,
    pub(crate) local_captures: BTreeMap<LocalId, Captures>,
}
pub(crate) struct Program {
    pub(crate) functions: Vec<Function>,
}
fn fail(text: &str) -> Diagnostic {
    bad(None, text)
}
pub(crate) fn charge_set(targets: &Targets, b: &mut Budget) -> Check<()> {
    b.spend(targets.len())?;
    for target in targets {
        b.spend(target.steps.len().saturating_add(1))?;
    }
    Ok(())
}
fn insert(to: &mut Targets, target: &Target, b: &mut Budget) -> Check<bool> {
    // Bound planning first, then every possible variable-width comparison.
    b.spend(to.len())?;
    let mut work = target
        .steps
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(to.len().saturating_add(1)))
        .ok_or_else(|| limit("actual replacement recipe comparison work overflow"))?;
    for old in to.iter() {
        work = work
            .checked_add(old.steps.len().saturating_add(1))
            .ok_or_else(|| limit("actual replacement recipe width overflow"))?;
    }
    b.spend(work)?;
    b.spend(target.steps.len().saturating_add(1))?;
    Ok(to.insert(target.clone()))
}
pub(crate) fn merge(to: &mut Targets, from: &Targets, b: &mut Budget) -> Check<bool> {
    b.spend(from.len())?;
    let mut changed = false;
    for target in from {
        changed |= insert(to, target, b)?;
    }
    Ok(changed)
}
pub(crate) fn merge_captures(to: &mut Captures, from: &Captures, b: &mut Budget) -> Check<bool> {
    b.spend(from.len())?;
    let mut changed = false;
    for (index, row) in from {
        lookup(b, to.len())?;
        changed |= merge(to.entry(*index).or_default(), row, b)?;
    }
    Ok(changed)
}
fn extend(
    from: &Targets,
    steps: &[PlaceProjection],
    permission: bool,
    b: &mut Budget,
) -> Check<Targets> {
    b.spend(from.len())?;
    let mut out = Targets::new();
    for target in from {
        let width = target
            .steps
            .len()
            .checked_add(steps.len())
            .ok_or_else(|| limit("actual replacement recipe depth overflow"))?;
        if width > b.limits.max_call_depth {
            return Err(limit("actual replacement recipe depth exceeded"));
        }
        b.spend(width.saturating_add(1))?;
        let mut next = Target {
            base: target.base,
            steps: Vec::with_capacity(width),
            mutable: target.mutable && permission,
        };
        next.steps.extend_from_slice(&target.steps);
        next.steps.extend_from_slice(steps);
        insert(&mut out, &next, b)?;
    }
    Ok(out)
}
fn environment(ty: &Type) -> Option<ClosureId> {
    match ty {
        Type::Closure(id) => Some(*id),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}
pub(crate) fn substitute(
    targets: &Targets,
    arguments: &[ValueId],
    captures: Option<&Captures>,
    function: &Function,
    b: &mut Budget,
) -> Check<Targets> {
    b.spend(targets.len())?;
    let mut result = Targets::new();
    for target in targets {
        let selected = match target.base {
            Base::Input(index) => arguments
                .get(index as usize)
                .map(|v| &function.values[v.0 as usize]),
            Base::Environment(index) => {
                if let Some(captures) = captures {
                    lookup(b, captures.len())?;
                    captures.get(&index)
                } else {
                    None
                }
            }
            Base::InputCapture(parameter, capture) => {
                if let Some(value) = arguments.get(parameter as usize) {
                    let slots = &function.captures[value.0 as usize];
                    lookup(b, slots.len())?;
                    slots.get(&capture)
                } else {
                    None
                }
            }
            Base::Local(_) | Base::Element(_) => None,
        };
        if let Some(selected) = selected {
            merge(
                &mut result,
                &extend(selected, &target.steps, target.mutable, b)?,
                b,
            )?;
        }
    }
    Ok(result)
}
impl Program {
    pub(crate) fn reconstruct(
        module: &TypedModule,
        graphs: &[FunctionGraph],
        b: &mut Budget,
    ) -> Check<Self> {
        b.spend(module.functions.len().saturating_mul(3))?;
        let mut functions = Vec::with_capacity(module.functions.len());
        let mut address_locals = Vec::with_capacity(module.functions.len());
        let mut returned = vec![Targets::new(); module.functions.len()];
        for f in &module.functions {
            b.spend(
                f.values
                    .len()
                    .saturating_mul(2)
                    .saturating_add(f.locals.len()),
            )?;
            let mut facts = Function {
                values: vec![Targets::new(); f.values.len()],
                captures: vec![Captures::new(); f.values.len()],
                local_captures: BTreeMap::new(),
            };
            let mut locals = BTreeMap::new();
            b.spend(f.parameters.len())?;
            for (index, local) in f.parameters.iter().enumerate() {
                let ty = &f.locals[local.0 as usize].ty;
                if let Type::Address { mutable, .. } = ty {
                    lookup(b, locals.len())?;
                    locals.insert(
                        *local,
                        Targets::from([Target {
                            base: Base::Input(index as u32),
                            steps: vec![],
                            mutable: *mutable,
                        }]),
                    );
                }
                if let Some(id) = environment(ty) {
                    let captures = &module.closures[id.0 as usize].captures;
                    b.spend(captures.len())?;
                    let mut slots = Captures::new();
                    for (capture, ty) in captures.iter().enumerate() {
                        if let Type::Address { mutable, .. } = ty {
                            lookup(b, slots.len())?;
                            let carrier = !matches!(
                                f.locals[local.0 as usize].ty,
                                Type::Address { mutable: false, .. }
                            );
                            slots.insert(
                                capture as u32,
                                Targets::from([Target {
                                    base: Base::InputCapture(index as u32, capture as u32),
                                    steps: vec![],
                                    mutable: *mutable && carrier,
                                }]),
                            );
                        }
                    }
                    lookup(b, facts.local_captures.len())?;
                    facts.local_captures.insert(*local, slots);
                }
            }
            functions.push(facts);
            address_locals.push(locals);
        }
        // All address stores contribute to a may-recipe. Pointwise authority,
        // initialization and parent loan liveness are checked independently.
        // Including other activations is conservative, never a new permission.
        loop {
            let mut changed = false;
            for (index, f) in module.functions.iter().enumerate() {
                for (block, reachable) in f.blocks.iter().zip(&graphs[index].reachable) {
                    b.spend(1)?;
                    if !reachable {
                        continue;
                    }
                    for instruction in &block.instructions {
                        b.spend(1)?;
                        if let Some(value) = instruction.result {
                            if matches!(f.values[value.0 as usize], Type::Address { .. }) {
                                let mut route = Targets::new();
                                match &instruction.kind {
                                    InstructionKind::BorrowLocal { local }
                                    | InstructionKind::BorrowLocalMut { local } => {
                                        let mutable = matches!(
                                            instruction.kind,
                                            InstructionKind::BorrowLocalMut { .. }
                                        ) && f.locals[local.0 as usize].mutable;
                                        b.spend(1)?;
                                        route.insert(Target {
                                            base: Base::Local(*local),
                                            steps: vec![],
                                            mutable,
                                        });
                                    }
                                    InstructionKind::BorrowPackageField {
                                        local,
                                        index,
                                        mutable,
                                    } => {
                                        b.spend(2)?;
                                        route.insert(Target {
                                            base: Base::Local(*local),
                                            steps: vec![PlaceProjection::Field(*index)],
                                            mutable: *mutable && f.locals[local.0 as usize].mutable,
                                        });
                                    }
                                    InstructionKind::BorrowPlaceLocal {
                                        local,
                                        path,
                                        mutable,
                                    } => {
                                        b.spend(path.len().saturating_add(1))?;
                                        route.insert(Target {
                                            base: Base::Local(*local),
                                            steps: path.clone(),
                                            mutable: *mutable && f.locals[local.0 as usize].mutable,
                                        });
                                    }
                                    InstructionKind::BorrowFieldLocal {
                                        local,
                                        path,
                                        mutable,
                                    } => {
                                        b.spend(path.len().saturating_add(1))?;
                                        route.insert(Target {
                                            base: Base::Local(*local),
                                            steps: path
                                                .iter()
                                                .copied()
                                                .map(PlaceProjection::Field)
                                                .collect(),
                                            mutable: *mutable && f.locals[local.0 as usize].mutable,
                                        });
                                    }
                                    InstructionKind::BorrowVariantLocal {
                                        local,
                                        path,
                                        mutable,
                                    } => {
                                        b.spend(path.len().saturating_add(1))?;
                                        route.insert(Target {
                                            base: Base::Local(*local),
                                            steps: path
                                                .iter()
                                                .copied()
                                                .map(PlaceProjection::Variant)
                                                .collect(),
                                            mutable: *mutable && f.locals[local.0 as usize].mutable,
                                        });
                                    }
                                    InstructionKind::Load(local)
                                    | InstructionKind::TakeLocal { local } => {
                                        lookup(b, address_locals[index].len())?;
                                        if let Some(source) = address_locals[index].get(local) {
                                            merge(&mut route, source, b)?;
                                        }
                                    }
                                    InstructionKind::ReadCapture { index: capture } => {
                                        let definition =
                                            f.environment.map(|id| &module.closures[id.0 as usize]);
                                        let mutable = definition.is_some_and(|env| {
                                            env.kind != CallableKind::Fn
                                                && matches!(
                                                    env.captures.get(*capture as usize),
                                                    Some(Type::Address { mutable: true, .. })
                                                )
                                        });
                                        b.spend(1)?;
                                        route.insert(Target {
                                            base: Base::Environment(*capture),
                                            steps: vec![],
                                            mutable,
                                        });
                                    }
                                    InstructionKind::ReborrowAddress { address, mutable } => {
                                        route = extend(
                                            &functions[index].values[address.0 as usize],
                                            &[],
                                            *mutable,
                                            b,
                                        )?
                                    }
                                    InstructionKind::AddressField {
                                        address,
                                        index: field,
                                    } => {
                                        route = extend(
                                            &functions[index].values[address.0 as usize],
                                            &[PlaceProjection::Field(*field)],
                                            true,
                                            b,
                                        )?
                                    }
                                    InstructionKind::BorrowPlaceAddress {
                                        address,
                                        path,
                                        mutable,
                                    } => {
                                        route = extend(
                                            &functions[index].values[address.0 as usize],
                                            path,
                                            *mutable,
                                            b,
                                        )?
                                    }
                                    InstructionKind::BorrowVariantAddress {
                                        address,
                                        path,
                                        mutable,
                                    } => {
                                        b.spend(path.len())?;
                                        let steps = path
                                            .iter()
                                            .copied()
                                            .map(PlaceProjection::Variant)
                                            .collect::<Vec<_>>();
                                        route = extend(
                                            &functions[index].values[address.0 as usize],
                                            &steps,
                                            *mutable,
                                            b,
                                        )?;
                                    }
                                    InstructionKind::OwnedStoreBorrow { .. }
                                    | InstructionKind::OwnedStoreBorrowMut { .. } => {
                                        b.spend(1)?;
                                        route.insert(Target {
                                            base: Base::Element(value),
                                            steps: vec![],
                                            mutable: matches!(
                                                instruction.kind,
                                                InstructionKind::OwnedStoreBorrowMut { .. }
                                            ),
                                        });
                                    }
                                    InstructionKind::Call {
                                        function,
                                        arguments,
                                    } => {
                                        route = substitute(
                                            &returned[function.0 as usize],
                                            arguments,
                                            None,
                                            &functions[index],
                                            b,
                                        )?
                                    }
                                    InstructionKind::CallClosureShared { callee, arguments }
                                    | InstructionKind::CallClosureMut { callee, arguments } => {
                                        let id=environment(&f.locals[callee.0 as usize].ty).ok_or_else(||fail("address recipe callback local has no actual entry"))?;
                                        lookup(b, functions[index].local_captures.len())?;
                                        route = substitute(
                                            &returned[module.closures[id.0 as usize].function.0
                                                as usize],
                                            arguments,
                                            functions[index].local_captures.get(callee),
                                            &functions[index],
                                            b,
                                        )?;
                                    }
                                    InstructionKind::CallClosureBorrowed {
                                        callee,
                                        arguments,
                                        ..
                                    } => {
                                        let id=environment(&f.values[callee.0 as usize]).ok_or_else(||fail("address recipe callback value has no actual entry"))?;
                                        route = substitute(
                                            &returned[module.closures[id.0 as usize].function.0
                                                as usize],
                                            arguments,
                                            Some(&functions[index].captures[callee.0 as usize]),
                                            &functions[index],
                                            b,
                                        )?;
                                    }
                                    _ => {}
                                }
                                let semantic = matches!(
                                    f.values[value.0 as usize],
                                    Type::Address { mutable: true, .. }
                                );
                                let route = extend(&route, &[], semantic, b)?;
                                changed |= merge(
                                    &mut functions[index].values[value.0 as usize],
                                    &route,
                                    b,
                                )?;
                            }
                            if environment(&f.values[value.0 as usize]).is_some() {
                                let mut slots = Captures::new();
                                match &instruction.kind {
                                    InstructionKind::MakeClosure { closure, captures } => {
                                        b.spend(captures.len())?;
                                        for (capture, (actual, ty)) in captures
                                            .iter()
                                            .zip(&module.closures[closure.0 as usize].captures)
                                            .enumerate()
                                        {
                                            if matches!(ty, Type::Address { .. }) {
                                                lookup(b, slots.len())?;
                                                merge(
                                                    slots.entry(capture as u32).or_default(),
                                                    &functions[index].values[actual.0 as usize],
                                                    b,
                                                )?;
                                            }
                                        }
                                    }
                                    InstructionKind::Load(local)
                                    | InstructionKind::TakeLocal { local }
                                    | InstructionKind::BorrowLocal { local }
                                    | InstructionKind::BorrowLocalMut { local } => {
                                        lookup(b, functions[index].local_captures.len())?;
                                        if let Some(row) =
                                            functions[index].local_captures.get(local)
                                        {
                                            merge_captures(&mut slots, row, b)?;
                                        }
                                    }
                                    InstructionKind::ReborrowAddress { address, .. } => {
                                        merge_captures(
                                            &mut slots,
                                            &functions[index].captures[address.0 as usize],
                                            b,
                                        )?;
                                    }
                                    _ => {}
                                }
                                changed |= merge_captures(
                                    &mut functions[index].captures[value.0 as usize],
                                    &slots,
                                    b,
                                )?;
                            }
                        }
                        if let InstructionKind::Store { local, value } = instruction.kind {
                            lookup(b, address_locals[index].len())?;
                            let mut row = Targets::new();
                            merge(&mut row, &functions[index].values[value.0 as usize], b)?;
                            if !row.is_empty() {
                                changed |= merge(
                                    address_locals[index].entry(local).or_default(),
                                    &row,
                                    b,
                                )?;
                            }
                            let mut slots = Captures::new();
                            merge_captures(
                                &mut slots,
                                &functions[index].captures[value.0 as usize],
                                b,
                            )?;
                            if !slots.is_empty() {
                                lookup(b, functions[index].local_captures.len())?;
                                changed |= merge_captures(
                                    functions[index].local_captures.entry(local).or_default(),
                                    &slots,
                                    b,
                                )?;
                            }
                        }
                    }
                    if matches!(f.return_type, Type::Address { .. })
                        && let Terminator::Return(Some(value)) = block.terminator
                    {
                        charge_set(&functions[index].values[value.0 as usize], b)?;
                        for target in &functions[index].values[value.0 as usize] {
                            if matches!(
                                target.base,
                                Base::Input(_) | Base::Environment(_) | Base::InputCapture(..)
                            ) {
                                b.spend(target.steps.len().saturating_add(1))?;
                                let mut target = target.clone();
                                target.mutable &=
                                    matches!(f.return_type, Type::Address { mutable: true, .. });
                                changed |= insert(&mut returned[index], &target, b)?;
                            }
                        }
                    }
                }
            }
            if !changed {
                return Ok(Self { functions });
            }
        }
    }
    pub(crate) fn selected(
        &self,
        f: &TypedFunction,
        place: &Place,
        b: &mut Budget,
    ) -> Check<Targets> {
        let targets = match place.origin {
            Origin::Local(local) => {
                b.spend(place.steps.len().saturating_add(1))?;
                Targets::from([Target {
                    base: Base::Local(local),
                    steps: place.steps.clone(),
                    mutable: f.locals[local.0 as usize].mutable,
                }])
            }
            Origin::Address(value) => extend(
                &self.functions[f.id.0 as usize].values[value.0 as usize],
                &place.steps,
                true,
                b,
            )?,
        };
        if targets.is_empty() {
            return Err(fail(
                "replacement has no independently reconstructed actual target",
            ));
        }
        // Exact whole-Package/Enum transfer retires one concrete old instance.
        // A may-target set cannot strongly replace every alternative. The
        // original Managed endpoint lane retains its conservative transfer.
        if matches!(place.ty, Type::Package { .. } | Type::Enum { .. }) && targets.len() != 1 {
            return Err(unsupported(
                "whole Package/Enum replacement requires one independently reconstructed actual target",
                None,
            ));
        }
        Ok(targets)
    }
}

//! Actual owner positions reconstructed from address-producing instructions.
//! Routes describe storage, never permissions; the loan/init passes prove those.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Root {
    Local(LocalId),
    Parameter(u32),
    Capture(u32),
    ParameterCapture(u32, u32),
    // An explicit Store borrow is a stable-storage boundary, not an inline
    // field of the descriptor/header. Its source SSA remains independently held.
    StoreElement(ValueId),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Place {
    pub(crate) root: Root,
    pub(crate) path: Vec<PlaceProjection>,
    pub(crate) mutable: bool,
}
pub(crate) type Places = BTreeSet<Place>;
pub(crate) type Slots = BTreeMap<u32, Places>;
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) addresses: BTreeMap<LocalId, Places>,
    pub(crate) environments: BTreeMap<LocalId, Slots>,
}
pub(crate) struct Facts {
    pub(crate) values: Vec<Places>,
    pub(crate) slots: Vec<Slots>,
    pub(crate) local_slots: BTreeMap<LocalId, Slots>,
    pub(crate) positions: BTreeMap<(usize, usize), State>,
    returned: Places,
}
fn work(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "replacement actual owner routes")
}
fn overflow() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Analyze,
        "replacement owner route work overflow",
        None,
    )]
}
fn place_cost(to: &Places, place: &Place, b: &mut Budget) -> CompileResult<()> {
    // Reserve the set scan before measuring variable-width comparisons.
    work(b, to.len())?;
    let widths = to
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(p.path.len().saturating_add(1)))
        .ok_or_else(overflow)?;
    let comparisons = place
        .path
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(to.len().saturating_add(1)))
        .and_then(|n| n.checked_add(widths))
        .ok_or_else(overflow)?;
    work(b, comparisons)
}
fn insert(to: &mut Places, place: &Place, b: &mut Budget) -> CompileResult<bool> {
    place_cost(to, place, b)?;
    work(b, place.path.len().saturating_add(1))?;
    Ok(to.insert(place.clone()))
}
fn join(to: &mut Places, from: &Places, b: &mut Budget) -> CompileResult<bool> {
    work(b, from.len())?;
    let mut changed = false;
    for place in from {
        changed |= insert(to, place, b)?;
    }
    Ok(changed)
}
fn slots_join(to: &mut Slots, from: &Slots, b: &mut Budget) -> CompileResult<bool> {
    work(b, from.len())?;
    let mut changed = false;
    for (index, places) in from {
        b.lookup(to.len(), "replacement capture route slot")?;
        changed |= join(to.entry(*index).or_default(), places, b)?;
    }
    Ok(changed)
}
fn state_cost(state: &State, b: &mut Budget) -> CompileResult<()> {
    work(
        b,
        state
            .addresses
            .len()
            .saturating_add(state.environments.len()),
    )?;
    for places in state.addresses.values() {
        work(b, places.len())?;
        for place in places {
            work(b, place.path.len().saturating_add(1))?;
        }
    }
    for slots in state.environments.values() {
        work(b, slots.len())?;
        for places in slots.values() {
            work(b, places.len())?;
            for place in places {
                work(b, place.path.len().saturating_add(1))?;
            }
        }
    }
    Ok(())
}
fn state_join(to: &mut State, from: &State, b: &mut Budget) -> CompileResult<()> {
    work(
        b,
        from.addresses.len().saturating_add(from.environments.len()),
    )?;
    for (local, places) in &from.addresses {
        b.lookup(to.addresses.len(), "replacement owner local merge")?;
        join(to.addresses.entry(*local).or_default(), places, b)?;
    }
    for (local, slots) in &from.environments {
        b.lookup(to.environments.len(), "replacement environment local merge")?;
        slots_join(to.environments.entry(*local).or_default(), slots, b)?;
    }
    Ok(())
}
pub(crate) fn project(
    from: &Places,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Places> {
    work(b, from.len())?;
    let mut out = Places::new();
    for place in from {
        let len = place
            .path
            .len()
            .checked_add(path.len())
            .ok_or_else(overflow)?;
        if len > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "replacement actual owner route depth exceeded",
                None,
            )]);
        }
        work(b, len.saturating_add(1))?;
        let mut projected = Place {
            root: place.root,
            mutable: place.mutable,
            path: Vec::with_capacity(len),
        };
        projected.path.extend_from_slice(&place.path);
        projected.path.extend_from_slice(path);
        insert(&mut out, &projected, b)?;
    }
    Ok(out)
}
fn local_mutable(f: &TypedFunction, instruction: &Instruction, local: LocalId) -> bool {
    f.locals[local.0 as usize].mutable
        && match instruction.kind {
            InstructionKind::BorrowLocalMut { .. } => true,
            InstructionKind::BorrowPackageField { mutable, .. }
            | InstructionKind::BorrowFieldLocal { mutable, .. }
            | InstructionKind::BorrowPlaceLocal { mutable, .. }
            | InstructionKind::BorrowVariantLocal { mutable, .. } => mutable,
            _ => false,
        }
}
fn closure_id(ty: &Type) -> Option<ClosureId> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if let Type::Closure(id) = ty {
        Some(*id)
    } else {
        None
    }
}
pub(crate) fn substitute(
    returns: &Places,
    arguments: &[ValueId],
    environment: Option<&Slots>,
    values: &[Places],
    slots: &[Slots],
    b: &mut Budget,
) -> CompileResult<Places> {
    let mut out = Places::new();
    work(b, returns.len())?;
    for returned in returns {
        let actual = match returned.root {
            Root::Parameter(index) => arguments.get(index as usize).map(|v| &values[v.0 as usize]),
            Root::Capture(index) => {
                if let Some(environment) = environment {
                    b.lookup(environment.len(), "replacement return capture lookup")?;
                    environment.get(&index)
                } else {
                    None
                }
            }
            Root::ParameterCapture(parameter, capture) => {
                if let Some(value) = arguments.get(parameter as usize) {
                    let slots = &slots[value.0 as usize];
                    b.lookup(slots.len(), "replacement return actual input capture")?;
                    slots.get(&capture)
                } else {
                    None
                }
            }
            Root::Local(_) | Root::StoreElement(_) => None,
        };
        if let Some(actual) = actual {
            let projected = project(actual, &returned.path, b)?;
            for place in &projected {
                work(b, place.path.len().saturating_add(1))?;
                let mut place = place.clone();
                place.mutable &= returned.mutable;
                insert(&mut out, &place, b)?;
            }
        }
    }
    Ok(out)
}
fn function(
    module: &TypedModule,
    f: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    summaries: &[Places],
    retirement: bool,
    b: &mut Budget,
) -> CompileResult<Facts> {
    work(
        b,
        f.values
            .len()
            .saturating_mul(2)
            .saturating_add(f.blocks.len()),
    )?;
    let mut values = vec![Places::new(); f.values.len()];
    let mut slots = vec![Slots::new(); f.values.len()];
    let mut outputs = vec![State::default(); f.blocks.len()];
    let mut initial = State::default();
    work(b, f.parameters.len())?;
    for (index, local) in f.parameters.iter().enumerate() {
        let ty = &f.locals[local.0 as usize].ty;
        if matches!(ty, Type::Address { .. }) || retirement && matches!(ty, Type::Desc(_)) {
            initial.addresses.insert(
                *local,
                Places::from([Place {
                    root: Root::Parameter(index as u32),
                    mutable: matches!(ty, Type::Address { mutable: true, .. }),
                    path: vec![],
                }]),
            );
        }
        if let Some(id) = closure_id(ty) {
            let captures = &module.closures[id.0 as usize].captures;
            work(b, captures.len())?;
            let mut row = Slots::new();
            for (capture, ty) in captures.iter().enumerate() {
                if matches!(ty, Type::Address { .. }) || retirement && matches!(ty, Type::Desc(_)) {
                    b.lookup(row.len(), "replacement symbolic capture insertion")?;
                    row.insert(
                        capture as u32,
                        Places::from([Place {
                            root: Root::ParameterCapture(index as u32, capture as u32),
                            mutable: matches!(ty, Type::Address { mutable: true, .. }),
                            path: vec![],
                        }]),
                    );
                }
            }
            initial.environments.insert(*local, row);
        }
    }
    state_cost(&initial, b)?;
    let mut local_slots = initial.environments.clone();
    let mut positions = BTreeMap::new();
    loop {
        let mut changed = false;
        for (block_index, block) in f.blocks.iter().enumerate() {
            work(b, 1)?;
            if !reachable[block_index] {
                continue;
            }
            let mut state = State::default();
            if block_index == f.entry.0 as usize {
                state_cost(&initial, b)?;
                state = initial.clone();
            }
            for predecessor in &predecessors[block_index] {
                work(b, 1)?;
                if reachable[*predecessor] {
                    state_join(&mut state, &outputs[*predecessor], b)?;
                }
            }
            for (position, instruction) in block.instructions.iter().enumerate() {
                work(b, 1)?;
                if retirement
                    && matches!(
                        instruction.kind,
                        InstructionKind::CommitPlaceReplace { .. }
                            | InstructionKind::Call { .. }
                            | InstructionKind::CallClosure { .. }
                            | InstructionKind::CallClosureOnce { .. }
                            | InstructionKind::CallClosureBorrowed { .. }
                            | InstructionKind::CallClosureShared { .. }
                            | InstructionKind::CallClosureMut { .. }
                            | InstructionKind::WithForeignCallback { .. }
                            | InstructionKind::WithScopedThread { .. }
                            | InstructionKind::WithSyncCell { .. }
                    )
                {
                    state_cost(&state, b)?;
                    b.lookup(positions.len(), "actual retirement point state")?;
                    positions.insert((block_index, position), state.clone());
                }
                if let Some(result) = instruction.result {
                    let mut row = Places::new();
                    if matches!(f.values[result.0 as usize], Type::Address { .. })
                        || retirement && matches!(f.values[result.0 as usize], Type::Desc(_))
                    {
                        match &instruction.kind {
                            InstructionKind::DescribeLocal { local } => {
                                row.insert(Place {
                                    root: Root::Local(*local),
                                    mutable: false,
                                    path: vec![],
                                });
                            }
                            InstructionKind::DescribePackageField { local, index } => {
                                row.insert(Place {
                                    root: Root::Local(*local),
                                    mutable: false,
                                    path: vec![PlaceProjection::Field(*index)],
                                });
                            }
                            InstructionKind::DescribeAddress { address } => {
                                join(&mut row, &values[address.0 as usize], b)?;
                            }
                            InstructionKind::DescribeCapture { index } => {
                                row.insert(Place {
                                    root: Root::Capture(*index),
                                    mutable: false,
                                    path: vec![],
                                });
                            }
                            InstructionKind::BorrowLocal { local }
                            | InstructionKind::BorrowLocalMut { local } => {
                                row.insert(Place {
                                    root: Root::Local(*local),
                                    mutable: local_mutable(f, instruction, *local),
                                    path: vec![],
                                });
                            }
                            InstructionKind::BorrowPackageField { local, index, .. } => {
                                row.insert(Place {
                                    root: Root::Local(*local),
                                    mutable: local_mutable(f, instruction, *local),
                                    path: vec![PlaceProjection::Field(*index)],
                                });
                            }
                            InstructionKind::BorrowPlaceLocal { local, path, .. } => {
                                work(b, path.len())?;
                                row.insert(Place {
                                    root: Root::Local(*local),
                                    mutable: local_mutable(f, instruction, *local),
                                    path: path.clone(),
                                });
                            }
                            InstructionKind::BorrowFieldLocal { local, path, .. } => {
                                work(b, path.len().saturating_add(1))?;
                                row.insert(Place {
                                    root: Root::Local(*local),
                                    mutable: local_mutable(f, instruction, *local),
                                    path: path
                                        .iter()
                                        .copied()
                                        .map(PlaceProjection::Field)
                                        .collect(),
                                });
                            }
                            InstructionKind::BorrowVariantLocal { local, path, .. } => {
                                work(b, path.len())?;
                                row.insert(Place {
                                    root: Root::Local(*local),
                                    mutable: local_mutable(f, instruction, *local),
                                    path: path
                                        .iter()
                                        .copied()
                                        .map(PlaceProjection::Variant)
                                        .collect(),
                                });
                            }
                            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                                b.lookup(state.addresses.len(), "replacement actual address load")?;
                                if let Some(actual) = state.addresses.get(local) {
                                    join(&mut row, actual, b)?;
                                }
                            }
                            InstructionKind::ReadCapture { index } => {
                                row.insert(Place {
                                    root: Root::Capture(*index),
                                    mutable: f
                                        .environment
                                        .and_then(|id| {
                                            module.closures[id.0 as usize]
                                                .captures
                                                .get(*index as usize)
                                        })
                                        .is_some_and(|ty| {
                                            matches!(ty, Type::Address { mutable: true, .. })
                                        }),
                                    path: vec![],
                                });
                            }
                            InstructionKind::ReborrowAddress { address, .. } => {
                                join(&mut row, &values[address.0 as usize], b)?;
                            }
                            InstructionKind::AddressField { address, index } => {
                                row = project(
                                    &values[address.0 as usize],
                                    &[PlaceProjection::Field(*index)],
                                    b,
                                )?;
                            }
                            InstructionKind::BorrowPlaceAddress { address, path, .. } => {
                                row = project(&values[address.0 as usize], path, b)?;
                            }
                            InstructionKind::BorrowVariantAddress { address, path, .. } => {
                                work(b, path.len())?;
                                let route = path
                                    .iter()
                                    .copied()
                                    .map(PlaceProjection::Variant)
                                    .collect::<Vec<_>>();
                                row = project(&values[address.0 as usize], &route, b)?;
                            }
                            InstructionKind::OwnedStoreBorrowMut { .. }
                            | InstructionKind::OwnedStoreBorrow { .. } => {
                                row.insert(Place {
                                    root: Root::StoreElement(result),
                                    mutable: matches!(
                                        instruction.kind,
                                        InstructionKind::OwnedStoreBorrowMut { .. }
                                    ),
                                    path: vec![],
                                });
                            }
                            InstructionKind::Call {
                                function,
                                arguments,
                            } => {
                                row = substitute(
                                    &summaries[function.0 as usize],
                                    arguments,
                                    None,
                                    &values,
                                    &slots,
                                    b,
                                )?;
                            }
                            InstructionKind::CallClosureShared { callee, arguments }
                            | InstructionKind::CallClosureMut { callee, arguments } => {
                                let id = closure_id(&f.locals[callee.0 as usize].ty).ok_or_else(
                                    || invalid("replacement address result has no actual closure"),
                                )?;
                                b.lookup(
                                    state.environments.len(),
                                    "replacement actual environment result",
                                )?;
                                row = substitute(
                                    &summaries[module.closures[id.0 as usize].function.0 as usize],
                                    arguments,
                                    state.environments.get(callee),
                                    &values,
                                    &slots,
                                    b,
                                )?;
                            }
                            InstructionKind::CallClosureBorrowed {
                                callee, arguments, ..
                            } => {
                                let id =
                                    closure_id(&f.values[callee.0 as usize]).ok_or_else(|| {
                                        invalid("replacement borrowed result has no actual closure")
                                    })?;
                                row = substitute(
                                    &summaries[module.closures[id.0 as usize].function.0 as usize],
                                    arguments,
                                    Some(&slots[callee.0 as usize]),
                                    &values,
                                    &slots,
                                    b,
                                )?;
                            }
                            _ => {}
                        }
                        let semantic_mutable = match instruction.kind {
                            InstructionKind::ReborrowAddress { mutable, .. }
                            | InstructionKind::BorrowPlaceAddress { mutable, .. }
                            | InstructionKind::BorrowVariantAddress { mutable, .. } => mutable,
                            _ => true,
                        };
                        if !semantic_mutable
                            || !matches!(
                                f.values[result.0 as usize],
                                Type::Address { mutable: true, .. }
                            )
                        {
                            let mut shared = Places::new();
                            work(b, row.len())?;
                            for place in &row {
                                work(b, place.path.len().saturating_add(1))?;
                                let mut place = place.clone();
                                place.mutable = false;
                                insert(&mut shared, &place, b)?;
                            }
                            row = shared;
                        }
                        changed |= join(&mut values[result.0 as usize], &row, b)?;
                    }
                    if closure_id(&f.values[result.0 as usize]).is_some() {
                        let mut row = Slots::new();
                        match &instruction.kind {
                            InstructionKind::MakeClosure { closure, captures } => {
                                work(b, captures.len())?;
                                for (index, (value, ty)) in captures
                                    .iter()
                                    .zip(&module.closures[closure.0 as usize].captures)
                                    .enumerate()
                                {
                                    if matches!(ty, Type::Address { .. })
                                        || retirement && matches!(ty, Type::Desc(_))
                                    {
                                        b.lookup(
                                            row.len(),
                                            "replacement captured actual slot insert",
                                        )?;
                                        join(
                                            row.entry(index as u32).or_default(),
                                            &values[value.0 as usize],
                                            b,
                                        )?;
                                    }
                                }
                            }
                            InstructionKind::Load(local)
                            | InstructionKind::TakeLocal { local }
                            | InstructionKind::BorrowLocal { local }
                            | InstructionKind::BorrowLocalMut { local } => {
                                b.lookup(
                                    state.environments.len(),
                                    "replacement actual environment load",
                                )?;
                                if let Some(actual) = state.environments.get(local) {
                                    slots_join(&mut row, actual, b)?;
                                }
                            }
                            InstructionKind::ReborrowAddress { address, .. } => {
                                slots_join(&mut row, &slots[address.0 as usize], b)?;
                            }
                            _ => {}
                        }
                        changed |= slots_join(&mut slots[result.0 as usize], &row, b)?;
                    }
                }
                match instruction.kind {
                    InstructionKind::Store { local, value } => {
                        b.lookup(
                            state
                                .addresses
                                .len()
                                .saturating_add(state.environments.len()),
                            "replacement exact local publication",
                        )?;
                        let mut address = Places::new();
                        join(&mut address, &values[value.0 as usize], b)?;
                        if address.is_empty() {
                            state.addresses.remove(&local);
                        } else {
                            state.addresses.insert(local, address);
                        }
                        let mut environment = Slots::new();
                        slots_join(&mut environment, &slots[value.0 as usize], b)?;
                        b.lookup(
                            local_slots.len(),
                            "replacement retained actual capture local",
                        )?;
                        slots_join(local_slots.entry(local).or_default(), &environment, b)?;
                        if environment.is_empty() {
                            state.environments.remove(&local);
                        } else {
                            state.environments.insert(local, environment);
                        }
                    }
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropClosure { local } => {
                        b.lookup(
                            state
                                .addresses
                                .len()
                                .saturating_add(state.environments.len()),
                            "replacement address local end",
                        )?;
                        state.addresses.remove(&local);
                        state.environments.remove(&local);
                    }
                    _ => {}
                }
            }
            state_cost(&state, b)?;
            state_cost(&outputs[block_index], b)?;
            if state != outputs[block_index] {
                outputs[block_index] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut returned = Places::new();
    for (block, live) in f.blocks.iter().zip(reachable) {
        work(b, 1)?;
        if *live
            && let Terminator::Return(Some(value)) = block.terminator
            && (matches!(f.return_type, Type::Address { .. })
                || retirement && matches!(f.return_type, Type::Desc(_)))
        {
            work(b, values[value.0 as usize].len())?;
            for place in &values[value.0 as usize] {
                if matches!(
                    place.root,
                    Root::Parameter(_) | Root::Capture(_) | Root::ParameterCapture(..)
                ) {
                    work(b, place.path.len().saturating_add(1))?;
                    let mut place = place.clone();
                    place.mutable &= matches!(f.return_type, Type::Address { mutable: true, .. });
                    insert(&mut returned, &place, b)?;
                }
            }
        }
    }
    Ok(Facts {
        values,
        slots,
        local_slots,
        positions,
        returned,
    })
}
pub(crate) fn module(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    retirement: bool,
    b: &mut Budget,
) -> CompileResult<Vec<Facts>> {
    work(b, module.functions.len().saturating_mul(2))?;
    let mut summaries = vec![Places::new(); module.functions.len()];
    loop {
        work(b, module.functions.len())?;
        let mut all = Vec::with_capacity(module.functions.len());
        let mut changed = false;
        for (index, (f, (predecessors, reachable))) in
            module.functions.iter().zip(graphs).enumerate()
        {
            let found = function(
                module,
                f,
                predecessors,
                reachable,
                &summaries,
                retirement,
                b,
            )?;
            changed |= join(&mut summaries[index], &found.returned, b)?;
            all.push(found);
        }
        if !changed {
            return Ok(all);
        }
    }
}

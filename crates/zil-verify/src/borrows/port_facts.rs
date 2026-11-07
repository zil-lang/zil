//! Private retained-port provenance. Types describe positions, never their
//! sources. This solver reads executable transfers and computes its own call
//! fixed point; neither candidate Holds nor producer summaries enter it.
use super::*;
use std::collections::BTreeSet;

pub(super) type Path = Vec<ContractProjection>;
type State = BTreeMap<LocalId, Carrier>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Input(StoragePosition),
    Local(Root),
    Static,
    Escaped,
}

/// A concrete caller-side loan, not a lifetime class. Value anchors select an
/// already reconstructed address loan; input anchors identify one exact port.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Parent {
    Input(StoragePosition),
    Value(ValueId),
    Retained(ValueId, Path),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Port {
    pub(super) mode: StoredBorrowMode,
    origins: BTreeSet<Origin>,
    pub(super) parents: BTreeSet<Parent>,
    unknown: bool,
    fresh: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Carrier {
    pub(super) ports: BTreeMap<Path, Port>,
    // Bottom (neither arm) is distinct from a proven error with no payload.
    success: bool,
    error: bool,
    unknown: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Owner {
    Value(ValueId),
    Local(LocalId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct FunctionFacts {
    pub(super) values: Vec<Carrier>,
    pub(super) parameters: BTreeMap<StoragePosition, StoredBorrowMode>,
    pub(super) owner_inputs: Vec<BTreeSet<Owner>>,
    pub(super) created: BTreeMap<(ValueId, Path), (StoredBorrowMode, BTreeSet<Parent>)>,
}

fn bad(span: Span, message: &str) -> Diagnostic {
    reject("E-VERIFY-TASK-PORT", Some(span), message)
}

fn mode(ty: &Type) -> Option<StoredBorrowMode> {
    match ty {
        Type::Address { mutable, .. } => Some(if *mutable {
            StoredBorrowMode::ExclusiveAddress
        } else {
            StoredBorrowMode::SharedAddress
        }),
        Type::Desc(_) => Some(StoredBorrowMode::ReadOnlyDescription),
        _ => None,
    }
}

fn retained_path(path: &[ContractProjection]) -> bool {
    path.iter()
        .any(|p| matches!(p, ContractProjection::RetainedPort(_)))
}

fn shape(ty: &Type, budget: &mut Budget) -> Check<BTreeMap<Path, StoredBorrowMode>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![(ty, Path::new())];
    while let Some((ty, path)) = pending.pop() {
        budget.spend(path.len().saturating_add(1))?;
        if let Some(mode) = mode(ty) {
            result.insert(path.clone(), mode);
        }
        match ty {
            Type::Task { retained_life, .. } => {
                for (index, port) in retained_life.ports.iter().enumerate() {
                    budget.spend(path.len().saturating_add(2))?;
                    let mut selected = path.clone();
                    selected.push(ContractProjection::RetainedPort(index as u32));
                    result.insert(selected, port.mode);
                }
            }
            Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Address { inner, .. }
            | Type::BorrowedStorage { storage: inner, .. } => pending.push((inner, path)),
            Type::ErrorUnion { error, result } => {
                for (is_error, ty) in [(true, error), (false, result)] {
                    let mut selected = path.clone();
                    selected.push(ContractProjection::Place(PlaceProjection::ErrorArm(
                        is_error,
                    )));
                    pending.push((ty, selected));
                }
            }
            Type::Tuple(fields) => {
                for (index, ty) in fields.iter().enumerate() {
                    let mut selected = path.clone();
                    selected.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    pending.push((ty, selected));
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, ty)) in fields.iter().enumerate() {
                    let mut selected = path.clone();
                    selected.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    pending.push((ty, selected));
                }
            }
            _ => {}
        }
    }
    Ok(result)
}

pub(super) fn owned(ty: &Type) -> bool {
    match ty {
        Type::Managed(inner) => {
            matches!(inner.as_ref(), Type::Task { retained_life, .. } if !retained_life.ports.is_empty())
        }
        Type::ErrorUnion { result, .. } => owned(result),
        _ => false,
    }
}

fn cost(value: &Carrier, budget: &mut Budget) -> Check<()> {
    budget.spend(value.ports.len().saturating_add(1))?;
    for (path, port) in &value.ports {
        budget.spend(
            path.len()
                .saturating_add(port.origins.len())
                .saturating_add(port.parents.len()),
        )?;
        for origin in &port.origins {
            if let Origin::Input(position) = origin {
                budget.spend(position.path.len())?;
            }
        }
        for parent in &port.parents {
            if let Parent::Input(position) = parent {
                budget.spend(position.path.len())?;
            }
        }
    }
    Ok(())
}

fn copy(value: &Carrier, budget: &mut Budget) -> Check<Carrier> {
    cost(value, budget)?;
    Ok(value.clone())
}

fn merge(target: &mut Carrier, source: &Carrier, budget: &mut Budget) -> Check<bool> {
    cost(source, budget)?;
    let mut changed = false;
    for (path, source) in &source.ports {
        if let Some(target) = target.ports.get_mut(path) {
            if target.mode != source.mode {
                return Err(reject(
                    "E-VERIFY-TASK-PORT",
                    None,
                    "retained position changes access mode",
                ));
            }
            let before = target.origins.len() + target.parents.len();
            target.origins.extend(source.origins.iter().cloned());
            target.parents.extend(source.parents.iter().cloned());
            changed |= before != target.origins.len() + target.parents.len()
                || source.unknown && !target.unknown;
            changed |= source.fresh && !target.fresh;
            target.fresh |= source.fresh;
            target.unknown |= source.unknown;
        } else {
            target.ports.insert(path.clone(), source.clone());
            changed = true;
        }
    }
    changed |= source.success && !target.success
        || source.error && !target.error
        || source.unknown && !target.unknown;
    target.success |= source.success;
    target.error |= source.error;
    target.unknown |= source.unknown;
    Ok(changed)
}

fn state_copy(state: &State, budget: &mut Budget) -> Check<State> {
    let mut result = State::new();
    for (local, value) in state {
        result.insert(*local, copy(value, budget)?);
    }
    Ok(result)
}

fn project(value: &Carrier, prefix: &[ContractProjection], budget: &mut Budget) -> Check<Carrier> {
    cost(value, budget)?;
    let mut result = Carrier {
        success: value.success,
        unknown: value.unknown,
        ..Carrier::default()
    };
    for (path, row) in &value.ports {
        if let Some(path) = path.strip_prefix(prefix) {
            result.ports.insert(path.to_vec(), row.clone());
        }
    }
    Ok(result)
}

fn wrap(value: &Carrier, prefix: ContractProjection, budget: &mut Budget) -> Check<Carrier> {
    cost(value, budget)?;
    let mut result = value.clone();
    result.ports.clear();
    for (path, row) in &value.ports {
        let mut selected = vec![prefix];
        selected.extend(path);
        result.ports.insert(selected, row.clone());
    }
    Ok(result)
}

fn input(ty: &Type, index: u32, budget: &mut Budget) -> Check<Carrier> {
    let mut result = Carrier {
        success: true,
        error: matches!(ty, Type::ErrorUnion { .. }),
        ..Carrier::default()
    };
    for (path, mode) in shape(ty, budget)? {
        let position = StoragePosition {
            base: StorageBase::Parameter(index),
            path: path.clone(),
        };
        result.ports.insert(
            path,
            Port {
                mode,
                origins: BTreeSet::from([Origin::Input(position.clone())]),
                parents: BTreeSet::from([Parent::Input(position)]),
                unknown: false,
                fresh: false,
            },
        );
    }
    Ok(result)
}

fn select<'a>(
    values: &'a [Carrier],
    arguments: &[ValueId],
    position: &StoragePosition,
) -> Option<&'a Port> {
    let StorageBase::Parameter(index) = position.base else {
        return None;
    };
    let value = arguments.get(index as usize)?;
    values.get(value.0 as usize)?.ports.get(&position.path)
}

fn invoke(
    summary: &Carrier,
    arguments: &[ValueId],
    values: &[Carrier],
    budget: &mut Budget,
) -> Check<Carrier> {
    cost(summary, budget)?;
    let mut result = Carrier {
        success: summary.success,
        error: summary.error,
        unknown: summary.unknown,
        ..Carrier::default()
    };
    for (path, port) in &summary.ports {
        let mut row = Port {
            mode: port.mode,
            origins: BTreeSet::new(),
            parents: BTreeSet::new(),
            unknown: port.unknown,
            fresh: port.fresh,
        };
        for origin in &port.origins {
            match origin {
                Origin::Input(position) => {
                    if let Some(actual) = select(values, arguments, position) {
                        budget.spend(actual.origins.len().saturating_add(actual.parents.len()))?;
                        row.origins.extend(actual.origins.iter().cloned());
                        row.parents.extend(actual.parents.iter().cloned());
                        row.unknown |= actual.unknown;
                        row.fresh |= actual.fresh;
                        if port.mode == StoredBorrowMode::ExclusiveAddress
                            && actual.mode != StoredBorrowMode::ExclusiveAddress
                        {
                            row.unknown = true;
                        }
                    }
                    // Missing while a callee/CFG predecessor is Bottom stays
                    // ungrounded and is rejected after the complete fixed point.
                }
                Origin::Static => {
                    row.origins.insert(Origin::Static);
                }
                Origin::Local(_) | Origin::Escaped => {
                    row.origins.insert(Origin::Escaped);
                }
            }
        }
        result.ports.insert(path.clone(), row);
    }
    Ok(result)
}

fn retain(
    value: ValueId,
    path: &Path,
    row: &mut Port,
    created: &mut BTreeMap<(ValueId, Path), (StoredBorrowMode, BTreeSet<Parent>)>,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(
        path.len()
            .saturating_add(row.parents.len())
            .saturating_add(1),
    )?;
    let entry = created
        .entry((value, path.clone()))
        .or_insert_with(|| (row.mode, BTreeSet::new()));
    entry.1.extend(row.parents.iter().cloned());
    row.parents = BTreeSet::from([Parent::Retained(value, path.clone())]);
    Ok(())
}

fn local_address(
    f: &TypedFunction,
    value: ValueId,
    local: LocalId,
    state: &State,
    budget: &mut Budget,
) -> Check<Carrier> {
    let mut result = match state.get(&local) {
        Some(row) => copy(row, budget)?,
        None => Carrier::default(),
    };
    result.success = true;
    if let Some(mode) = mode(&f.values[value.0 as usize]) {
        result.ports.insert(
            vec![],
            Port {
                mode,
                origins: BTreeSet::from([Origin::Local(Root::Local(local))]),
                parents: BTreeSet::from([Parent::Value(value)]),
                unknown: false,
                fresh: false,
            },
        );
    }
    Ok(result)
}

fn produce(
    f: &TypedFunction,
    instruction: &Instruction,
    values: &[Carrier],
    state: &State,
    summaries: &[Carrier],
    created: &mut BTreeMap<(ValueId, Path), (StoredBorrowMode, BTreeSet<Parent>)>,
    budget: &mut Budget,
) -> Check<Carrier> {
    let Some(value) = instruction.result else {
        return Ok(Carrier::default());
    };
    let ty = &f.values[value.0 as usize];
    let wanted = shape(ty, budget)?;
    if wanted.is_empty() && !matches!(ty, Type::ErrorUnion { .. }) {
        return Ok(Carrier::default());
    }
    match &instruction.kind {
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => state
            .get(local)
            .map_or(Ok(Carrier::default()), |row| copy(row, budget)),
        InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::DescribeLocal { local } => {
            local_address(f, value, *local, state, budget)
        }
        InstructionKind::TaskCreate {
            arguments,
            port_bindings,
            ..
        } => {
            let mut result = Carrier {
                success: true,
                error: true,
                ..Carrier::default()
            };
            for binding in port_bindings {
                budget.spend(binding.source.path.len().saturating_add(1))?;
                let path = vec![
                    ContractProjection::Place(PlaceProjection::ErrorArm(false)),
                    ContractProjection::RetainedPort(binding.port),
                ];
                let Some(&mode) = wanted.get(&path) else {
                    return Err(bad(
                        instruction.span,
                        "constructor output has no selected retained port",
                    ));
                };
                let mut row = match select(values, arguments, &binding.source) {
                    Some(source) => {
                        let mut row = source.clone();
                        row.unknown |= row.mode != mode;
                        row.mode = mode;
                        row
                    }
                    None => Port {
                        mode,
                        origins: BTreeSet::new(),
                        parents: BTreeSet::new(),
                        unknown: false,
                        fresh: false,
                    },
                };
                row.fresh = true;
                retain(value, &path, &mut row, created, budget)?;
                result.ports.insert(path, row);
            }
            Ok(result)
        }
        InstructionKind::Call {
            function,
            arguments,
        }
        | InstructionKind::SuspendCall {
            function,
            arguments,
        } => {
            let mut result = invoke(&summaries[function.0 as usize], arguments, values, budget)?;
            for (path, port) in &mut result.ports {
                if summaries[function.0 as usize]
                    .ports
                    .get(path)
                    .is_some_and(|row| row.fresh)
                    && retained_path(path)
                {
                    retain(value, path, port, created, budget)?;
                }
            }
            Ok(result)
        }
        InstructionKind::MakeErrorUnion {
            value: source,
            is_error,
        } => {
            if *is_error {
                return Ok(Carrier {
                    error: true,
                    ..Carrier::default()
                });
            }
            let mut result = wrap(
                &values[source.0 as usize],
                ContractProjection::Place(PlaceProjection::ErrorArm(false)),
                budget,
            )?;
            result.error = false;
            Ok(result)
        }
        InstructionKind::TakeErrorUnion {
            value: source,
            is_error,
        } => {
            if *is_error {
                return Ok(Carrier::default());
            }
            project(
                &values[source.0 as usize],
                &[ContractProjection::Place(PlaceProjection::ErrorArm(false))],
                budget,
            )
        }
        InstructionKind::TakeVariantField { local, path }
        | InstructionKind::TakeTupleField { local, path } => {
            let path = path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            state
                .get(local)
                .map_or(Ok(Carrier::default()), |row| project(row, &path, budget))
        }
        InstructionKind::DescribeAddress { address }
        | InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressSlice { address, .. }
        | InstructionKind::AddressIndex { address, .. } => {
            let mut result = copy(&values[address.0 as usize], budget)?;
            if let Some(row) = result.ports.get_mut(&Vec::new()) {
                if let Some(mode) = mode(ty) {
                    row.mode = mode;
                }
                if matches!(instruction.kind, InstructionKind::ReborrowAddress { .. }) {
                    row.parents = BTreeSet::from([Parent::Value(value)]);
                }
            }
            Ok(result)
        }
        InstructionKind::AddressField { address, index } => {
            let prefix = ContractProjection::Place(PlaceProjection::Field(*index));
            let mut result = project(&values[address.0 as usize], &[prefix], budget)?;
            if let Some(source) = values[address.0 as usize].ports.get(&Vec::new()) {
                let mut row = source.clone();
                row.origins = row
                    .origins
                    .into_iter()
                    .map(|origin| match origin {
                        Origin::Input(mut position) => {
                            position.path.push(prefix);
                            Origin::Input(position)
                        }
                        other => other,
                    })
                    .collect();
                if let Some(mode) = mode(ty) {
                    row.mode = mode;
                }
                result.ports.insert(vec![], row);
            }
            Ok(result)
        }
        InstructionKind::StaticUtf8 { .. } => {
            let ports = wanted
                .into_iter()
                .map(|(path, mode)| {
                    (
                        path,
                        Port {
                            mode,
                            origins: BTreeSet::from([Origin::Static]),
                            parents: BTreeSet::new(),
                            unknown: false,
                            fresh: false,
                        },
                    )
                })
                .collect();
            Ok(Carrier {
                ports,
                success: true,
                ..Carrier::default()
            })
        }
        _ => Ok(Carrier {
            success: true,
            unknown: true,
            ports: wanted
                .into_iter()
                .map(|(path, mode)| {
                    (
                        path,
                        Port {
                            mode,
                            origins: BTreeSet::new(),
                            parents: BTreeSet::new(),
                            unknown: true,
                            fresh: false,
                        },
                    )
                })
                .collect(),
            ..Carrier::default()
        }),
    }
}

fn transfer(
    state: &mut State,
    instruction: &Instruction,
    values: &[Carrier],
    budget: &mut Budget,
) -> Check<()> {
    match instruction.kind {
        InstructionKind::Store { local, value } => {
            state.insert(local, copy(&values[value.0 as usize], budget)?);
        }
        InstructionKind::TakeLocal { local }
        | InstructionKind::TakeVariantField { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropErrorUnion { local } => {
            state.remove(&local);
        }
        _ => {}
    }
    Ok(())
}

fn reconstruct(
    f: &TypedFunction,
    graph: &FunctionGraph,
    summaries: &[Carrier],
    budget: &mut Budget,
) -> Check<(FunctionFacts, Carrier)> {
    budget.spend(f.values.len().saturating_add(f.blocks.len()))?;
    let mut facts = FunctionFacts {
        values: vec![Carrier::default(); f.values.len()],
        ..FunctionFacts::default()
    };
    let mut initial = State::new();
    for (index, local) in f.parameters.iter().enumerate() {
        let row = input(&f.locals[local.0 as usize].ty, index as u32, budget)?;
        for (path, port) in &row.ports {
            facts.parameters.insert(
                StoragePosition {
                    base: StorageBase::Parameter(index as u32),
                    path: path.clone(),
                },
                port.mode,
            );
        }
        initial.insert(*local, row);
    }
    let mut outputs = vec![State::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1)?;
            if !graph.reachable[b] {
                continue;
            }
            let mut state = if b == f.entry.0 as usize {
                state_copy(&initial, budget)?
            } else {
                State::new()
            };
            for &predecessor in &graph.predecessors[b] {
                if !graph.reachable[predecessor] {
                    continue;
                }
                for (local, row) in &outputs[predecessor] {
                    merge(state.entry(*local).or_default(), row, budget)?;
                }
            }
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let Some(value) = instruction.result {
                    let row = produce(
                        f,
                        instruction,
                        &facts.values,
                        &state,
                        summaries,
                        &mut facts.created,
                        budget,
                    )?;
                    changed |= merge(&mut facts.values[value.0 as usize], &row, budget)?;
                }
                transfer(&mut state, instruction, &facts.values, budget)?;
            }
            if outputs[b] != state {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut summary = Carrier::default();
    for (block, reachable) in f.blocks.iter().zip(&graph.reachable) {
        budget.spend(1)?;
        if *reachable && let Terminator::Return(Some(value)) = block.terminator {
            merge(&mut summary, &facts.values[value.0 as usize], budget)?;
        }
    }
    // Only origin selectors cross calls. Caller-specific anchors never do.
    for port in summary.ports.values_mut() {
        port.parents.clear();
    }
    Ok((facts, summary))
}

fn binding<'a>(
    contract: &'a StorageLifeContract,
    position: &StoragePosition,
) -> Option<&'a StorageLifeBinding> {
    contract
        .bindings
        .iter()
        .filter(|row| {
            row.position.base == position.base && position.path.starts_with(&row.position.path)
        })
        .max_by_key(|row| row.position.path.len())
}

fn validate(
    f: &TypedFunction,
    graph: &FunctionGraph,
    facts: &FunctionFacts,
    budget: &mut Budget,
) -> Check<()> {
    let expected = shape(&f.return_type, budget)?;
    if !expected.keys().any(|path| retained_path(path)) {
        return Ok(());
    }
    let contract = f
        .storage_life
        .as_ref()
        .ok_or_else(|| bad(f.span, "retained return lacks an actual input relation"))?;
    for (block, reachable) in f.blocks.iter().zip(&graph.reachable) {
        budget.spend(1)?;
        let Terminator::Return(Some(value)) = block.terminator else {
            continue;
        };
        if !reachable {
            continue;
        }
        let actual = &facts.values[value.0 as usize];
        if actual.unknown || !actual.success && !actual.error {
            return Err(bad(
                f.span,
                "retained return source is unknown or ungrounded",
            ));
        }
        if !actual.success {
            continue;
        }
        for (path, &mode) in &expected {
            if !retained_path(path) {
                continue;
            }
            let row = actual
                .ports
                .get(path)
                .ok_or_else(|| bad(f.span, "retained result port lacks an actual source"))?;
            if row.unknown || row.origins.is_empty() || row.mode != mode {
                return Err(bad(
                    f.span,
                    "retained result port has no complete actual source",
                ));
            }
            let output = StoragePosition {
                base: StorageBase::Result,
                path: path.clone(),
            };
            let output = binding(contract, &output)
                .ok_or_else(|| bad(f.span, "retained result port has no declared class"))?;
            for origin in &row.origins {
                budget.spend(contract.bindings.len().saturating_add(1))?;
                let Origin::Input(position) = origin else {
                    return Err(bad(
                        f.span,
                        "retained return escapes a local or unknown source",
                    ));
                };
                let input = binding(contract, position).ok_or_else(|| {
                    bad(f.span, "actual retained source has no declared input class")
                })?;
                if input.binder != output.binder {
                    return Err(bad(
                        f.span,
                        "actual retained source selects a different input class",
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn owner_transfer(
    f: &TypedFunction,
    instruction: &Instruction,
    owners: &mut BTreeSet<Owner>,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(owners.len().saturating_add(1))?;
    match &instruction.kind {
        InstructionKind::Store { local, value } if owned(&f.locals[local.0 as usize].ty) => {
            owners.remove(&Owner::Value(*value));
            owners.insert(Owner::Local(*local));
        }
        InstructionKind::TakeLocal { local }
        | InstructionKind::TakeVariantField { local, .. }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropErrorUnion { local } => {
            owners.remove(&Owner::Local(*local));
        }
        InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::TakeErrorUnion { value, .. } => {
            owners.remove(&Owner::Value(*value));
        }
        InstructionKind::TaskFinishDestroy { task } => {
            owners.remove(&Owner::Value(*task));
        }
        InstructionKind::Call { arguments, .. }
        | InstructionKind::SuspendCall { arguments, .. } => {
            for argument in arguments {
                if owned(&f.values[argument.0 as usize]) {
                    owners.remove(&Owner::Value(*argument));
                }
            }
        }
        InstructionKind::TaskCreate { arguments, .. } => {
            for argument in arguments {
                if owned(&f.values[argument.0 as usize]) {
                    owners.remove(&Owner::Value(*argument));
                }
            }
        }
        _ => {}
    }
    if let Some(value) = instruction.result
        && owned(&f.values[value.0 as usize])
    {
        owners.insert(Owner::Value(value));
    }
    Ok(())
}

fn owners(
    f: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<Vec<BTreeSet<Owner>>> {
    let initial = f
        .parameters
        .iter()
        .filter(|local| owned(&f.locals[local.0 as usize].ty))
        .map(|local| Owner::Local(*local))
        .collect::<BTreeSet<_>>();
    let mut inputs = vec![BTreeSet::new(); f.blocks.len()];
    let mut outputs = inputs.clone();
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1)?;
            if !graph.reachable[b] {
                continue;
            }
            let mut state = if b == f.entry.0 as usize {
                initial.clone()
            } else {
                BTreeSet::new()
            };
            for &predecessor in &graph.predecessors[b] {
                budget.spend(outputs[predecessor].len())?;
                state.extend(&outputs[predecessor]);
            }
            inputs[b] = state.clone();
            for instruction in &block.instructions {
                owner_transfer(f, instruction, &mut state, budget)?;
            }
            if state != outputs[b] {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(inputs)
}

pub(super) fn solve(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<FunctionFacts>> {
    let present = module.functions.iter().any(|f| {
        f.values
            .iter()
            .chain(f.locals.iter().map(|l| &l.ty))
            .any(super::retained_task_value)
    });
    if !present {
        return Ok(vec![FunctionFacts::default(); module.functions.len()]);
    }
    let mut summaries = vec![Carrier::default(); module.functions.len()];
    let mut facts = vec![FunctionFacts::default(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, f) in module.functions.iter().enumerate() {
            let (actual, summary) = reconstruct(f, &graphs[index], &summaries, budget)?;
            changed |= merge(&mut summaries[index], &summary, budget)?;
            facts[index] = actual;
        }
        if !changed {
            break;
        }
    }
    for (index, f) in module.functions.iter().enumerate() {
        validate(f, &graphs[index], &facts[index], budget)?;
        facts[index].owner_inputs = owners(f, &graphs[index], budget)?;
        // Constructor ports must be grounded even when their result is unused.
        for (block, reachable) in f.blocks.iter().zip(&graphs[index].reachable) {
            if !reachable {
                continue;
            }
            for instruction in &block.instructions {
                let Some(value) = instruction.result else {
                    continue;
                };
                if !super::retained_task_value(&f.values[value.0 as usize]) {
                    continue;
                }
                let actual = &facts[index].values[value.0 as usize];
                if actual.unknown || !actual.success && !actual.error {
                    return Err(bad(
                        instruction.span,
                        "retained value has no proven executable source",
                    ));
                }
                if actual.success {
                    for path in shape(&f.values[value.0 as usize], budget)?
                        .keys()
                        .filter(|path| retained_path(path))
                    {
                        if actual.ports.get(path).is_none_or(|row| {
                            row.unknown
                                || row.origins.is_empty()
                                || row.origins.contains(&Origin::Escaped)
                        }) {
                            return Err(bad(
                                instruction.span,
                                "retained port has no grounded actual constructor or call source",
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(facts)
}

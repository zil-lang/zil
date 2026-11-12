//! Independent producer reconstruction of each Task frame port's input source.
//! A retained class equates lifetimes, not target identities or parent loans.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use zil_ir::{ContractProjection, StorageBase, StoragePosition};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Input(StoragePosition),
    Local(LocalId),
    Static,
    Unknown,
}
type Sources = BTreeSet<Origin>;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Carrier {
    ports: Vec<Sources>,
    success: bool,
    error: bool,
}
impl Carrier {
    fn empty(count: usize) -> Self {
        Self {
            ports: vec![Sources::new(); count],
            success: false,
            error: false,
        }
    }
    fn join(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        if self.ports.len() != other.ports.len() {
            return Err(bad("Task port count changes across flow"));
        }
        let mut changed = false;
        for (left, right) in self.ports.iter_mut().zip(&other.ports) {
            budget.spend(
                left.len().saturating_add(right.len()).saturating_add(1),
                "Task port source merge",
            )?;
            let before = left.len();
            left.extend(right.iter().cloned());
            changed |= left.len() != before;
        }
        changed |= self.success != (self.success | other.success)
            || self.error != (self.error | other.error);
        self.success |= other.success;
        self.error |= other.error;
        Ok(changed)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Value(ValueId),
    Local(LocalId),
}
type State = BTreeMap<Owner, Carrier>;
#[derive(Clone, Debug)]
struct Summary {
    carrier: Carrier,
}

fn shape(ty: &Type) -> Option<&zil_ir::RetainedLifeShape> {
    if let Some(owner) = super::transfer::outcome_owner(ty) {
        return shape(owner);
    }
    match ty {
        Type::Task { retained_life, .. } if !retained_life.ports.is_empty() => Some(retained_life),
        Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => shape(inner),
        Type::ErrorUnion { result, .. } => shape(result),
        _ => None,
    }
}
fn fee(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "Task port origin reconstruction")
}
fn copy_state(state: &State, b: &mut Budget) -> CompileResult<State> {
    fee(b, state.len())?;
    for carrier in state.values() {
        fee(b, carrier.ports.len())?;
        for sources in &carrier.ports {
            fee(b, sources.len())?;
        }
    }
    Ok(state.clone())
}
fn source_from_hold(root: HoldRoot) -> Origin {
    match root {
        HoldRoot::Parameter(index) => Origin::Input(StoragePosition {
            base: StorageBase::Parameter(index),
            path: vec![],
        }),
        HoldRoot::Local(local) => Origin::Local(local),
        HoldRoot::Static => Origin::Static,
        HoldRoot::Capture(_) => Origin::Unknown,
    }
}
fn address_sources(holds: &[ValueHold], value: ValueId, b: &mut Budget) -> CompileResult<Sources> {
    let row = holds
        .get(value.0 as usize)
        .ok_or_else(|| bad("Task address Hold is absent"))?;
    if row.value != value {
        return Err(bad("Task address Hold identity differs"));
    }
    fee(b, row.roots.len().saturating_add(1))?;
    Ok(row.roots.iter().copied().map(source_from_hold).collect())
}
fn input_carrier(ty: &Type, index: u32, b: &mut Budget) -> CompileResult<Option<Carrier>> {
    let Some(shape) = shape(ty) else {
        return Ok(None);
    };
    fee(b, shape.ports.len().saturating_add(1))?;
    let mut carrier = Carrier::empty(shape.ports.len());
    carrier.success = true;
    for (port, roots) in carrier.ports.iter_mut().enumerate() {
        roots.insert(Origin::Input(StoragePosition {
            base: StorageBase::Parameter(index),
            path: vec![ContractProjection::RetainedPort(port as u32)],
        }));
    }
    Ok(Some(carrier))
}
fn merge_state(into: &mut State, from: &State, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (key, value) in from {
        fee(
            b,
            into.len()
                .saturating_add(value.ports.len())
                .saturating_add(1),
        )?;
        if let Some(prior) = into.get_mut(key) {
            changed |= prior.join(value, b)?;
        } else {
            fee(
                b,
                value
                    .ports
                    .iter()
                    .fold(0usize, |n, s| n.saturating_add(s.len())),
            )?;
            into.insert(*key, value.clone());
            changed = true;
        }
    }
    Ok(changed)
}
fn substitute(
    origin: &Origin,
    args: &[ValueId],
    state: &State,
    holds: &[ValueHold],
    b: &mut Budget,
) -> CompileResult<Sources> {
    fee(b, 1)?;
    let Origin::Input(position) = origin else {
        return Ok(Sources::from([origin.clone()]));
    };
    let StorageBase::Parameter(index) = position.base else {
        return Err(bad("Task result references a non-parameter source"));
    };
    let value = *args
        .get(index as usize)
        .ok_or_else(|| bad("Task call source parameter is absent"))?;
    match position.path.as_slice() {
        [] => address_sources(holds, value, b),
        [ContractProjection::RetainedPort(port)] => {
            let carrier = state
                .get(&Owner::Value(value))
                .ok_or_else(|| bad("Task call lost actual input owner"))?;
            carrier
                .ports
                .get(*port as usize)
                .cloned()
                .ok_or_else(|| bad("Task call input port is absent"))
        }
        _ => Err(bad(
            "Task call source selector is outside implemented port envelope",
        )),
    }
}
// The actual module, candidate-independent graph, and private port facts are
// separate proof inputs; keep them explicit at the transfer boundary.
#[allow(clippy::too_many_arguments)]
fn transfer(
    module: &TypedModule,
    plan: &CandidatePlan,
    fi: usize,
    instruction: &Instruction,
    holds: &[ValueHold],
    state: &mut State,
    memo: &mut [Option<Summary>],
    active: &mut [bool],
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    let f = &module.functions[fi];
    let output = instruction
        .result
        .filter(|v| f.values.get(v.0 as usize).and_then(shape).is_some());
    let result = match &instruction.kind {
        InstructionKind::TakeCapture { .. } | InstructionKind::CallClosureOnce { .. }
            if output.is_some() && b.task_callable_results =>
        {
            use crate::budget::TaskPortSource;
            let value = output.unwrap();
            let shape = shape(&f.values[value.0 as usize]).unwrap();
            let rows = b
                .task_retained_sources
                .get(fi)
                .and_then(|rows| rows.get(value.0 as usize))
                .ok_or_else(|| {
                    bad("Task returned frame lacks a complete actual source inventory")
                })?;
            let work = rows.iter().fold(1usize, |work, (path, sources)| {
                sources.iter().fold(
                    work.saturating_add(path.len())
                        .saturating_add(sources.len()),
                    |work, source| {
                        work.saturating_add(match source {
                            TaskPortSource::External(position) => position.path.len(),
                            _ => 0,
                        })
                    },
                )
            });
            fee(b, work)?;
            let rows = &b.task_retained_sources[fi][value.0 as usize];
            let mut carrier = Carrier::empty(shape.ports.len());
            carrier.success = true;
            for (port, sources) in carrier.ports.iter_mut().enumerate() {
                let path = [ContractProjection::RetainedPort(port as u32)];
                let actual = rows
                    .get(path.as_slice())
                    .ok_or_else(|| bad("Task returned frame omitted an actual retained port"))?;
                for source in actual {
                    sources.insert(match source {
                        TaskPortSource::External(position) => Origin::Input(position.clone()),
                        TaskPortSource::Local(local) => Origin::Local(*local),
                        TaskPortSource::Static => Origin::Static,
                        TaskPortSource::Unknown => Origin::Unknown,
                    });
                }
            }
            Some(carrier)
        }
        InstructionKind::TaskCreate {
            arguments,
            port_bindings,
            ..
        }
        | InstructionKind::TaskCreateOwned {
            arguments,
            port_bindings,
            ..
        } if output.is_some() => {
            let count = shape(&f.values[output.unwrap().0 as usize])
                .unwrap()
                .ports
                .len();
            let mut carrier = Carrier::empty(count);
            carrier.success = true;
            carrier.error = true;
            fee(b, port_bindings.len())?;
            for binding in port_bindings {
                let StorageBase::Parameter(index) = binding.source.base else {
                    return Err(bad("Task port source is not a constructor parameter"));
                };
                if !binding.source.path.is_empty() {
                    return Err(bad("Task port source path is not reconstructed"));
                }
                let value = *arguments
                    .get(index as usize)
                    .ok_or_else(|| bad("Task port input is absent"))?;
                let roots = address_sources(holds, value, b)?;
                if roots.is_empty() {
                    return Err(bad("Task constructor port has no actual address source"));
                }
                let port = carrier
                    .ports
                    .get_mut(binding.port as usize)
                    .ok_or_else(|| bad("Task port ordinal is absent"))?;
                port.extend(roots);
            }
            Some(carrier)
        }
        InstructionKind::WithScopedThread { argument, .. } if output.is_some() => {
            let mut carrier = state
                .remove(&Owner::Value(*argument))
                .ok_or_else(|| bad("Task thread outcome lost its original frame ports"))?;
            carrier.success = true;
            carrier.error = true;
            Some(carrier)
        }
        InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } if super::transfer::outcome_owner(&f.locals[local.0 as usize].ty).is_some() => {
            let carrier = state
                .remove(&Owner::Local(*local))
                .ok_or_else(|| bad("Task thread outcome lost actual frame"))?;
            if *variant == 0 {
                let target = *fields
                    .first()
                    .ok_or_else(|| bad("Task failure owner field absent"))?;
                state.insert(Owner::Local(target), carrier);
            }
            None
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let mut carrier = None;
            if let Some(value) = output {
                let callee = function.0 as usize;
                if callee >= module.functions.len() || module.functions[callee].foreign.is_some() {
                    return Err(bad("Task call has no complete verified body"));
                }
                let summary = summarize(module, plan, callee, graphs, memo, active, b)?;
                let mut actual =
                    Carrier::empty(shape(&f.values[value.0 as usize]).unwrap().ports.len());
                if actual.ports.len() != summary.carrier.ports.len() {
                    return Err(bad("Task call result port count differs"));
                }
                actual.success = summary.carrier.success;
                actual.error = summary.carrier.error;
                for (target, sources) in actual.ports.iter_mut().zip(&summary.carrier.ports) {
                    for origin in sources {
                        let replacements = substitute(origin, arguments, state, holds, b)?;
                        fee(b, replacements.len())?;
                        target.extend(replacements);
                    }
                }
                carrier = Some(actual);
            }
            for argument in arguments {
                fee(b, 1)?;
                if f.values.get(argument.0 as usize).and_then(shape).is_some() {
                    state.remove(&Owner::Value(*argument));
                }
            }
            carrier
        }
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::DescribeLocal { local }
            if output.is_some() =>
        {
            let carrier = state
                .get(&Owner::Local(*local))
                .ok_or_else(|| bad("Task borrowed view lost its actual local carrier"))?;
            fee(
                b,
                carrier
                    .ports
                    .iter()
                    .fold(1usize, |n, s| n.saturating_add(s.len())),
            )?;
            Some(carrier.clone())
        }
        InstructionKind::ReborrowAddress { address, .. } if output.is_some() => {
            let carrier = state
                .get(&Owner::Value(*address))
                .ok_or_else(|| bad("Task reborrow lost its actual input carrier"))?;
            fee(
                b,
                carrier
                    .ports
                    .iter()
                    .fold(1usize, |n, s| n.saturating_add(s.len())),
            )?;
            Some(carrier.clone())
        }
        InstructionKind::Store { local, value }
            if f.locals
                .get(local.0 as usize)
                .and_then(|l| shape(&l.ty))
                .is_some() =>
        {
            let carrier = state
                .remove(&Owner::Value(*value))
                .ok_or_else(|| bad("Task Store lost actual source carrier"))?;
            state.insert(Owner::Local(*local), carrier);
            None
        }
        InstructionKind::TakeLocal { local }
            if f.locals
                .get(local.0 as usize)
                .and_then(|l| shape(&l.ty))
                .is_some() =>
        {
            Some(
                state
                    .remove(&Owner::Local(*local))
                    .ok_or_else(|| bad("Task TakeLocal lost actual owner"))?,
            )
        }
        InstructionKind::TakeVariantField { local, path }
            if f.locals
                .get(local.0 as usize)
                .and_then(|l| shape(&l.ty))
                .is_some() =>
        {
            let mut carrier = state
                .remove(&Owner::Local(*local))
                .ok_or_else(|| bad("Task variant take lost actual owner"))?;
            match path.as_slice() {
                [zil_ir::PlaceProjection::ErrorArm(false)]
                | [
                    zil_ir::PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant: 0,
                        index: 0,
                    }),
                ] => {
                    carrier.error = false;
                    Some(carrier)
                }
                [zil_ir::PlaceProjection::ErrorArm(true)] => None,
                _ => return Err(bad("Task variant take path is not a complete arm")),
            }
        }
        InstructionKind::TakeErrorUnion { value, is_error }
            if f.values.get(value.0 as usize).and_then(shape).is_some() =>
        {
            let mut carrier = state
                .remove(&Owner::Value(*value))
                .ok_or_else(|| bad("Task error extraction lost carrier"))?;
            if *is_error {
                None
            } else {
                carrier.error = false;
                Some(carrier)
            }
        }
        InstructionKind::MakeErrorUnion { value, is_error } if output.is_some() => {
            if *is_error {
                let mut carrier = Carrier::empty(
                    shape(&f.values[output.unwrap().0 as usize])
                        .unwrap()
                        .ports
                        .len(),
                );
                carrier.error = true;
                Some(carrier)
            } else {
                let mut carrier = state
                    .remove(&Owner::Value(*value))
                    .ok_or_else(|| bad("Task success wrap lost carrier"))?;
                carrier.error = false;
                Some(carrier)
            }
        }
        InstructionKind::TaskFinishDestroy { task }
            if f.values.get(task.0 as usize).and_then(shape).is_some() =>
        {
            state.remove(&Owner::Value(*task));
            None
        }
        InstructionKind::DropManaged { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropEnum { local }
            if f.locals
                .get(local.0 as usize)
                .and_then(|l| shape(&l.ty))
                .is_some() =>
        {
            state.remove(&Owner::Local(*local));
            None
        }
        _ if output.is_some() => {
            return Err(bad("Task port carrier has unsupported actual producer"));
        }
        _ => None,
    };
    if let Some(value) = output {
        state.insert(
            Owner::Value(value),
            result.ok_or_else(|| bad("Task port result has no actual carrier"))?,
        );
    }
    Ok(())
}
fn summarize(
    module: &TypedModule,
    plan: &CandidatePlan,
    fi: usize,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    memo: &mut [Option<Summary>],
    active: &mut [bool],
    b: &mut Budget,
) -> CompileResult<Summary> {
    if let Some(summary) = &memo[fi] {
        fee(
            b,
            summary
                .carrier
                .ports
                .iter()
                .fold(1usize, |n, s| n.saturating_add(s.len())),
        )?;
        return Ok(summary.clone());
    }
    if active[fi] {
        return Err(bad(
            "recursive Task port source has no finite proven producer",
        ));
    }
    active[fi] = true;
    let f = &module.functions[fi];
    let count = shape(&f.return_type).map_or(0, |s| s.ports.len());
    let mut initial = State::new();
    for (index, local) in f.parameters.iter().enumerate() {
        fee(b, 1)?;
        if let Some(carrier) = input_carrier(&f.locals[local.0 as usize].ty, index as u32, b)? {
            initial.insert(Owner::Local(*local), carrier);
        }
    }
    let (predecessors, reachable) = &graphs[fi];
    let holds = &plan
        .functions
        .get(fi)
        .ok_or_else(|| bad("Task function plan is absent"))?
        .holds;
    fee(b, f.blocks.len().saturating_mul(3))?;
    let mut outputs = vec![State::new(); f.blocks.len()];
    let mut discovered = vec![false; f.blocks.len()];
    loop {
        let mut changed = false;
        for (index, block) in f.blocks.iter().enumerate() {
            fee(b, 1)?;
            if !reachable[index]
                || (index != f.entry.0 as usize
                    && !predecessors[index].iter().any(|p| discovered[*p]))
            {
                continue;
            }
            if !discovered[index] {
                discovered[index] = true;
                changed = true;
            }
            let mut state = if index == f.entry.0 as usize {
                copy_state(&initial, b)?
            } else {
                State::new()
            };
            for predecessor in &predecessors[index] {
                fee(b, 1)?;
                if *predecessor < f.blocks.len() && reachable[*predecessor] {
                    merge_state(&mut state, &outputs[*predecessor], b)?;
                }
            }
            for instruction in &block.instructions {
                transfer(
                    module,
                    plan,
                    fi,
                    instruction,
                    holds,
                    &mut state,
                    memo,
                    active,
                    graphs,
                    b,
                )?;
            }
            if state != outputs[index] {
                outputs[index] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut returned = Carrier::empty(count);
    let mut saw_return = false;
    for (index, block) in f.blocks.iter().enumerate() {
        fee(b, 1)?;
        if !reachable[index] {
            continue;
        }
        let Terminator::Return(Some(value)) = block.terminator else {
            continue;
        };
        if count == 0 {
            continue;
        }
        saw_return = true;
        let carrier = outputs[index]
            .get(&Owner::Value(value))
            .ok_or_else(|| bad("Task return lost actual port carrier"))?;
        returned.join(carrier, b)?;
    }
    if count > 0 {
        if !saw_return {
            return Err(bad("Task result has no reachable producer"));
        }
        if returned.success {
            for sources in &returned.ports {
                fee(b, sources.len().saturating_add(1))?;
                if sources.is_empty() {
                    return Err(bad("Task success port has no actual source"));
                }
                if sources.iter().any(|s| !matches!(s, Origin::Input(_))) {
                    return Err(bad(
                        "Task success port escapes a non-input or unknown source",
                    ));
                }
            }
            check_contract(f, &returned, b)?;
        }
    }
    active[fi] = false;
    let summary = Summary { carrier: returned };
    memo[fi] = Some(summary.clone());
    Ok(summary)
}
fn check_contract(f: &TypedFunction, carrier: &Carrier, b: &mut Budget) -> CompileResult<()> {
    let contract = f
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("Task result lacks source relation"))?;
    for (port, sources) in carrier.ports.iter().enumerate() {
        fee(
            b,
            contract
                .bindings
                .len()
                .saturating_mul(2)
                .saturating_add(sources.len()),
        )?;
        let result = contract.bindings.iter().find(|row| {
            row.position.base == StorageBase::Result
                && matches!(row.position.path.last(), Some(ContractProjection::RetainedPort(k)) if *k as usize == port)
        }).ok_or_else(|| bad("Task result port lacks declared relation"))?;
        for source in sources {
            let Origin::Input(position) = source else {
                return Err(bad("Task result port source is not an input"));
            };
            if !contract
                .bindings
                .iter()
                .any(|row| row.position == *position && row.binder == result.binder)
            {
                return Err(bad(
                    "Task result port differs from actual input selector or life class",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn check(
    module: &TypedModule,
    plan: &CandidatePlan,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<()> {
    // Hold values are independently produced by Analyze; they identify actual
    // address targets, while this table retains each opaque port selector.
    let mut memo = vec![None; module.functions.len()];
    let mut active = vec![false; module.functions.len()];
    fee(b, module.functions.len().saturating_mul(2))?;
    for (index, f) in module.functions.iter().enumerate() {
        fee(b, f.parameters.len().saturating_add(1))?;
        if shape(&f.return_type).is_some()
            || f.parameters
                .iter()
                .any(|p| shape(&f.locals[p.0 as usize].ty).is_some())
        {
            // Address provenance is supplied only for the actual function.
            let _ = plan
                .functions
                .get(index)
                .ok_or_else(|| bad("Task function plan is absent"))?;
            summarize(module, plan, index, graphs, &mut memo, &mut active, b)?;
        }
    }
    Ok(())
}

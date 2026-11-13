//! Producer-side frame dependency conservation. The input Hold facts were
//! reconstructed by Analyze from this module, never supplied by Verify.
//! A live owning carrier keeps its roots even when it has no later SSA use.
use super::*;
use std::collections::BTreeMap;
mod carriers;
use carriers::{Catalog, Path, Shape};
use zil_ir::{ContractProjection, PlaceProjection};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Value(ValueId),
    Local(LocalId),
    Capture(u32),
}
type Roots = BTreeSet<HoldRoot>;
type Apart = BTreeSet<(u32, u32)>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Debt {
    all: Roots,
    exclusive: Roots,
}
type Carrier = BTreeMap<Path, Debt>;
type State = BTreeMap<Owner, Carrier>;

fn retained_shape(ty: &Type) -> Option<&zil_ir::RetainedLifeShape> {
    if let Some(owner) = super::transfer::outcome_owner(ty) {
        return retained_shape(owner);
    }
    match ty {
        Type::Task { retained_life, .. } if !retained_life.ports.is_empty() => Some(retained_life),
        Type::Managed(inner) | Type::Desc(inner) => retained_shape(inner),
        Type::ErrorUnion { result, .. } => retained_shape(result),
        _ => None,
    }
}
fn fee(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "borrowed Task actual owner dependencies")
}
fn carrier_work(carrier: &Carrier, b: &mut Budget) -> CompileResult<()> {
    fee(b, carrier.len())?;
    for (path, debt) in carrier {
        fee(
            b,
            path.len()
                .saturating_add(debt.all.len())
                .saturating_add(debt.exclusive.len()),
        )?;
    }
    Ok(())
}
fn copy(state: &State, b: &mut Budget) -> CompileResult<State> {
    fee(b, state.len())?;
    for carrier in state.values() {
        carrier_work(carrier, b)?;
    }
    Ok(state.clone())
}
fn merge(to: &mut State, from: &State, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (owner, carrier) in from {
        carrier_work(carrier, b)?;
        for (path, debt) in carrier {
            fee(b, to.len().saturating_add(path.len()).saturating_add(1))?;
            let row = to
                .entry(*owner)
                .or_default()
                .entry(path.clone())
                .or_default();
            let before = (row.all.len(), row.exclusive.len());
            fee(b, row.all.len().saturating_add(row.exclusive.len()))?;
            row.all.extend(&debt.all);
            row.exclusive.extend(&debt.exclusive);
            changed |= before != (row.all.len(), row.exclusive.len());
        }
    }
    Ok(changed)
}
fn take(state: &mut State, owner: Owner, b: &mut Budget) -> CompileResult<Carrier> {
    fee(b, state.len().saturating_add(1))?;
    Ok(state.remove(&owner).unwrap_or_default())
}
fn put(state: &mut State, owner: Owner, carrier: Carrier, b: &mut Budget) -> CompileResult<()> {
    fee(b, state.len().saturating_add(1))?;
    carrier_work(&carrier, b)?;
    if carrier.is_empty() {
        state.remove(&owner);
    } else {
        state.insert(owner, carrier);
    }
    Ok(())
}
fn seed(shape: &Shape, all: &Roots, exclusive: &Roots, b: &mut Budget) -> CompileResult<Carrier> {
    let mut carrier = Carrier::new();
    fee(b, shape.len())?;
    for (path, mutable) in shape {
        fee(
            b,
            path.len()
                .saturating_add(all.len())
                .saturating_add(exclusive.len()),
        )?;
        carrier.insert(
            path.clone(),
            Debt {
                all: all.clone(),
                exclusive: if *mutable {
                    exclusive.clone()
                } else {
                    Roots::new()
                },
            },
        );
    }
    Ok(carrier)
}
fn wrap(carrier: Carrier, prefix: ContractProjection, b: &mut Budget) -> CompileResult<Carrier> {
    let mut out = Carrier::new();
    carrier_work(&carrier, b)?;
    for (path, debt) in carrier {
        fee(b, path.len().saturating_add(1))?;
        let mut selected = vec![prefix];
        selected.extend(path);
        out.insert(selected, debt);
    }
    Ok(out)
}
fn project(
    carrier: &mut Carrier,
    prefix: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<Carrier> {
    carrier_work(carrier, b)?;
    fee(
        b,
        carrier.len().saturating_mul(prefix.len().saturating_add(1)),
    )?;
    let mut out = Carrier::new();
    let mut rest = Carrier::new();
    for (path, debt) in std::mem::take(carrier) {
        if path.starts_with(prefix) {
            out.insert(path[prefix.len()..].to_vec(), debt);
        } else {
            rest.insert(path, debt);
        }
    }
    *carrier = rest;
    Ok(out)
}
fn actual(holds: &[ValueHold], value: ValueId, b: &mut Budget) -> CompileResult<Roots> {
    let row = holds
        .get(value.0 as usize)
        .ok_or_else(|| bad("Task dependency Hold value absent"))?;
    fee(b, row.roots.len().saturating_add(1))?;
    if row.value != value {
        return Err(bad("Task dependency Hold identity differs"));
    }
    Ok(row.roots.iter().copied().collect())
}

fn physical(
    f: &TypedFunction,
    holds: &[ValueHold],
    value: ValueId,
    b: &mut Budget,
) -> CompileResult<Roots> {
    if !b.task_sources() || !matches!(f.values[value.0 as usize], Type::Address { .. }) {
        return actual(holds, value, b);
    }
    if b.masked_stored_tasks() {
        fee(b, 1)?;
        if !b
            .task_stored_coverage
            .get(f.id.0 as usize)
            .copied()
            .ok_or_else(|| bad("Task physical access lacks function coverage"))?
        {
            return actual(holds, value, b);
        }
    }
    let roots = b
        .task_pointer_roots
        .get(f.id.0 as usize)
        .and_then(|rows| rows.get(value.0 as usize))
        .ok_or_else(|| bad("Task physical access lacks an actual SSA target proof"))?;
    let work = roots.len().saturating_add(1);
    fee(b, work)?;
    Ok(b.task_pointer_roots[f.id.0 as usize][value.0 as usize]
        .iter()
        .copied()
        .collect())
}

fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    holds: &[ValueHold],
    catalog: &Catalog,
    state: &mut State,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    let output = i
        .result
        .filter(|v| !catalog.values[v.0 as usize].is_empty());
    let roots = match &i.kind {
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
            let all = actual(holds, output.unwrap(), b)?;
            if all.is_empty() {
                return Err(fail(
                    i.span,
                    "borrowed Task constructor lacks an actual input root",
                ));
            }
            let shape = retained_shape(&f.values[output.unwrap().0 as usize])
                .ok_or_else(|| bad("borrowed Task constructor retained shape absent"))?;
            let mut exclusive = Roots::new();
            fee(b, port_bindings.len())?;
            for binding in port_bindings {
                let port = shape
                    .ports
                    .get(binding.port as usize)
                    .ok_or_else(|| bad("Task exclusive source port absent"))?;
                if port.mode != zil_ir::StoredBorrowMode::ExclusiveAddress {
                    continue;
                }
                let zil_ir::StorageBase::Parameter(index) = binding.source.base else {
                    return Err(bad("Task exclusive port is not a worker parameter"));
                };
                if !binding.source.path.is_empty() {
                    return Err(bad("Task exclusive port path is unproved"));
                }
                let value = *arguments
                    .get(index as usize)
                    .ok_or_else(|| bad("Task exclusive port argument absent"))?;
                exclusive.extend(actual(holds, value, b)?);
            }
            Some(seed(
                &catalog.values[output.unwrap().0 as usize],
                &all,
                &exclusive,
                b,
            )?)
        }
        InstructionKind::WithScopedThread { argument, .. } if output.is_some() => Some(wrap(
            take(state, Owner::Value(*argument), b)?,
            ContractProjection::Place(PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: 0,
                index: 0,
            })),
            b,
        )?),
        InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } => {
            let mut original = take(state, Owner::Local(*local), b)?;
            for (index, target) in fields.iter().enumerate() {
                let prefix = [ContractProjection::Place(PlaceProjection::Variant(
                    zil_ir::VariantProjection {
                        variant: *variant,
                        index: index as u32,
                    },
                ))];
                let selected = project(&mut original, &prefix, b)?;
                put(state, Owner::Local(*target), selected, b)?;
            }
            None
        }
        InstructionKind::MakeClosure { captures, .. } => {
            let mut out = Carrier::new();
            for (index, value) in captures.iter().enumerate() {
                let child = wrap(
                    take(state, Owner::Value(*value), b)?,
                    ContractProjection::Capture(index as u32),
                    b,
                )?;
                carrier_work(&child, b)?;
                out.extend(child);
            }
            Some(out)
        }
        InstructionKind::MakePackage { fields, .. }
        | InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakeOwnedTuple { fields } => {
            let mut out = Carrier::new();
            for (index, value) in fields.iter().enumerate() {
                let child = wrap(
                    take(state, Owner::Value(*value), b)?,
                    ContractProjection::Place(PlaceProjection::Field(index as u32)),
                    b,
                )?;
                carrier_work(&child, b)?;
                out.extend(child);
            }
            Some(out)
        }
        InstructionKind::TakeCapture { index } => Some(take(state, Owner::Capture(*index), b)?),
        InstructionKind::CallClosureOnce { callee, arguments } => {
            let environment = take(state, Owner::Value(*callee), b)?;
            let mut transferred = Roots::new();
            for debt in environment.values() {
                fee(b, debt.all.len())?;
                transferred.extend(&debt.all);
            }
            for value in arguments {
                for debt in take(state, Owner::Value(*value), b)?.values() {
                    fee(b, debt.all.len())?;
                    transferred.extend(&debt.all);
                }
            }
            output.map(|output| {
                let ports = b.task_retained_roots.get(f.id.0 as usize)
                    .and_then(|rows| rows.get(output.0 as usize))
                    .ok_or_else(|| fail(i.span,"returned Task lacks its independently reconstructed actual ports"))?;
                let work = ports.iter().fold(1usize, |work,(path,roots)| work.saturating_add(path.len()).saturating_add(roots.len()));
                fee(b,work)?;
                let ports = &b.task_retained_roots[f.id.0 as usize][output.0 as usize];
                let mut returned = Carrier::new();
                for (owner,exclusive) in &catalog.values[output.0 as usize] {
                    let mut all = Roots::new();
                    for (path,roots) in ports {
                        if matches!(path.strip_prefix(owner.as_slice()),Some([ContractProjection::RetainedPort(_)])) {
                            all.extend(roots);
                        }
                    }
                    if all.is_empty() || all.iter().any(|root| *root!=HoldRoot::Static && !transferred.contains(root)) {
                        return Err(fail(i.span,"returned Task dependency is not an actual transferred environment frame root"));
                    }
                    returned.insert(owner.clone(),Debt {
                        exclusive: if *exclusive {all.clone()} else {Roots::new()},
                        all,
                    });
                }
                carrier_work(&returned,b)?;
                Ok(returned)
            }).transpose()?
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            // Ownership arguments are discharged by the independently checked
            // callee. Its returned carrier starts with the actual return roots.
            let mut carried_exclusive = Roots::new();
            for argument in arguments {
                fee(b, 1)?;
                if !catalog.values[argument.0 as usize].is_empty() {
                    if module.functions[function.0 as usize].foreign.is_some() {
                        return Err(fail(
                            i.span,
                            "borrowed Task cannot escape through an unverified foreign owner transfer",
                        ));
                    }
                    for debt in take(state, Owner::Value(*argument), b)?.values() {
                        fee(b, debt.exclusive.len())?;
                        carried_exclusive.extend(&debt.exclusive);
                    }
                }
            }
            if output.is_some() && module.functions[function.0 as usize].foreign.is_some() {
                return Err(fail(
                    i.span,
                    "foreign Task result has no reconstructed frame dependency",
                ));
            }
            output
                .map(|v| {
                    let all = actual(holds, v, b)?;
                    let mut exclusive = carried_exclusive;
                    if catalog.values[v.0 as usize]
                        .values()
                        .any(|exclusive| *exclusive)
                    {
                        // A callee may create a fresh frame from any address input.
                        // Existing Task inputs retain their exact exclusive roots;
                        // address inputs add their actual target roots conservatively.
                        for argument in arguments {
                            fee(b, 1)?;
                            if matches!(f.values[argument.0 as usize], Type::Address { .. }) {
                                exclusive.extend(actual(holds, *argument, b)?);
                            }
                        }
                        if exclusive.is_empty() {
                            exclusive.extend(&all);
                        }
                    }
                    // The callee's actual capture sources and per-port relation
                    // are checked by the independent retained/stored consumers.
                    if all.is_empty() {
                        return Err(fail(
                            i.span,
                            "returned Task carrier lacks reconstructed actual input roots",
                        ));
                    }
                    seed(&catalog.values[v.0 as usize], &all, &exclusive, b)
                })
                .transpose()?
        }
        InstructionKind::Store { local, value } if !catalog.locals[local.0 as usize].is_empty() => {
            let debt = take(state, Owner::Value(*value), b)?;
            put(state, Owner::Local(*local), debt, b)?;
            None
        }
        InstructionKind::TakeLocal { local } if !catalog.locals[local.0 as usize].is_empty() => {
            Some(take(state, Owner::Local(*local), b)?)
        }
        InstructionKind::TakeVariantField { local, path }
        | InstructionKind::TakeTupleField { local, path }
            if !catalog.locals[local.0 as usize].is_empty() =>
        {
            let mut carrier = take(state, Owner::Local(*local), b)?;
            fee(b, path.len())?;
            let prefix = path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            let selected = project(&mut carrier, &prefix, b)?;
            put(state, Owner::Local(*local), carrier, b)?;
            Some(selected)
        }
        InstructionKind::TakePackageField { local, index } => {
            let mut carrier = take(state, Owner::Local(*local), b)?;
            let selected = project(
                &mut carrier,
                &[ContractProjection::Place(PlaceProjection::Field(*index))],
                b,
            )?;
            put(state, Owner::Local(*local), carrier, b)?;
            Some(selected)
        }
        InstructionKind::UnpackTuple { local, fields } => {
            let mut carrier = take(state, Owner::Local(*local), b)?;
            for (index, target) in fields.iter().enumerate() {
                let selected = project(
                    &mut carrier,
                    &[ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    ))],
                    b,
                )?;
                put(state, Owner::Local(*target), selected, b)?;
            }
            if !carrier.is_empty() {
                return Err(fail(
                    i.span,
                    "Task carrier tuple unpack leaves an untransported frame responsibility",
                ));
            }
            None
        }
        InstructionKind::TakeErrorUnion { value, is_error }
            if !catalog.values[value.0 as usize].is_empty() =>
        {
            let debt = take(state, Owner::Value(*value), b)?;
            let mut debt = debt;
            Some(project(
                &mut debt,
                &[ContractProjection::Place(PlaceProjection::ErrorArm(
                    *is_error,
                ))],
                b,
            )?)
        }
        InstructionKind::MakeErrorUnion { value, is_error } if output.is_some() => {
            Some(if *is_error {
                Carrier::new()
            } else {
                wrap(
                    take(state, Owner::Value(*value), b)?,
                    ContractProjection::Place(PlaceProjection::ErrorArm(false)),
                    b,
                )?
            })
        }
        InstructionKind::TaskFinishDestroy { task }
            if !catalog.values[task.0 as usize].is_empty() =>
        {
            take(state, Owner::Value(*task), b)?;
            None
        }
        InstructionKind::DropManaged { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropClosure { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropTuple { local }
            if !catalog.locals[local.0 as usize].is_empty() =>
        {
            // Task admission separately proves the reachable teardown is
            // synchronous; only completion of this instruction releases it.
            take(state, Owner::Local(*local), b)?;
            None
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local)
            if !catalog.locals[local.0 as usize].is_empty() =>
        {
            if state.contains_key(&Owner::Local(*local)) {
                return Err(fail(
                    i.span,
                    "Task storage lifetime ends before its actual owner is discharged",
                ));
            }
            None
        }
        InstructionKind::ErrorTag { .. } => None,
        _ if output.is_some() => {
            return Err(fail(
                i.span,
                "borrowed Task owner has no supported actual transfer",
            ));
        }
        _ => {
            for operand in resources::uses(&i.kind) {
                fee(b, 1)?;
                if !catalog.values[operand.0 as usize].is_empty() {
                    return Err(fail(
                        i.span,
                        "borrowed Task transport requires a checked complete owner transfer",
                    ));
                }
            }
            None
        }
    };
    if let Some(value) = output {
        put(state, Owner::Value(value), roots.unwrap_or_default(), b)?;
    }
    Ok(())
}

// Distinct local storage is disjoint. Distinct incoming pointer spellings are
// not a separation proof; this first profile conservatively overlaps them.
fn overlaps(left: HoldRoot, right: HoldRoot) -> bool {
    left == right
        || matches!(
            (left, right),
            (HoldRoot::Parameter(_), HoldRoot::Parameter(_))
                | (HoldRoot::Parameter(_), HoldRoot::Capture(_))
                | (HoldRoot::Capture(_), HoldRoot::Parameter(_))
                | (HoldRoot::Capture(_), HoldRoot::Capture(_))
        )
}
fn conflict(
    left: HoldRoot,
    right: HoldRoot,
    apart: &mut Apart,
    b: &mut Budget,
) -> CompileResult<bool> {
    if let (HoldRoot::Parameter(a), HoldRoot::Parameter(c)) = (left, right)
        && a != c
    {
        fee(b, apart.len().saturating_add(1))?;
        apart.insert((a.min(c), a.max(c)));
        return Ok(false);
    }
    Ok(overlaps(left, right))
}
fn exclusive(
    state: &State,
    roots: &Roots,
    i: &Instruction,
    apart: &mut Apart,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, state.len())?;
    for debt in state.values().flat_map(|carrier| carrier.values()) {
        let held = &debt.all;
        b.product(
            roots.len(),
            held.len(),
            1,
            "borrowed Task protected root overlap",
        )?;
        for root in roots {
            for source in held {
                if conflict(*root, *source, apart, b)? {
                    return Err(fail(
                        i.span,
                        "exclusive access or storage end conflicts with a live borrowed Task frame",
                    ));
                }
            }
        }
    }
    Ok(())
}
fn shared_access(
    _f: &TypedFunction,
    state: &State,
    roots: &Roots,
    i: &Instruction,
    apart: &mut Apart,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, state.len())?;
    for debt in state.values().flat_map(|carrier| carrier.values()) {
        let held = &debt.exclusive;
        fee(b, held.len().saturating_add(1))?;
        b.product(
            roots.len(),
            held.len(),
            1,
            "Task exclusive port shared read",
        )?;
        for root in roots {
            for source in held {
                if conflict(*root, *source, apart, b)? {
                    return Err(fail(
                        i.span,
                        "shared access conflicts with a live exclusive Task frame port",
                    ));
                }
            }
        }
    }
    Ok(())
}
fn access(
    module: &TypedModule,
    f: &TypedFunction,
    catalog: &Catalog,
    i: &Instruction,
    holds: &[ValueHold],
    state: &State,
    apart: &mut Apart,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut consumed = Vec::new();
    match &i.kind {
        InstructionKind::Store { value, .. }
        | InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::TakeErrorUnion { value, .. } => consumed.push(*value),
        InstructionKind::MakeClosure {
            captures: values, ..
        }
        | InstructionKind::MakePackage { fields: values }
        | InstructionKind::MakeOwnedTuple { fields: values }
        | InstructionKind::MakeAggregate { fields: values }
        | InstructionKind::Call {
            arguments: values, ..
        } => {
            fee(b, values.len())?;
            consumed.extend_from_slice(values);
        }
        InstructionKind::CallClosureOnce { callee, arguments } => {
            fee(b, arguments.len().saturating_add(1))?;
            consumed.push(*callee);
            consumed.extend_from_slice(arguments);
        }
        _ => {}
    }
    let mut remaining;
    let state = if consumed
        .iter()
        .any(|value| !catalog.values[value.0 as usize].is_empty())
    {
        remaining = copy(state, b)?;
        for value in consumed {
            take(&mut remaining, Owner::Value(value), b)?;
        }
        &remaining
    } else {
        state
    };
    if state.is_empty() {
        return Ok(());
    }
    fee(b, 1)?;
    let local = match i.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropTuple { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::BorrowFieldLocal {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowPlaceLocal {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowVariantLocal {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowPackageField {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowOwnedArrayElement {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::StorePackageField { local, .. }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StoreOwnedArrayElement { local, .. }
        | InstructionKind::TakeOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArrayElement { local, .. } => Some(local),
        _ => None,
    };
    if let Some(local) = local {
        exclusive(state, &Roots::from([HoldRoot::Local(local)]), i, apart, b)?;
    }
    let read_local = match i.kind {
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowFieldLocal {
            local,
            mutable: false,
            ..
        }
        | InstructionKind::BorrowPlaceLocal {
            local,
            mutable: false,
            ..
        }
        | InstructionKind::BorrowVariantLocal {
            local,
            mutable: false,
            ..
        }
        | InstructionKind::BorrowPackageField {
            local,
            mutable: false,
            ..
        }
        | InstructionKind::BorrowOwnedArrayElement {
            local,
            mutable: false,
            ..
        }
        | InstructionKind::ReadPackageField { local, .. }
        | InstructionKind::ReadVariantLocal { local, .. } => Some(local),
        _ => None,
    };
    if let Some(local) = read_local
        && !f
            .locals
            .get(local.0 as usize)
            .is_some_and(|_| !catalog.locals[local.0 as usize].is_empty())
    {
        shared_access(
            f,
            state,
            &Roots::from([HoldRoot::Local(local)]),
            i,
            apart,
            b,
        )?;
    }
    match i.kind {
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => {
            let (definition, _) = crate::stored_callables::called(module, f, &i.kind)
                .ok_or_else(|| fail(i.span, "Task callable access has no actual receiver entry"))?;
            if !crate::stored_callables::scalar(&definition.result) {
                return Err(fail(
                    i.span,
                    "Task callable result needs its complete retained transport",
                ));
            }
            fee(
                b,
                definition
                    .captures
                    .len()
                    .saturating_add(definition.parameters.len()),
            )?;
            for parameter in &definition.parameters {
                b.type_work(parameter, false, "Task callable actual scalar input")?;
                if !crate::stored_callables::scalar(parameter) {
                    return Err(fail(
                        i.span,
                        "Task callable external inputs require exact effect substitution",
                    ));
                }
            }
            for capture in &definition.captures {
                b.type_work(capture, false, "Task callable complete captured dependency")?;
                if let Type::Managed(payload) = capture
                    && matches!(payload.as_ref(), Type::Task { .. })
                {
                    super::require_closed_context(payload)?;
                } else if !crate::stored_callables::scalar(capture) {
                    return Err(fail(
                        i.span,
                        "Task callable borrowed captures require exact effect substitution",
                    ));
                }
            }
            // This actual entry can touch external data only through its own
            // captured frames. Preserve that complete debt, and compare it
            // with every other live owner even without a future SSA use.
            let owner = Owner::Local(callee);
            let carried = state.get(&owner);
            if !catalog.locals[callee.0 as usize].is_empty() && carried.is_none() {
                return Err(fail(
                    i.span,
                    "Task callable lost its actual captured frame responsibility",
                ));
            }
            if let Some(carried) = carried {
                carrier_work(carried, b)?;
                fee(b, state.len())?;
                for (other_owner, other) in state {
                    if *other_owner == owner {
                        continue;
                    }
                    carrier_work(other, b)?;
                    for used in carried.values() {
                        for protected in other.values() {
                            let reads = if used.exclusive.is_empty() {
                                &protected.exclusive
                            } else {
                                &protected.all
                            };
                            b.product(
                                used.all.len(),
                                reads.len(),
                                1,
                                "Task callable actual competing roots",
                            )?;
                            for left in &used.all {
                                for right in reads {
                                    if conflict(*left, *right, apart, b)? {
                                        return Err(fail(
                                            i.span,
                                            "Task callable access conflicts with another live frame",
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        InstructionKind::BorrowCapture { mutable: true, .. }
            if crate::stored_flow::task_capture_instruction(module, f, i, b)? =>
        {
            // This borrows the owning slot, not any retained data leaf. Actual
            // Task contexts admit only the separately checked data referents;
            // the head/receiver loan check protects the real capture slot.
        }
        InstructionKind::WriteCapture { index, .. }
        | InstructionKind::BorrowCapture {
            index,
            mutable: true,
        } => {
            exclusive(state, &Roots::from([HoldRoot::Capture(index)]), i, apart, b)?;
        }
        InstructionKind::CommitPlaceReplace { reservation, .. } => {
            // A reservation may predate creation of the Task. Check its actual
            // destination at commit, rather than treating reservation as write.
            for block in &f.blocks {
                fee(b, block.instructions.len().saturating_add(1))?;
                for producer in &block.instructions {
                    match producer.kind {
                        InstructionKind::ReservePlaceLocal {
                            reservation: id,
                            local,
                            ..
                        } if id == reservation => {
                            exclusive(state, &Roots::from([HoldRoot::Local(local)]), i, apart, b)?;
                        }
                        InstructionKind::ReservePlaceAddress {
                            reservation: id,
                            address,
                            ..
                        } if id == reservation => {
                            exclusive(state, &physical(f, holds, address, b)?, i, apart, b)?;
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
    let address = match i.kind {
        InstructionKind::WriteAddress { address, .. }
        | InstructionKind::ReborrowAddress {
            address,
            mutable: true,
        }
        | InstructionKind::BorrowVariantAddress {
            address,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowPlaceAddress {
            address,
            mutable: true,
            ..
        } => Some(address),
        _ => None,
    };
    if let Some(value) = address {
        if f.values[value.0 as usize].is_exclusive_task_head_address() {
            if matches!(i.kind, InstructionKind::WriteAddress { .. }) {
                return Err(fail(
                    i.span,
                    "whole Task head replacement needs exact frame debt transport",
                ));
            }
            // A Task-slot reborrow preserves its existing frame. Head windows
            // have their own actual source/parent checker; it does not write
            // the frame's retained data merely by borrowing the slot.
        } else {
            exclusive(state, &physical(f, holds, value, b)?, i, apart, b)?;
        }
    }
    // Existing addresses can be used after Task construction. Checking only
    // BorrowLocalMut would miss writes through an earlier reference or helper.
    for value in resources::uses(&i.kind) {
        fee(b, 1)?;
        if f.values[value.0 as usize].is_exclusive_task_head_address() {
            continue;
        }
        if matches!(
            &f.values[value.0 as usize],
            Type::Address { mutable: false, .. }
        ) {
            shared_access(f, state, &physical(f, holds, value, b)?, i, apart, b)?;
        }
        if matches!(
            &f.values[value.0 as usize],
            Type::Address { mutable: true, .. }
        ) {
            exclusive(state, &physical(f, holds, value, b)?, i, apart, b)?;
        }
        if matches!(&f.values[value.0 as usize], Type::Closure(_)) {
            exclusive(state, &physical(f, holds, value, b)?, i, apart, b)?;
        }
    }
    Ok(())
}

fn prove_apart(
    module: &TypedModule,
    plan: &CandidatePlan,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    apart: &mut [Apart],
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, apart.len())?;
    if apart.iter().all(Apart::is_empty) {
        return Ok(());
    }
    loop {
        let mut changed = false;
        for (caller, (f, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
            let holds = &plan.functions[caller].holds;
            for (block, live) in f.blocks.iter().zip(reachable) {
                fee(b, 1)?;
                if !live {
                    continue;
                }
                for instruction in &block.instructions {
                    fee(b, 1)?;
                    match &instruction.kind {
                        InstructionKind::FunctionRef { function }
                            if !apart.get(function.0 as usize).is_some_and(Apart::is_empty) =>
                        {
                            return Err(fail(
                                instruction.span,
                                "Task port separation cannot pass through an indirect function reference",
                            ));
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            let target = function.0 as usize;
                            let rows = apart
                                .get(target)
                                .ok_or_else(|| bad("Task port separation callee absent"))?;
                            fee(b, rows.len())?;
                            let requirements = rows.iter().copied().collect::<Vec<_>>();
                            for (left, right) in requirements {
                                let left = *arguments
                                    .get(left as usize)
                                    .ok_or_else(|| bad("Task port separation input absent"))?;
                                let right = *arguments
                                    .get(right as usize)
                                    .ok_or_else(|| bad("Task port separation input absent"))?;
                                let a = physical(f, holds, left, b)?;
                                let c = physical(f, holds, right, b)?;
                                if a.is_empty() || c.is_empty() {
                                    return Err(fail(
                                        instruction.span,
                                        "Task port separation has no actual input root",
                                    ));
                                }
                                b.product(
                                    a.len(),
                                    c.len(),
                                    1,
                                    "Task port actual caller separation",
                                )?;
                                for x in &a {
                                    for y in &c {
                                        let before = apart[caller].len();
                                        if conflict(*x, *y, &mut apart[caller], b)? {
                                            return Err(fail(
                                                instruction.span,
                                                "actual caller aliases Task ports requiring separation",
                                            ));
                                        }
                                        changed |= apart[caller].len() != before;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if !changed {
            break;
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
    fee(b, module.functions.len())?;
    let mut apart = vec![Apart::new(); module.functions.len()];
    for (fi, f) in module.functions.iter().enumerate() {
        fee(b, f.values.len().saturating_add(f.locals.len()))?;
        let catalog = Catalog::new(module, f, b)?;
        if catalog
            .values
            .iter()
            .chain(&catalog.locals)
            .chain(&catalog.captures)
            .all(Shape::is_empty)
        {
            continue;
        }
        if f.foreign.is_some() {
            continue;
        }
        let holds = &plan.functions[fi].holds;
        let (predecessors, reachable) = &graphs[fi];
        fee(
            b,
            f.blocks
                .len()
                .saturating_mul(3)
                .saturating_add(f.values.len()),
        )?;
        let mut tags = vec![None; f.values.len()];
        for block in &f.blocks {
            for i in &block.instructions {
                fee(b, 1)?;
                if let (Some(result), InstructionKind::ErrorTag { value }) = (i.result, &i.kind)
                    && !catalog.values[value.0 as usize].is_empty()
                {
                    tags[result.0 as usize] = Some(*value);
                }
            }
        }
        let mut initial = State::new();
        for (index, local) in f.parameters.iter().enumerate() {
            fee(b, 1)?;
            if !catalog.locals[local.0 as usize].is_empty() {
                let all = Roots::from([HoldRoot::Parameter(index as u32)]);
                let exclusive = all.clone();
                let carrier = seed(&catalog.locals[local.0 as usize], &all, &exclusive, b)?;
                put(&mut initial, Owner::Local(*local), carrier, b)?;
            }
        }
        for (index, shape) in catalog.captures.iter().enumerate() {
            let roots = Roots::from([HoldRoot::Capture(index as u32)]);
            let carrier = seed(shape, &roots, &roots, b)?;
            put(&mut initial, Owner::Capture(index as u32), carrier, b)?;
        }
        let mut inputs = vec![State::new(); f.blocks.len()];
        let mut outputs = vec![State::new(); f.blocks.len()];
        loop {
            let mut changed = false;
            for (index, block) in f.blocks.iter().enumerate() {
                fee(b, 1)?;
                if !reachable[index] {
                    continue;
                }
                let mut state = if index == f.entry.0 as usize {
                    copy(&initial, b)?
                } else {
                    State::new()
                };
                for predecessor in &predecessors[index] {
                    fee(b, 1)?;
                    if !reachable[*predecessor] {
                        continue;
                    }
                    let mut edge = copy(&outputs[*predecessor], b)?;
                    if let Terminator::Branch {
                        condition,
                        then_block,
                        else_block,
                    } = f.blocks[*predecessor].terminator
                    {
                        // ErrorTag=true means no successful Task frame exists.
                        // An unknown condition never grants this refinement.
                        if then_block != else_block
                            && then_block.0 as usize == index
                            && let Some(value) = tags[condition.0 as usize]
                        {
                            take(&mut edge, Owner::Value(value), b)?;
                        }
                    }
                    merge(&mut state, &edge, b)?;
                }
                inputs[index] = copy(&state, b)?;
                for i in &block.instructions {
                    transfer(module, f, i, holds, &catalog, &mut state, b)?;
                }
                fee(b, state.len().saturating_add(outputs[index].len()))?;
                for carrier in state.values().chain(outputs[index].values()) {
                    carrier_work(carrier, b)?;
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
        for (index, block) in f.blocks.iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            let mut state = copy(&inputs[index], b)?;
            for i in &block.instructions {
                access(module, f, &catalog, i, holds, &state, &mut apart[fi], b)?;
                transfer(module, f, i, holds, &catalog, &mut state, b)?;
            }
        }
    }
    prove_apart(module, plan, graphs, &mut apart, b)?;
    Ok(())
}

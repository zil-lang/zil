//! Producer-side frame dependency conservation. The input Hold facts were
//! reconstructed by Analyze from this module, never supplied by Verify.
//! A live owning carrier keeps its roots even when it has no later SSA use.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Value(ValueId),
    Local(LocalId),
}
type Roots = BTreeSet<HoldRoot>;
type Apart = BTreeSet<(u32, u32)>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Debt {
    all: Roots,
    exclusive: Roots,
}
type State = BTreeMap<Owner, Debt>;

fn retained(ty: &Type) -> bool {
    if let Some(owner) = super::transfer::outcome_owner(ty) {
        return retained(owner);
    }
    match ty {
        Type::Managed(payload) => {
            matches!(payload.as_ref(), Type::Task { retained_life, .. } if !retained_life.ports.is_empty())
        }
        Type::ErrorUnion { result, .. } => retained(result),
        _ => false,
    }
}
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
fn copy(state: &State, b: &mut Budget) -> CompileResult<State> {
    fee(b, state.len())?;
    for debt in state.values() {
        fee(b, debt.all.len().saturating_add(debt.exclusive.len()))?;
    }
    Ok(state.clone())
}
fn merge(to: &mut State, from: &State, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (owner, debt) in from {
        fee(
            b,
            to.len()
                .saturating_add(debt.all.len())
                .saturating_add(debt.exclusive.len())
                .saturating_add(1),
        )?;
        let entry = to.entry(*owner).or_default();
        let before = (entry.all.len(), entry.exclusive.len());
        entry.all.extend(&debt.all);
        entry.exclusive.extend(&debt.exclusive);
        changed |= (entry.all.len(), entry.exclusive.len()) != before;
    }
    Ok(changed)
}
fn take(state: &mut State, owner: Owner, b: &mut Budget) -> CompileResult<Debt> {
    fee(b, state.len().saturating_add(1))?;
    Ok(state.remove(&owner).unwrap_or_default())
}
fn put(state: &mut State, owner: Owner, debt: Debt, b: &mut Budget) -> CompileResult<()> {
    fee(
        b,
        state
            .len()
            .saturating_add(debt.all.len())
            .saturating_add(debt.exclusive.len())
            .saturating_add(1),
    )?;
    if debt.all.is_empty() && debt.exclusive.is_empty() {
        state.remove(&owner);
    } else {
        state.insert(owner, debt);
    }
    Ok(())
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
    state: &mut State,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    let output = i.result.filter(|v| retained(&f.values[v.0 as usize]));
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
            Some(Debt { all, exclusive })
        }
        InstructionKind::WithScopedThread { argument, .. } if output.is_some() => {
            Some(take(state, Owner::Value(*argument), b)?)
        }
        InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } if super::transfer::outcome_owner(&f.locals[local.0 as usize].ty).is_some() => {
            let debt = take(state, Owner::Local(*local), b)?;
            if *variant == 0 {
                let target = *fields
                    .first()
                    .ok_or_else(|| bad("Task failure has no owner field"))?;
                put(state, Owner::Local(target), debt, b)?;
            }
            None
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
                if retained(&f.values[argument.0 as usize]) {
                    if module.functions[function.0 as usize].foreign.is_some() {
                        return Err(fail(
                            i.span,
                            "borrowed Task cannot escape through an unverified foreign owner transfer",
                        ));
                    }
                    carried_exclusive.extend(take(state, Owner::Value(*argument), b)?.exclusive);
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
                    if exclusive_port(&f.values[v.0 as usize]) {
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
                    } else {
                        exclusive.clear();
                    }
                    Ok::<Debt, Vec<Diagnostic>>(Debt { all, exclusive })
                })
                .transpose()?
        }
        InstructionKind::Store { local, value } if retained(&f.locals[local.0 as usize].ty) => {
            let debt = take(state, Owner::Value(*value), b)?;
            put(state, Owner::Local(*local), debt, b)?;
            None
        }
        InstructionKind::TakeLocal { local } if retained(&f.locals[local.0 as usize].ty) => {
            Some(take(state, Owner::Local(*local), b)?)
        }
        InstructionKind::TakeVariantField { local, path }
            if retained(&f.locals[local.0 as usize].ty) =>
        {
            // The independent Sum pass has already checked the actual tag
            // guard and that this selective take consumes the owning shell.
            let debt = take(state, Owner::Local(*local), b)?;
            match path.as_slice() {
                [zil_ir::PlaceProjection::ErrorArm(false)]
                | [
                    zil_ir::PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant: 0,
                        index: 0,
                    }),
                ] => Some(debt),
                [zil_ir::PlaceProjection::ErrorArm(true)] => None,
                _ => {
                    return Err(fail(
                        i.span,
                        "borrowed Task selective take has no complete error arm",
                    ));
                }
            }
        }
        InstructionKind::TakeErrorUnion { value, is_error }
            if retained(&f.values[value.0 as usize]) =>
        {
            let debt = take(state, Owner::Value(*value), b)?;
            if *is_error { None } else { Some(debt) }
        }
        InstructionKind::MakeErrorUnion { value, is_error } if output.is_some() => {
            Some(if *is_error {
                Debt::default()
            } else {
                take(state, Owner::Value(*value), b)?
            })
        }
        InstructionKind::TaskFinishDestroy { task } if retained(&f.values[task.0 as usize]) => {
            take(state, Owner::Value(*task), b)?;
            None
        }
        InstructionKind::DropManaged { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropEnum { local }
            if retained(&f.locals[local.0 as usize].ty) =>
        {
            // Task admission separately proves the reachable teardown is
            // synchronous; only completion of this instruction releases it.
            take(state, Owner::Local(*local), b)?;
            None
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local)
            if retained(&f.locals[local.0 as usize].ty) =>
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
                if retained(&f.values[operand.0 as usize]) {
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
    for debt in state.values() {
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
fn exclusive_port(ty: &Type) -> bool {
    retained_shape(ty).is_some_and(|shape| {
        shape
            .ports
            .iter()
            .any(|p| p.mode == zil_ir::StoredBorrowMode::ExclusiveAddress)
    })
}
fn shared_access(
    f: &TypedFunction,
    state: &State,
    roots: &Roots,
    i: &Instruction,
    apart: &mut Apart,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, state.len())?;
    for (owner, debt) in state {
        let held = &debt.exclusive;
        fee(b, held.len().saturating_add(1))?;
        let ty = match owner {
            Owner::Value(value) => f.values.get(value.0 as usize),
            Owner::Local(local) => f.locals.get(local.0 as usize).map(|l| &l.ty),
        }
        .ok_or_else(|| bad("Task exclusive owner type is absent"))?;
        b.type_work(ty, false, "Task exclusive owner port modes")?;
        if !exclusive_port(ty) {
            continue;
        }
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
    f: &TypedFunction,
    i: &Instruction,
    holds: &[ValueHold],
    state: &State,
    apart: &mut Apart,
    b: &mut Budget,
) -> CompileResult<()> {
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
            .is_some_and(|l| retained(&l.ty))
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
        InstructionKind::CallClosureShared { .. }
        | InstructionKind::CallClosureMut { .. }
        | InstructionKind::CallClosureOnce { .. } => {
            return Err(fail(
                i.span,
                "closure environment effects during a borrowed Task require the retained capture consumer",
            ));
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
        exclusive(state, &physical(f, holds, value, b)?, i, apart, b)?;
    }
    // Existing addresses can be used after Task construction. Checking only
    // BorrowLocalMut would miss writes through an earlier reference or helper.
    for value in resources::uses(&i.kind) {
        fee(b, 1)?;
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
        if !f.values.iter().any(retained) && !f.locals.iter().any(|l| retained(&l.ty)) {
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
                    && retained(&f.values[value.0 as usize])
                {
                    tags[result.0 as usize] = Some(*value);
                }
            }
        }
        let mut initial = State::new();
        for (index, local) in f.parameters.iter().enumerate() {
            fee(b, 1)?;
            if retained(&f.locals[local.0 as usize].ty) {
                let all = Roots::from([HoldRoot::Parameter(index as u32)]);
                let exclusive = if exclusive_port(&f.locals[local.0 as usize].ty) {
                    all.clone()
                } else {
                    Roots::new()
                };
                put(
                    &mut initial,
                    Owner::Local(*local),
                    Debt { all, exclusive },
                    b,
                )?;
            }
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
                    transfer(module, f, i, holds, &mut state, b)?;
                }
                fee(b, state.len().saturating_add(outputs[index].len()))?;
                for debt in state.values().chain(outputs[index].values()) {
                    fee(b, debt.all.len().saturating_add(debt.exclusive.len()))?;
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
                access(f, i, holds, &state, &mut apart[fi], b)?;
                transfer(module, f, i, holds, &mut state, b)?;
            }
        }
    }
    prove_apart(module, plan, graphs, &mut apart, b)?;
    Ok(())
}

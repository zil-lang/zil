//! Analyze's private, position-sensitive proof of callable parameter returns.
//! Public source/kind tags are claims. Only checked body/call substitutions
//! establish a relation, and no fact from Types or Verify is consumed here.
use super::helpers::Origin;
use super::*;
#[cfg(test)]
#[path = "forward_tests.rs"]
mod tests;

pub(super) fn parameter(f: &TypedFunction) -> Option<u32> {
    match f.callable_return_contract.as_ref()?.source {
        CallableReturnSource::Parameter(k) => Some(k),
        CallableReturnSource::Fresh => None,
    }
}

pub(super) fn compatible(actual: CallableKind, requested: CallableKind) -> bool {
    matches!(
        (actual, requested),
        (
            CallableKind::Fn,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) | (
            CallableKind::FnMut,
            CallableKind::FnMut | CallableKind::FnOnce
        ) | (CallableKind::FnOnce, CallableKind::FnOnce)
    )
}

pub(super) fn envelope(
    module: &TypedModule,
    f: &TypedFunction,
    budget: &mut Budget,
) -> CompileResult<()> {
    let k = parameter(f).ok_or_else(|| bad("callable forwarding source is absent"))?;
    let c = f.callable_return_contract.as_ref().unwrap();
    let claim = f
        .callable_contract
        .as_ref()
        .ok_or_else(|| bad("parameter return needs its actual callable role and trace"))?;
    let local = *f
        .parameters
        .get(k as usize)
        .ok_or_else(|| bad("callable return parameter index is outside the actual signature"))?;
    let definition = module
        .closures
        .get(c.closure.0 as usize)
        .ok_or_else(|| bad("callable forwarding environment is absent"))?;
    budget.spend(
        f.parameters.len().saturating_add(claim.roles.len()),
        "callable forwarding signature envelope",
    )?;
    let external = c
        .capture_sources
        .as_ref()
        .is_some_and(|rows| !rows.is_empty())
        || definition.captures.iter().any(|ty| {
            matches!(
                ty,
                Type::Address { .. } | Type::Desc(_) | Type::BorrowedStorage { .. }
            ) || matches!(ty,Type::OwnedDyn{interface} if module.dyn_interfaces.iter().any(|d|d.id==*interface && d.context.is_some()))
        });
    if f.environment.is_some()
        || f.foreign.is_some()
        || f.result_mode != PassingMode::Infer
        || f.return_type != Type::Closure(c.closure)
        || f.locals.get(local.0 as usize).map(|l| &l.ty) != Some(&f.return_type)
        || claim.roles.len() != 1
        || claim.roles[0].parameter != k
        || claim.roles[0].closure != c.closure
        || claim.roles[0].kind != c.kind
        || claim.roles[0].cleanup != CallableCleanup::Returned
        || !compatible(definition.kind, c.kind)
        || c.hold != claim.roles[0].hold
        || c.call != claim.roles[0].call
        || c.writes != claim.roles[0].writes
        || c.lending_captures != definition.lending_captures
        || !definition.lending_captures.is_empty() && c.kind == CallableKind::FnOnce
    {
        return Err(bad(
            "parameter return must match one by-value Returned role and its declared bound",
        ));
    }
    for (index, p) in f.parameters.iter().enumerate() {
        if index != k as usize {
            super::owned_callbacks::data(module, &f.locals[p.0 as usize].ty, budget)?;
        }
    }
    // This checks the actual capture graph, including child environments and
    // Store captures, independently of the empty public Hold/Call claims.
    if external {
        super::retained::check_sources(module, f, budget)?;
        let mut expected = std::collections::BTreeSet::new();
        for row in c
            .capture_sources
            .as_ref()
            .ok_or_else(|| bad("forward source relation unknown"))?
        {
            let StorageBase::Capture(index) = row.position.base else {
                return Err(bad("forward leaf is not an actual capture"));
            };
            expected.insert(index);
        }
        let expected = expected.into_iter().collect::<Vec<_>>();
        if c.hold != expected || c.call != expected {
            return Err(bad(
                "forward Hold/Call differs from actual external capture leaves",
            ));
        }
    } else {
        super::callable_fields::self_contained(module, c.closure, budget)?;
    }
    budget.spend(
        module.closures.len().saturating_add(1),
        "forward capture graph tables",
    )?;
    let mut colors = vec![0u8; module.closures.len()];
    let mut pending = vec![(c.closure, false)];
    while let Some((id, leaving)) = pending.pop() {
        budget.spend(1, "forward capture graph visit")?;
        let color = colors
            .get_mut(id.0 as usize)
            .ok_or_else(|| bad("forward child environment is absent"))?;
        if leaving {
            *color = 2;
            continue;
        }
        if *color == 2 {
            continue;
        }
        if *color == 1 {
            return Err(bad("forward environment contains an owning capture cycle"));
        }
        *color = 1;
        budget.lookup(
            budget.forward_closures.len(),
            "forward concrete environment inventory",
        )?;
        budget.forward_closures.insert(id);
        let def = &module.closures[id.0 as usize];
        let body = module
            .functions
            .get(def.function.0 as usize)
            .ok_or_else(|| bad("forward child entry is absent"))?;
        budget.type_work(&def.result, false, "forward actual child result")?;
        budget.spend(
            def.parameters
                .len()
                .saturating_add(def.captures.len())
                .saturating_add(1),
            "forward actual child signature and edges",
        )?;
        if def.id != id
            || body.environment != Some(id)
            || body.foreign.is_some()
            || def.kind == CallableKind::FunctionPointer
            || body.return_type != def.result
            || body.parameters.len() != def.parameters.len()
        {
            return Err(bad("forward environment differs from its actual entry"));
        }
        for (local, ty) in body.parameters.iter().zip(&def.parameters) {
            budget.type_work(ty, false, "forward actual child parameter")?;
            if body.locals.get(local.0 as usize).map(|l| &l.ty) != Some(ty) {
                return Err(bad("forward environment actual parameter type differs"));
            }
        }
        budget.spend(body.blocks.len(), "forward actual capture access blocks")?;
        for block in &body.blocks {
            budget.spend(
                block.instructions.len(),
                "forward actual capture access instructions",
            )?;
            for instruction in &block.instructions {
                if let InstructionKind::WriteCapture { index, value } = instruction.kind {
                    let ty = def
                        .captures
                        .get(index as usize)
                        .ok_or_else(|| bad("forward capture write index is absent"))?;
                    budget.type_work(ty, true, "forward actual capture write type")?;
                    let owning_sum = budget.ordinary_store_sum_capture(id, ty)?;
                    if owning_sum && !resources::sums::admit(module, ty, budget)? {
                        return Err(bad("forward owned Sum write lacks an ordinary Store leaf"));
                    }
                    if def.kind == CallableKind::Fn
                        || (!ty.is_copy_value() && !(owning_sum && def.kind == CallableKind::FnMut))
                        || body.values.get(value.0 as usize) != Some(ty)
                        || instruction.result.is_some()
                    {
                        return Err(bad(
                            "forward environment capture write violates its actual entry mode",
                        ));
                    }
                }
            }
        }
        pending.push((id, true));
        for ty in &def.captures {
            if let Type::Closure(child) = ty {
                pending.push((*child, false));
            }
        }
    }
    budget.spend(
        definition.captures.len(),
        "parameter return remaining capture cleanup",
    )?;
    let mut cleanup = Vec::new();
    for (index, ty) in definition.captures.iter().enumerate() {
        if matches!(
            ty,
            Type::Managed(_) | Type::Package { .. } | Type::Closure(_) | Type::OwnedDyn { .. }
        ) || matches!(ty,Type::BorrowedStorage{storage,..} if matches!(storage.as_ref(),Type::Package{..}))
        {
            budget.spend(1, "parameter return cleanup index")?;
            cleanup.push(index as u32);
        } else if budget.ordinary_store_sum_capture(definition.id, ty)? {
            if !resources::sums::admit(module, ty, budget)? {
                return Err(bad(
                    "parameter return Sum capture lacks an ordinary Store leaf",
                ));
            }
            budget.spend(1, "parameter return Sum cleanup index")?;
            cleanup.push(index as u32);
        }
    }
    budget.spend(c.cleanup.len(), "parameter return cleanup comparison")?;
    if c.cleanup != cleanup {
        return Err(bad(
            "parameter return cleanup differs from its actual environment",
        ));
    }
    budget.lookup(budget.parameter_returns.len(), "parameter return inventory")?;
    budget.parameter_returns.insert(f.id);
    Ok(())
}

const LIVE: u8 = 1;
const VACANT: u8 = 2;
const MOVED: u8 = 4;
const CLEANED: u8 = 8;
const FRESH: Origin = Origin::FACTORY;
const UNKNOWN: Origin = Origin::UNKNOWN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    origin: Origin,
    life: u8,
    // A Load used to invoke an actual Copy environment is an observation of
    // the symbolic F, not permission to transfer/copy that formal value.
    alias: bool,
}
impl Slot {
    const EMPTY: Self = Self {
        origin: Origin::EMPTY,
        life: VACANT,
        alias: false,
    };
    fn fresh() -> Self {
        Self {
            origin: FRESH,
            life: LIVE,
            alias: false,
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct State {
    locals: Vec<Slot>,
    values: Vec<Slot>,
}
impl State {
    fn copied(&self, budget: &mut Budget) -> CompileResult<Self> {
        budget.spend(
            self.locals
                .len()
                .saturating_add(self.values.len())
                .saturating_mul(std::mem::size_of::<Slot>()),
            "forward CFG state copy",
        )?;
        Ok(self.clone())
    }
    fn join(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(
            self.locals.len().saturating_add(self.values.len()),
            "forward CFG state join",
        )?;
        let mut changed = false;
        for (old, new) in self
            .locals
            .iter_mut()
            .chain(&mut self.values)
            .zip(other.locals.iter().chain(&other.values))
        {
            let found = Slot {
                origin: old.origin | new.origin,
                life: old.life | new.life,
                alias: old.alias || new.alias,
            };
            changed |= found != *old;
            *old = found;
        }
        Ok(changed)
    }
}

pub(super) struct Snapshot {
    pub(super) locals: Vec<Origin>,
    pub(super) values: Vec<Origin>,
}
pub(super) type Snapshots = Vec<Vec<Option<Snapshot>>>;

fn closure(ty: &Type) -> Option<(ClosureId, bool)> {
    match ty {
        Type::Closure(id) => Some((*id, false)),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => Some((*id, true)),
            _ => None,
        },
        _ => None,
    }
}
/// Existing lowering stores a replaceable owning F in one real array slot.
/// This is a local carrier, never an additional public parameter/result form.
pub(super) fn internal_slot(ty: &Type) -> Option<ClosureId> {
    match ty {
        Type::Array { element, length: 1 } => match element.as_ref() {
            Type::Closure(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}
fn carrier(ty: &Type) -> bool {
    closure(ty).is_some() || internal_slot(ty).is_some()
}
fn role(origin: Origin) -> CompileResult<Option<usize>> {
    if let Some(index) = origin.index() {
        return Ok(Some(index));
    }
    if origin == FRESH {
        return Ok(None);
    }
    Err(bad(
        "callable instance source is missing or mixed at this CFG position",
    ))
}

fn live(slot: Slot, check: bool) -> CompileResult<()> {
    if check && slot.life != LIVE {
        Err(bad(
            "callable instance is moved, absent or cleaned on a reachable path",
        ))
    } else {
        Ok(())
    }
}
fn mode(
    module: &TypedModule,
    claim: &CallableParameterContract,
    ty: &Type,
    slot: Slot,
) -> CompileResult<CallableKind> {
    if let Some(index) = role(slot.origin)? {
        Ok(claim
            .roles
            .get(index)
            .ok_or_else(|| bad("callable instance role is absent"))?
            .kind)
    } else {
        let (id, _) = closure(ty).ok_or_else(|| bad("callable instance type is absent"))?;
        Ok(module.closures[id.0 as usize].kind)
    }
}
fn concrete_copy(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let Some((id, borrowed)) = closure(ty) else {
        return Ok(false);
    };
    if borrowed {
        return Ok(true);
    }
    let definition = &module.closures[id.0 as usize];
    budget.spend(
        definition.captures.len().saturating_add(1),
        "forward concrete Copy environment",
    )?;
    if definition.kind == CallableKind::FnOnce {
        return Ok(false);
    }
    for capture in &definition.captures {
        budget.type_work(capture, true, "forward concrete Copy capture")?;
        if !capture.is_copy_value() {
            return Ok(false);
        }
    }
    Ok(true)
}

// Legacy Fn/Owned helpers may observe an actual Copy environment through an
// SSA snapshot, including a local alias. This is not a transferable F value:
// consume still rejects it, and Parameter/Returned identity proofs never use
// this lane. Declared FnOnce alone never acquires Copy from its instantiation.
fn copy_observation(
    module: &TypedModule,
    f: &TypedFunction,
    claim: &CallableParameterContract,
    ty: &Type,
    slot: Slot,
    budget: &mut Budget,
) -> CompileResult<bool> {
    if !slot.alias || f.callable_return_contract.is_some() || !matches!(ty, Type::Closure(_)) {
        return Ok(false);
    }
    let Some(index) = role(slot.origin)? else {
        return Ok(false);
    };
    if !claim
        .roles
        .get(index)
        .is_some_and(|r| r.kind == CallableKind::Fn && r.cleanup == CallableCleanup::Owned)
    {
        return Ok(false);
    }
    concrete_copy(module, ty, budget)
}

fn consume(
    module: &TypedModule,
    f: &TypedFunction,
    state: &mut State,
    value: ValueId,
    check: bool,
    budget: &mut Budget,
) -> CompileResult<Slot> {
    let slot = state.values[value.0 as usize];
    live(slot, check)?;
    let ty = &f.values[value.0 as usize];
    if check && slot.alias && (matches!(ty, Type::Closure(_)) || internal_slot(ty).is_some()) {
        return Err(bad(
            "a symbolic F observation cannot be copied into a transferred value",
        ));
    }
    let symbolic = slot.origin.has_role();
    if !slot.alias && (symbolic || !concrete_copy(module, ty, budget)?) {
        state.values[value.0 as usize].life = MOVED;
    }
    Ok(slot)
}

fn call_arguments(
    module: &TypedModule,
    f: &TypedFunction,
    state: &State,
    instruction: &Instruction,
    ty: &Type,
    arguments: &[ValueId],
    budget: &mut Budget,
) -> CompileResult<()> {
    let (id, _) = closure(ty)
        .ok_or_else(|| bad("forward callback receiver has no concrete callable signature"))?;
    let definition = &module.closures[id.0 as usize];
    budget.spend(arguments.len(), "forward ordinary callback arguments")?;
    budget.type_work(&definition.result, false, "forward actual callback result")?;
    if arguments.len() != definition.parameters.len()
        || !(instruction.result.and_then(|v| f.values.get(v.0 as usize))
            == Some(&definition.result)
            || (instruction.result.is_none() && definition.result == Type::Unit))
    {
        return Err(bad(
            "forward callback actual argument count or result type differs",
        ));
    }
    for (value, expected) in arguments.iter().zip(&definition.parameters) {
        budget.type_work(expected, false, "forward actual callback argument type")?;
        if carrier(&f.values[value.0 as usize]) && state.values[value.0 as usize].origin.has_role()
        {
            return Err(bad(
                "symbolic F cannot enter an ordinary callback data argument",
            ));
        }
        if f.values.get(value.0 as usize) != Some(expected) {
            return Err(bad(
                "forward callback actual ordinary argument type differs",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    claim: &CallableParameterContract,
    block: usize,
    state: &mut State,
    checked: &[bool],
    check: bool,
    mut snapshots: Option<&mut [Option<Snapshot>]>,
    budget: &mut Budget,
) -> CompileResult<()> {
    for (position, i) in f.blocks[block].instructions.iter().enumerate() {
        budget.spend(1, "forward actual instruction")?;
        if i.result.is_some_and(|v| v.0 as usize >= f.values.len()) {
            return Err(bad("forward instruction result is outside the value table"));
        }
        for value in super::resources::uses(&i.kind) {
            budget.spend(1, "forward actual operand bounds")?;
            if value.0 as usize >= f.values.len() {
                return Err(bad(
                    "forward instruction operand is outside the value table",
                ));
            }
        }
        let call = matches!(
            i.kind,
            InstructionKind::MakeClosure { .. }
                | InstructionKind::Call { .. }
                | InstructionKind::WithForeignCallback { .. }
                | InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureOnce { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureBorrowed { .. }
        );
        if call && let Some(rows) = snapshots.as_deref_mut() {
            budget.spend(
                Origin::table_work(state.locals.len().saturating_add(state.values.len())),
                "forward call provenance snapshot",
            )?;
            rows[position] = Some(Snapshot {
                locals: state.locals.iter().map(|s| s.origin).collect(),
                values: state.values.iter().map(|s| s.origin).collect(),
            });
        }
        let mut output = None;
        match &i.kind {
            InstructionKind::Load(local)
            | InstructionKind::TakeLocal { local }
            | InstructionKind::BorrowLocal { local }
            | InstructionKind::BorrowLocalMut { local }
                if carrier(&f.locals[local.0 as usize].ty) =>
            {
                let slot = state.locals[local.0 as usize];
                live(slot, check)?;
                if check {
                    let declared = &f.locals[local.0 as usize].ty;
                    budget.type_work(declared, false, "forward position operation exact type")?;
                    let output = i.result.and_then(|v| f.values.get(v.0 as usize));
                    let valid = match i.kind {
                        InstructionKind::Load(_) | InstructionKind::TakeLocal { .. } => {
                            output == Some(declared)
                        }
                        InstructionKind::BorrowLocal { .. }
                        | InstructionKind::BorrowLocalMut { .. } => {
                            let write = matches!(i.kind, InstructionKind::BorrowLocalMut { .. });
                            (!write || f.locals[local.0 as usize].mutable)
                                && matches!(output, Some(Type::Address { mutable, inner }) if *mutable == write && inner.as_ref() == declared)
                        }
                        _ => false,
                    };
                    if !valid {
                        return Err(bad(
                            "forward position operation changes type or invents write permission",
                        ));
                    }
                }
                let mut taken = slot;
                if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                    state.locals[local.0 as usize].life = MOVED;
                } else if matches!(
                    i.kind,
                    InstructionKind::BorrowLocal { .. } | InstructionKind::BorrowLocalMut { .. }
                ) || (slot.origin.has_role() && !slot.alias)
                {
                    taken.alias = true;
                } else if check && !concrete_copy(module, &f.locals[local.0 as usize].ty, budget)? {
                    return Err(bad("Load duplicates a non-Copy concrete environment"));
                }
                output = Some(taken);
            }
            InstructionKind::ReborrowAddress { address, .. }
                if i.result
                    .is_some_and(|v| closure(&f.values[v.0 as usize]).is_some()) =>
            {
                let mut slot = state.values[address.0 as usize];
                live(slot, check)?;
                slot.alias = true;
                output = Some(slot);
            }
            InstructionKind::Store { local, value } if carrier(&f.locals[local.0 as usize].ty) => {
                let observation = state.values[value.0 as usize];
                let slot = if matches!(
                    (&f.locals[local.0 as usize].ty, &f.values[value.0 as usize]),
                    (Type::Closure(a), Type::Closure(b)) if a == b
                ) && copy_observation(
                    module,
                    f,
                    claim,
                    &f.values[value.0 as usize],
                    observation,
                    budget,
                )? {
                    live(observation, check)?;
                    observation
                } else if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                    || internal_slot(&f.locals[local.0 as usize].ty).is_some()
                {
                    consume(module, f, state, *value, check, budget)?
                } else {
                    let slot = state.values[value.0 as usize];
                    live(slot, check)?;
                    slot
                };
                state.locals[local.0 as usize] = slot;
            }
            InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                state.locals[local.0 as usize] = Slot::EMPTY;
            }
            InstructionKind::MakeOwnedArray { elements }
                if i.result
                    .is_some_and(|v| internal_slot(&f.values[v.0 as usize]).is_some()) =>
            {
                if elements.len() != 1 {
                    return Err(bad("forward internal slot must have exactly one element"));
                }
                output = Some(consume(module, f, state, elements[0], check, budget)?);
            }
            InstructionKind::TakeOwnedArrayElement { local, index }
            | InstructionKind::BorrowOwnedArrayElement { local, index, .. }
            | InstructionKind::StoreOwnedArrayElement { local, index, .. }
            | InstructionKind::DropOwnedArrayElement { local, index }
                if internal_slot(&f.locals[local.0 as usize].ty).is_some() =>
            {
                if *index != 0 {
                    return Err(bad(
                        "forward internal slot index is outside its actual element",
                    ));
                }
                match &i.kind {
                    InstructionKind::StoreOwnedArrayElement { value, .. } => {
                        state.locals[local.0 as usize] =
                            consume(module, f, state, *value, check, budget)?;
                    }
                    InstructionKind::TakeOwnedArrayElement { .. } => {
                        let slot = state.locals[local.0 as usize];
                        live(slot, check)?;
                        state.locals[local.0 as usize].life = MOVED;
                        output = Some(slot);
                    }
                    InstructionKind::BorrowOwnedArrayElement { .. } => {
                        let mut slot = state.locals[local.0 as usize];
                        live(slot, check)?;
                        slot.alias = true;
                        output = Some(slot);
                    }
                    InstructionKind::DropOwnedArrayElement { .. } => {
                        let slot = &mut state.locals[local.0 as usize];
                        if slot.life & LIVE != 0 {
                            slot.life = (slot.life & !LIVE) | CLEANED;
                        }
                    }
                    _ => unreachable!(),
                }
            }
            InstructionKind::DropClosure { local } | InstructionKind::DropOwnedArray { local }
                if carrier(&f.locals[local.0 as usize].ty) =>
            {
                let slot = &mut state.locals[local.0 as usize];
                if check && slot.alias {
                    return Err(bad("a borrowed callable observation cannot be destroyed"));
                }
                if check && slot.life == CLEANED {
                    return Err(bad("callable instance is cleaned twice"));
                }
                if slot.life & LIVE != 0 {
                    slot.life = (slot.life & !LIVE) | CLEANED;
                }
            }
            InstructionKind::MakeClosure { captures, .. } => {
                for value in captures {
                    if carrier(&f.values[value.0 as usize]) {
                        let slot = consume(module, f, state, *value, check, budget)?;
                        if check && slot.origin.has_role() {
                            return Err(bad(
                                "forwarded symbolic F cannot be hidden in a new local environment",
                            ));
                        }
                    }
                }
                output = Some(Slot::fresh());
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                let callee = &module.functions[function.0 as usize];
                let returned = parameter(callee);
                if returned.is_some() && check && !checked[function.0 as usize] {
                    return Err(bad(
                        "nested parameter return was not independently checked first",
                    ));
                }
                let mut result = None;
                for (index, value) in arguments.iter().enumerate() {
                    if !carrier(&f.values[value.0 as usize]) {
                        continue;
                    }
                    let slot = state.values[value.0 as usize];
                    live(slot, check)?;
                    if let Some(child) = &callee.callable_contract {
                        for requested in child.roles.iter().filter(|r| r.parameter == index as u32)
                        {
                            if check
                                && !compatible(
                                    mode(module, claim, &f.values[value.0 as usize], slot)?,
                                    requested.kind,
                                )
                            {
                                return Err(bad(
                                    "nested helper upgrades the actual argument's declared mode",
                                ));
                            }
                        }
                    }
                    let slot = if matches!(f.values[value.0 as usize], Type::Closure(_))
                        || internal_slot(&f.values[value.0 as usize]).is_some()
                    {
                        consume(module, f, state, *value, check, budget)?
                    } else {
                        slot
                    };
                    if returned == Some(index as u32) {
                        result = Some(slot);
                    }
                }
                if returned.is_some() {
                    let mut slot = result.ok_or_else(|| {
                        bad("nested return source has no actual callable argument")
                    })?;
                    slot.life = LIVE;
                    output = Some(slot);
                } else if i
                    .result
                    .is_some_and(|v| closure(&f.values[v.0 as usize]).is_some())
                {
                    output = Some(
                        if callee
                            .callable_return_contract
                            .as_ref()
                            .is_some_and(|c| c.source == CallableReturnSource::Fresh)
                        {
                            Slot::fresh()
                        } else {
                            Slot {
                                origin: UNKNOWN,
                                life: LIVE,
                                alias: false,
                            }
                        },
                    );
                }
            }
            InstructionKind::WithForeignCallback { callback, body, .. } => {
                let callback_slot = state.values[callback.0 as usize];
                let body_slot = state.values[body.0 as usize];
                live(callback_slot, check)?;
                live(body_slot, check)?;
                if check
                    && (!matches!(&f.values[callback.0 as usize], Type::Address { mutable: true, inner } if matches!(inner.as_ref(), Type::Closure(_)))
                        || !matches!(f.values[body.0 as usize], Type::Closure(_))
                        || !compatible(
                            mode(module, claim, &f.values[callback.0 as usize], callback_slot)?,
                            CallableKind::FnMut,
                        ))
                    || !compatible(
                        mode(module, claim, &f.values[body.0 as usize], body_slot)?,
                        CallableKind::FnOnce,
                    )
                {
                    return Err(bad(
                        "scoped callback upgrades its actual callable mode or address",
                    ));
                }
                consume(module, f, state, *body, check, budget)?;
            }
            InstructionKind::CallClosureShared { callee, arguments }
            | InstructionKind::CallClosureMut { callee, arguments } => {
                let slot = state.locals[callee.0 as usize];
                live(slot, check)?;
                if check {
                    if !matches!(f.locals[callee.0 as usize].ty, Type::Closure(_)) {
                        return Err(bad(
                            "forward local callback call requires an actual environment position",
                        ));
                    }
                    call_arguments(
                        module,
                        f,
                        state,
                        i,
                        &f.locals[callee.0 as usize].ty,
                        arguments,
                        budget,
                    )?;
                }
                let wanted = if matches!(i.kind, InstructionKind::CallClosureMut { .. }) {
                    CallableKind::FnMut
                } else {
                    CallableKind::Fn
                };
                if check
                    && (!compatible(
                        mode(module, claim, &f.locals[callee.0 as usize].ty, slot)?,
                        wanted,
                    ) || (wanted == CallableKind::FnMut
                        && !f.locals[callee.0 as usize].mutable))
                {
                    return Err(bad(
                        "call through a local upgrades the declared mode or lacks write permission",
                    ));
                }
            }
            InstructionKind::CallClosure { callee, arguments }
            | InstructionKind::CallClosureOnce { callee, arguments }
            | InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => {
                let slot = state.values[callee.0 as usize];
                live(slot, check)?;
                if check {
                    call_arguments(
                        module,
                        f,
                        state,
                        i,
                        &f.values[callee.0 as usize],
                        arguments,
                        budget,
                    )?;
                    if matches!(i.kind, InstructionKind::CallClosure { .. })
                        && slot.origin.has_role()
                        && !copy_observation(
                            module,
                            f,
                            claim,
                            &f.values[callee.0 as usize],
                            slot,
                            budget,
                        )?
                    {
                        return Err(bad(
                            "by-value symbolic Fn call would require a Copy permission",
                        ));
                    }
                }
                let wanted = match i.kind {
                    InstructionKind::CallClosureOnce { .. } => CallableKind::FnOnce,
                    InstructionKind::CallClosureBorrowed { kind, .. } => kind,
                    _ => CallableKind::Fn,
                };
                if check {
                    match &i.kind {
                        InstructionKind::CallClosureBorrowed { kind, .. } => {
                            if !matches!(&f.values[callee.0 as usize], Type::Address { mutable, inner }
                                if matches!(inner.as_ref(), Type::Closure(_))
                                    && *kind != CallableKind::FnOnce
                                    && (*kind != CallableKind::FnMut || *mutable))
                            {
                                return Err(bad(
                                    "forward borrowed call lacks its exact address and permission",
                                ));
                            }
                        }
                        _ if !matches!(f.values[callee.0 as usize], Type::Closure(_)) => {
                            return Err(bad(
                                "forward value call requires a concrete environment value",
                            ));
                        }
                        _ => {}
                    }
                }
                if check
                    && !compatible(
                        mode(module, claim, &f.values[callee.0 as usize], slot)?,
                        wanted,
                    )
                {
                    return Err(bad(
                        "call through an SSA value upgrades its original declared mode",
                    ));
                }
                if wanted == CallableKind::FnOnce {
                    consume(module, f, state, *callee, check, budget)?;
                }
            }
            _ => {
                // A concrete instantiation being Copy does not authorize the
                // symbolic F to escape into an aggregate, field, conversion or
                // thin value. Only the explicit position/call rules above can
                // transfer or observe a supplied callable role.
                for value in super::resources::uses(&i.kind) {
                    budget.spend(1, "forward unhandled callable operand audit")?;
                    if carrier(&f.values[value.0 as usize])
                        && state.values[value.0 as usize].origin.has_role()
                    {
                        return Err(bad("symbolic F enters an unsupported value operation"));
                    }
                }
            }
        }
        if let Some(value) = i.result
            && carrier(&f.values[value.0 as usize])
        {
            state.values[value.0 as usize] = output.unwrap_or(Slot {
                origin: UNKNOWN,
                life: LIVE,
                alias: false,
            });
        }
    }
    if check
        && let Some(k) = parameter(f)
        && let Terminator::Return(value) = f.blocks[block].terminator
    {
        let value = value.ok_or_else(|| bad("callable parameter return has no value"))?;
        budget.spend(1, "forward normal return exact type")?;
        if f.values.get(value.0 as usize) != Some(&f.return_type) {
            return Err(bad("forward return does not have the actual bare F type"));
        }
        let slot = state.values[value.0 as usize];
        live(slot, true)?;
        let index = claim
            .roles
            .iter()
            .position(|r| r.parameter == k)
            .ok_or_else(|| bad("returned parameter has no actual role"))?;
        if slot.origin.index() != Some(index) || slot.alias {
            return Err(bad(
                "normal return is not the same live prepared callable parameter instance",
            ));
        }
    }
    Ok(())
}

pub(super) fn prove(
    module: &TypedModule,
    f: &TypedFunction,
    claim: &CallableParameterContract,
    parameters: &[(u32, LocalId)],
    checked: &[bool],
    budget: &mut Budget,
) -> CompileResult<Snapshots> {
    let cells = f.locals.len().saturating_add(f.values.len());
    budget.spend(
        cells.saturating_mul(std::mem::size_of::<Slot>()),
        "forward initial CFG state",
    )?;
    let mut entry = State {
        locals: vec![Slot::EMPTY; f.locals.len()],
        values: vec![Slot::EMPTY; f.values.len()],
    };
    budget.spend(parameters.len(), "forward incoming role instances")?;
    for (index, (_, local)) in parameters.iter().enumerate() {
        entry.locals[local.0 as usize] = Slot {
            origin: Origin::supplied(
                index,
                matches!(f.locals[local.0 as usize].ty, Type::Address { .. }),
            ),
            life: LIVE,
            alias: matches!(f.locals[local.0 as usize].ty, Type::Address { .. }),
        };
    }
    budget.product(f.blocks.len(), 3, 1, "forward CFG frontier")?;
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(entry);
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    let mut queue = VecDeque::from([f.entry.0 as usize]);
    while let Some(block) = queue.pop_front() {
        budget.spend(1, "forward CFG frontier pop")?;
        queued[block] = false;
        let mut state = inputs[block].as_ref().unwrap().copied(budget)?;
        transfer(
            module, f, claim, block, &mut state, checked, false, None, budget,
        )?;
        let successors = match f.blocks[block].terminator {
            Terminator::Jump(target) => [Some(target), None],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => [Some(then_block), Some(else_block)],
            _ => [None, None],
        };
        for target in successors.into_iter().flatten() {
            budget.spend(1, "forward CFG edge")?;
            let target = target.0 as usize;
            let changed = if let Some(input) = &mut inputs[target] {
                input.join(&state, budget)?
            } else {
                inputs[target] = Some(state.copied(budget)?);
                true
            };
            if changed && !queued[target] {
                budget.spend(1, "forward CFG queue growth")?;
                queued[target] = true;
                queue.push_back(target);
            }
        }
    }
    budget.spend(f.blocks.len(), "forward call snapshot blocks")?;
    let mut snapshots = Vec::new();
    for (block, input) in inputs.into_iter().enumerate() {
        budget.spend(
            f.blocks[block].instructions.len(),
            "forward call snapshot slots",
        )?;
        let mut rows: Vec<_> = (0..f.blocks[block].instructions.len())
            .map(|_| None)
            .collect();
        if let Some(mut state) = input {
            transfer(
                module,
                f,
                claim,
                block,
                &mut state,
                checked,
                true,
                Some(&mut rows),
                budget,
            )?;
        }
        snapshots.push(rows);
    }
    Ok(snapshots)
}

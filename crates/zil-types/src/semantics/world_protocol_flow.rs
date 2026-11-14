//! Actual protocol roots and Poll generations are reconstructed over the CFG.
//! Original tables supply conditional obligations only after the head bridge;
//! neither equal lifetime images nor a proposed output phase creates a root.
use super::*;
use std::collections::BTreeSet;
const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 4;
const UNKNOWN: u8 = 7;
fn bit(state: ProtocolState) -> u8 {
    match state {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
fn mask(states: &[ProtocolState]) -> u8 {
    states.iter().fold(0, |m, s| m | bit(*s))
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Parameter(u32),
    Created(ValueId),
}
#[derive(Clone, PartialEq, Eq)]
struct Outcome {
    tag: i64,
    phases: BTreeMap<Root, u8>,
}
#[derive(Clone, PartialEq, Eq)]
enum Value {
    Plain,
    Integer(i64),
    Root(Root, bool),
    Poll {
        fresh: bool,
        outcomes: Vec<Outcome>,
    },
    Tag {
        fresh: bool,
        outcomes: Vec<Outcome>,
    },
    Predicate {
        fresh: bool,
        yes: Vec<Outcome>,
        no: Vec<Outcome>,
    },
    Tuple(Vec<Value>),
}
impl Value {
    fn work(&self) -> usize {
        match self {
            Self::Poll { outcomes, .. } | Self::Tag { outcomes, .. } => outcomes
                .iter()
                .fold(1, |w, o| w.saturating_add(o.phases.len()).saturating_add(1)),
            Self::Predicate { yes, no, .. } => yes
                .iter()
                .chain(no)
                .fold(1, |w, o| w.saturating_add(o.phases.len()).saturating_add(1)),
            Self::Tuple(v) => v.iter().fold(1, |w, v| w.saturating_add(v.work())),
            _ => 1,
        }
    }
    fn invalidate(&mut self, root: Root, replace: bool) {
        match self {
            Self::Root(id, live) if *id == root && replace => *live = false,
            Self::Poll { fresh, outcomes } | Self::Tag { fresh, outcomes }
                if outcomes.iter().any(|o| o.phases.contains_key(&root)) =>
            {
                *fresh = false
            }
            Self::Predicate { fresh, yes, no }
                if yes
                    .iter()
                    .chain(no.iter())
                    .any(|o| o.phases.contains_key(&root)) =>
            {
                *fresh = false
            }
            Self::Tuple(values) => {
                for value in values {
                    value.invalidate(root, replace);
                }
            }
            _ => {}
        }
    }
    fn roots(&self, output: &mut BTreeSet<Root>) {
        match self {
            Self::Root(root, true) => {
                output.insert(*root);
            }
            Self::Tuple(v) => {
                for v in v {
                    v.roots(output);
                }
            }
            _ => {}
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct State {
    locals: Vec<Option<Value>>,
    values: Vec<Option<Value>>,
    phases: BTreeMap<Root, u8>,
}
impl State {
    fn work(&self) -> usize {
        self.locals.iter().chain(&self.values).flatten().fold(
            self.locals
                .len()
                .saturating_add(self.values.len())
                .saturating_add(self.phases.len())
                .saturating_add(1),
            |w, v| w.saturating_add(v.work()),
        )
    }
    fn copy(&self, world: &mut World<'_, '_>) -> R<Self> {
        world.pay(self.work())?;
        Ok(self.clone())
    }
    fn get(&self, id: ValueId, world: &mut World<'_, '_>) -> R<Value> {
        let value = self
            .values
            .get(id.0 as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| {
                bad("actual protocol operand has no initialized dominating SSA occurrence")
            })?;
        world.pay(value.work())?;
        Ok(value.clone())
    }
    fn local(&self, id: LocalId, world: &mut World<'_, '_>) -> R<Value> {
        let value = self
            .locals
            .get(id.0 as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| bad("actual protocol local is uninitialized"))?;
        world.pay(value.work())?;
        Ok(value.clone())
    }
    fn change(&mut self, world: &mut World<'_, '_>, root: Root, phase: u8, replace: bool) -> R<()> {
        world.pay(self.work())?;
        for value in self.locals.iter_mut().chain(&mut self.values).flatten() {
            value.invalidate(root, replace);
        }
        self.phases.insert(root, phase);
        Ok(())
    }
    fn root(&self, value: &Value) -> R<Root> {
        if let Value::Root(root, true) = value {
            if self.phases.contains_key(root) {
                return Ok(*root);
            }
        }
        Err(bad(
            "actual protocol opcode lacks its live owning root or parameter boundary",
        ))
    }
}
fn async_head(world: &mut World<'_, '_>, mut ty: &Type) -> R<bool> {
    loop {
        world.pay(1)?;
        match ty {
            Type::Managed(t) | Type::Desc(t) | Type::Address { inner: t, .. } => ty = t,
            Type::AsyncRegistration { .. } => return Ok(true),
            _ => return Ok(false),
        }
    }
}
fn operand_values(
    world: &mut World<'_, '_>,
    state: &State,
    arguments: &[ValueId],
) -> R<Vec<Value>> {
    world.pay(arguments.len())?;
    arguments.iter().map(|id| state.get(*id, world)).collect()
}
fn unknown(
    world: &mut World<'_, '_>,
    state: &mut State,
    arguments: &[Value],
    dynamic: bool,
) -> R<()> {
    let mut roots = BTreeSet::new();
    for value in arguments {
        world.pay(value.work())?;
        value.roots(&mut roots);
    }
    if dynamic {
        world.pay(state.phases.len())?;
        roots.extend(state.phases.keys().copied());
    }
    for root in roots {
        state.change(world, root, UNKNOWN, false)?;
    }
    Ok(())
}
fn call(
    world: &mut World<'_, '_>,
    state: &mut State,
    function: FunctionId,
    arguments: &[ValueId],
) -> R<Value> {
    let values = operand_values(world, state, arguments)?;
    let callee = world.function(function)?;
    world.pay(world.program.instances.len())?;
    let mut instances=world.program.instances.iter().filter(|i|matches!(i.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function));
    let instance = instances
        .next()
        .ok_or_else(|| bad("actual protocol call has no true original callee instance"))?;
    if instances.next().is_some() {
        return Err(bad(
            "actual protocol call has multiple original callee instances",
        ));
    }
    let original = world.definition(instance.definition)?;
    super::protocol::header(world, original, callee)?;
    if callee.protocol_contracts.is_empty() {
        unknown(world, state, &values, false)?;
        return Ok(Value::Plain);
    }
    if arguments.len() != callee.parameters.len() {
        return Err(bad("protocol call changes actual parameter positions"));
    }
    world.pay(
        callee
            .protocol_contracts
            .len()
            .saturating_add(arguments.len()),
    )?;
    let mut roles = Vec::new();
    let mut seen = BTreeSet::new();
    for contract in &callee.protocol_contracts {
        crate::registration::charge_contract(contract, |n| world.pay(n))?;
        let value = values
            .get(contract.parameter as usize)
            .ok_or_else(|| bad("protocol call is missing a declared parameter"))?;
        let root = state.root(value)?;
        if !seen.insert(root) {
            return Err(bad(
                "actual protocol call aliases independent exclusive roles",
            ));
        }
        let phase = state.phases[&root];
        if phase == 0 || phase & !mask(&contract.entry) != 0 {
            return Err(bad(
                "actual protocol caller has not established this argument root's entry state",
            ));
        }
        roles.push((root, phase, contract));
    }
    let mut outcomes = Vec::new();
    for (exit, tag) in [
        (ProtocolExit::Return, -1),
        (ProtocolExit::PollPending, 0),
        (ProtocolExit::PollReady, 1),
    ] {
        let mut phases = BTreeMap::new();
        for (root, phase, contract) in &roles {
            world.pay(contract.exits.len().saturating_mul(4))?;
            let mut output = 0;
            for row in &contract.exits {
                if row.exit == exit && bit(row.input) & *phase != 0 {
                    output |= mask(&row.output);
                }
            }
            if output == 0 {
                phases.clear();
                break;
            }
            phases.insert(*root, output);
        }
        if !phases.is_empty() {
            outcomes.push(Outcome { tag, phases });
        }
    }
    if callee.protocol_contracts.iter().any(|c| {
        c.exits
            .iter()
            .any(|r| matches!(r.exit, ProtocolExit::ErrorOk | ProtocolExit::ErrorErr))
    }) {
        return Err(pending(
            "actual protocol ErrorUnion outputs require complete tagged owner relations",
        ));
    }
    if outcomes.is_empty() {
        return Err(bad("actual protocol call has no proved reachable result"));
    }
    for (root, _, _) in roles {
        let phase = outcomes
            .iter()
            .fold(0, |m, o| m | o.phases.get(&root).copied().unwrap_or(0));
        state.change(world, root, phase, false)?;
    }
    if outcomes.iter().all(|o| o.tag == -1) {
        Ok(Value::Plain)
    } else {
        Ok(Value::Poll {
            fresh: true,
            outcomes,
        })
    }
}
fn instruction(
    world: &mut World<'_, '_>,
    state: &mut State,
    f: &TypedFunction,
    instruction: &Instruction,
) -> R<()> {
    world.pay(1)?;
    let value = match &instruction.kind {
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::DescribeLocal { local } => state.local(*local, world)?,
        InstructionKind::TakeLocal { local } => {
            let value = state.local(*local, world)?;
            state.locals[local.0 as usize] = None;
            value
        }
        InstructionKind::Store { local, value } => {
            let value = state.get(*value, world)?;
            *state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| bad("protocol Store has no local"))? = Some(value);
            Value::Plain
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            *state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| bad("protocol activation has no local"))? = None;
            Value::Plain
        }
        InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::DescribeAddress { address }
        | InstructionKind::ReadAddress { address } => state.get(*address, world)?,
        InstructionKind::WriteAddress { address, value } => {
            let target = state.get(*address, world)?;
            let value = state.get(*value, world)?;
            let mut roots = BTreeSet::new();
            world.pay(target.work().saturating_add(value.work()))?;
            target.roots(&mut roots);
            value.roots(&mut roots);
            for root in roots {
                state.change(world, root, UNKNOWN, false)?;
            }
            Value::Plain
        }
        InstructionKind::UnpackTuple { local, fields } => {
            let value = state.local(*local, world)?;
            let values = match value {
                Value::Tuple(values) => values,
                _ => {
                    world.pay(fields.len())?;
                    vec![Value::Plain; fields.len()]
                }
            };
            if fields.len() != values.len() {
                return Err(bad(
                    "actual protocol tuple destructuring changes field arity",
                ));
            }
            world.pay(fields.len())?;
            for (local, value) in fields.iter().zip(values) {
                *state
                    .locals
                    .get_mut(local.0 as usize)
                    .ok_or_else(|| bad("protocol tuple destination is absent"))? = Some(value);
            }
            *state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| bad("protocol cleanup local is absent"))? = None;
            Value::Plain
        }
        InstructionKind::UnpackVariant { local, fields, .. } => {
            let value = state.local(*local, world)?;
            let mut roots = BTreeSet::new();
            world.pay(value.work().saturating_add(fields.len()))?;
            value.roots(&mut roots);
            if !roots.is_empty() {
                return Err(pending(
                    "actual owning protocol enum payload needs its complete root projection",
                ));
            }
            for local in fields {
                *state
                    .locals
                    .get_mut(local.0 as usize)
                    .ok_or_else(|| bad("protocol variant destination is absent"))? =
                    Some(Value::Plain);
            }
            state.locals[local.0 as usize] = None;
            Value::Plain
        }
        InstructionKind::AsyncRegistrationStart { .. } => {
            let id = instruction
                .result
                .ok_or_else(|| bad("actual registration constructor has no SSA occurrence"))?;
            let root = Root::Created(id);
            state.change(world, root, OPEN | CLOSED, true)?;
            Value::Root(root, true)
        }
        InstructionKind::AsyncRegistrationValid { registration } => {
            let value = state.get(*registration, world)?;
            let root = state.root(&value)?;
            world.pay(4)?;
            Value::Predicate {
                fresh: true,
                yes: vec![Outcome {
                    tag: 1,
                    phases: BTreeMap::new(),
                }],
                no: vec![Outcome {
                    tag: 0,
                    phases: BTreeMap::from([(root, CLOSED)]),
                }],
            }
        }
        InstructionKind::AsyncRegistrationRequestClose { registration }
        | InstructionKind::AsyncRegistrationPollClose { registration } => {
            let value = state.get(*registration, world)?;
            let root = state.root(&value)?;
            let phase = state.phases[&root];
            let closing = if phase & (OPEN | CLOSING) != 0 {
                CLOSING
            } else {
                0
            };
            if matches!(
                instruction.kind,
                InstructionKind::AsyncRegistrationRequestClose { .. }
            ) {
                state.change(world, root, closing | (phase & CLOSED), false)?;
                Value::Plain
            } else {
                state.change(world, root, closing | CLOSED, false)?;
                world.pay(4)?;
                let mut outcomes = Vec::new();
                if closing != 0 {
                    outcomes.push(Outcome {
                        tag: 0,
                        phases: BTreeMap::from([(root, CLOSING)]),
                    });
                }
                outcomes.push(Outcome {
                    tag: 1,
                    phases: BTreeMap::from([(root, CLOSED)]),
                });
                Value::Poll {
                    fresh: true,
                    outcomes,
                }
            }
        }
        InstructionKind::AsyncRegistrationFinishDestroy { registration }
        | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
            let value = state.get(*registration, world)?;
            let root = state.root(&value)?;
            if state.phases[&root] != CLOSED {
                return Err(bad(
                    "actual registration reclamation lacks a dominating Closed fact for this generation",
                ));
            }
            state.change(world, root, CLOSED, true)?;
            Value::Plain
        }
        InstructionKind::VariantTag { value } => match state.get(*value, world)? {
            Value::Poll { fresh, outcomes } => Value::Tag { fresh, outcomes },
            _ => Value::Plain,
        },
        InstructionKind::VariantTagLocal { local, path } if path.is_empty() => {
            match state.local(*local, world)? {
                Value::Poll { fresh, outcomes } => Value::Tag { fresh, outcomes },
                _ => Value::Plain,
            }
        }
        InstructionKind::PlaceTagLocal { local, path } if path.is_empty() => {
            match state.local(*local, world)? {
                Value::Poll { fresh, outcomes } => Value::Tag { fresh, outcomes },
                _ => Value::Plain,
            }
        }
        InstructionKind::VariantTagAddress { address, path } if path.is_empty() => {
            match state.get(*address, world)? {
                Value::Poll { fresh, outcomes } => Value::Tag { fresh, outcomes },
                _ => Value::Plain,
            }
        }
        InstructionKind::PlaceTagAddress { address, path } if path.is_empty() => {
            match state.get(*address, world)? {
                Value::Poll { fresh, outcomes } => Value::Tag { fresh, outcomes },
                _ => Value::Plain,
            }
        }
        InstructionKind::Constant(Constant::I32(v)) => Value::Integer(*v as i64),
        InstructionKind::Constant(Constant::Bool(v)) => Value::Integer(i64::from(*v)),
        InstructionKind::Unary {
            op: UnaryOp::Not,
            operand,
        } => match state.get(*operand, world)? {
            Value::Predicate { fresh, yes, no } => Value::Predicate {
                fresh,
                yes: no,
                no: yes,
            },
            Value::Integer(i) => Value::Integer(i64::from(i == 0)),
            _ => Value::Plain,
        },
        InstructionKind::Binary {
            op: op @ (BinaryOp::Equal | BinaryOp::NotEqual),
            left,
            right,
        } => {
            let a = state.get(*left, world)?;
            let b = state.get(*right, world)?;
            match (a, b) {
                (Value::Tag { fresh, outcomes }, Value::Integer(tag))
                | (Value::Integer(tag), Value::Tag { fresh, outcomes }) => {
                    world.pay(outcomes.iter().fold(0usize, |w, o| {
                        w.saturating_add(o.phases.len()).saturating_add(1)
                    }))?;
                    let (mut yes, mut no) = (Vec::new(), Vec::new());
                    for outcome in outcomes {
                        if (outcome.tag == tag) == (*op == BinaryOp::Equal) {
                            yes.push(outcome);
                        } else {
                            no.push(outcome);
                        }
                    }
                    Value::Predicate { fresh, yes, no }
                }
                _ => Value::Plain,
            }
        }
        InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakeOwnedTuple { fields }
        | InstructionKind::MakeManaged { fields }
        | InstructionKind::MakePackage { fields } => {
            Value::Tuple(operand_values(world, state, fields)?)
        }
        InstructionKind::ExtractField { aggregate, index } => {
            match state.get(*aggregate, world)? {
                Value::Tuple(mut values) => {
                    if *index as usize >= values.len() {
                        return Err(bad(
                            "actual protocol field is outside its complete aggregate",
                        ));
                    }
                    values.remove(*index as usize)
                }
                _ => Value::Plain,
            }
        }
        InstructionKind::MakeVariant { variant, .. }
            if f.return_type == tasks::poll_type(&Type::Unit) =>
        {
            world.pay(state.phases.len().saturating_add(1))?;
            Value::Poll {
                fresh: true,
                outcomes: vec![Outcome {
                    tag: *variant as i64,
                    phases: state.phases.clone(),
                }],
            }
        }
        InstructionKind::Call {
            function,
            arguments,
        }
        | InstructionKind::SuspendCall {
            function,
            arguments,
        } => call(world, state, *function, arguments)?,
        InstructionKind::ForeignCall { arguments, .. } => {
            let values = operand_values(world, state, arguments)?;
            unknown(world, state, &values, false)?;
            Value::Plain
        }
        InstructionKind::CallIndirect { arguments, .. }
        | InstructionKind::CallClosure { arguments, .. }
        | InstructionKind::CallClosureOnce { arguments, .. }
        | InstructionKind::CallClosureBorrowed { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. } => {
            let values = operand_values(world, state, arguments)?;
            unknown(world, state, &values, true)?;
            Value::Plain
        }
        InstructionKind::TaskCreate { arguments, .. }
        | InstructionKind::TaskCreateOwned { arguments, .. } => {
            let values = operand_values(world, state, arguments)?;
            unknown(world, state, &values, false)?;
            Value::Plain
        }
        InstructionKind::MakeClosure { captures, .. } => {
            let values = operand_values(world, state, captures)?;
            let mut roots = BTreeSet::new();
            for value in &values {
                world.pay(value.work())?;
                value.roots(&mut roots);
            }
            if !roots.is_empty() {
                return Err(pending(
                    "actual protocol capture needs its complete creator/child root boundary",
                ));
            }
            Value::Plain
        }
        InstructionKind::DropManaged { local }
        | InstructionKind::DropTuple { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropClosure { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::DropErrorUnion { local } => {
            let mut roots = BTreeSet::new();
            if let Some(value) = state.locals.get(local.0 as usize).and_then(Option::as_ref) {
                world.pay(value.work())?;
                value.roots(&mut roots);
            }
            for root in roots {
                if state.phases[&root] != CLOSED {
                    return Err(bad(
                        "actual lexical registration cleanup lacks Closed for this generation",
                    ));
                }
                state.change(world, root, CLOSED, true)?;
            }
            *state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| bad("protocol cleanup local is absent"))? = None;
            Value::Plain
        }
        InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. }
        | InstructionKind::UnsafeEnter { .. }
        | InstructionKind::UnsafeExit { .. }
        | InstructionKind::RegisterDefer { .. }
        | InstructionKind::Constant(_)
        | InstructionKind::Binary { .. }
        | InstructionKind::Unary { .. }
        | InstructionKind::ConvertInteger { .. }
        | InstructionKind::ConvertNumber { .. }
        | InstructionKind::StaticUtf8 { .. }
        | InstructionKind::FunctionRef { .. }
        | InstructionKind::ForeignFunctionRef { .. }
        | InstructionKind::TaskYield
        | InstructionKind::TaskValid { .. }
        | InstructionKind::TaskPoll { .. }
        | InstructionKind::TaskPollClose { .. }
        | InstructionKind::TaskRequestCancel { .. }
        | InstructionKind::TaskFinishDestroy { .. }
        | InstructionKind::ReadVariant { .. }
        | InstructionKind::ReadCapture { .. }
        | InstructionKind::TakeCapture { .. }
        | InstructionKind::DescribeCapture { .. }
        | InstructionKind::BorrowCapture { .. }
        | InstructionKind::ByteBufferNew
        | InstructionKind::ByteBufferReserve { .. }
        | InstructionKind::ByteBufferPush { .. }
        | InstructionKind::ByteBufferTruncate { .. }
        | InstructionKind::ByteBufferLen { .. }
        | InstructionKind::ByteBufferCapacity { .. }
        | InstructionKind::ByteBufferBorrow { .. } => Value::Plain,
        _ => {
            if !state.phases.is_empty() {
                return Err(pending(
                    "actual protocol opcode requires its complete root and mutation relation",
                ));
            }
            Value::Plain
        }
    };
    if let Some(result) = instruction.result {
        let ty = f
            .values
            .get(result.0 as usize)
            .ok_or_else(|| bad("protocol result has no complete actual type"))?;
        let value = if matches!(value, Value::Plain) && async_head(world, ty)? {
            let root = Root::Created(result);
            state.change(world, root, UNKNOWN, true)?;
            Value::Root(root, true)
        } else {
            value
        };
        *state
            .values
            .get_mut(result.0 as usize)
            .ok_or_else(|| bad("actual protocol instruction result is absent"))? = Some(value);
    }
    Ok(())
}
fn offer(
    world: &mut World<'_, '_>,
    cfg: &flow::Cfg,
    block: BlockId,
    mut state: State,
    seen: &mut [Vec<State>],
    queue: &mut std::collections::VecDeque<(usize, State)>,
) -> R<()> {
    let index = cfg.index(block)?;
    world.pay(state.values.len())?;
    cfg.retain_dominating(index, &mut state.values);
    for old in &seen[index] {
        world.pay(old.work().saturating_add(state.work()))?;
        if old == &state {
            return Ok(());
        }
    }
    let saved = state.copy(world)?;
    seen[index].push(saved);
    queue.push_back((index, state));
    Ok(())
}
fn returned(
    world: &mut World<'_, '_>,
    state: &State,
    f: &TypedFunction,
    result: Option<ValueId>,
    inputs: &BTreeMap<u32, ProtocolState>,
) -> R<()> {
    if f.protocol_contracts.is_empty() {
        return Ok(());
    }
    let value = result
        .map(|id| state.get(id, world))
        .transpose()?
        .unwrap_or(Value::Plain);
    let outcomes = match value {
        Value::Poll {
            fresh: true,
            outcomes,
        } => outcomes,
        Value::Poll { fresh: false, .. } => {
            return Err(bad(
                "actual return uses a Poll invalidated by a later operation on its root",
            ));
        }
        _ => {
            world.pay(state.phases.len().saturating_add(1))?;
            vec![Outcome {
                tag: -1,
                phases: state.phases.clone(),
            }]
        }
    };
    for outcome in outcomes {
        let exit = match outcome.tag {
            -1 => ProtocolExit::Return,
            0 => ProtocolExit::PollPending,
            1 => ProtocolExit::PollReady,
            _ => {
                return Err(bad(
                    "actual protocol return has a nonexistent closed Poll tag",
                ));
            }
        };
        for contract in &f.protocol_contracts {
            crate::registration::charge_contract(contract, |n| world.pay(n))?;
            let input = inputs
                .get(&contract.parameter)
                .ok_or_else(|| bad("actual protocol input case is missing"))?;
            let phase = outcome
                .phases
                .get(&Root::Parameter(contract.parameter))
                .or_else(|| state.phases.get(&Root::Parameter(contract.parameter)))
                .copied()
                .ok_or_else(|| bad("actual protocol return lost its original parameter root"))?;
            let row = contract
                .exits
                .iter()
                .find(|r| r.input == *input && r.exit == exit)
                .ok_or_else(|| {
                    bad("actual reachable return is absent from the original protocol relation")
                })?;
            if phase == 0 || phase & !mask(&row.output) != 0 {
                return Err(bad(
                    "actual CFG exceeds its original per-result protocol phase relation",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn check(world: &mut World<'_, '_>, f: &TypedFunction) -> R<()> {
    if f.foreign.is_some() {
        return Ok(());
    }
    world.pay(
        f.values
            .len()
            .saturating_add(f.locals.len())
            .saturating_add(f.blocks.len()),
    )?;
    let mut relevant = !f.protocol_contracts.is_empty();
    for ty in f.values.iter().chain(f.locals.iter().map(|l| &l.ty)) {
        relevant |= async_head(world, ty)?;
    }
    if !relevant {
        return Ok(());
    }
    let cfg = flow::Cfg::new(world, f)?;
    let mut cases = vec![BTreeMap::new()];
    for contract in &f.protocol_contracts {
        let mut next = Vec::new();
        for case in cases {
            for input in &contract.entry {
                world.pay(case.len().saturating_add(1))?;
                let mut case = case.clone();
                case.insert(contract.parameter, *input);
                next.push(case);
            }
        }
        cases = next;
    }
    for inputs in cases {
        world.pay(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(f.blocks.len()),
        )?;
        let mut state = State {
            locals: vec![None; f.locals.len()],
            values: vec![None; f.values.len()],
            phases: BTreeMap::new(),
        };
        for (ordinal, local) in f.parameters.iter().enumerate() {
            let ty = &f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("actual protocol parameter local is absent"))?
                .ty;
            let value = if async_head(world, ty)? {
                let root = Root::Parameter(ordinal as u32);
                state.phases.insert(
                    root,
                    inputs
                        .get(&(ordinal as u32))
                        .copied()
                        .map(bit)
                        .unwrap_or(UNKNOWN),
                );
                Value::Root(root, true)
            } else {
                Value::Plain
            };
            state.locals[local.0 as usize] = Some(value);
        }
        let mut seen: Vec<Vec<State>> = (0..f.blocks.len()).map(|_| Vec::new()).collect();
        let mut queue = std::collections::VecDeque::new();
        offer(world, &cfg, f.entry, state, &mut seen, &mut queue)?;
        while let Some((index, mut state)) = queue.pop_front() {
            world.pay(1)?;
            let block = &f.blocks[index];
            for op in &block.instructions {
                instruction(world, &mut state, f, op)?;
            }
            match block.terminator {
                Terminator::Return(result) => returned(world, &state, f, result, &inputs)?,
                Terminator::Trap => {}
                Terminator::Jump(target) => {
                    offer(world, &cfg, target, state, &mut seen, &mut queue)?
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let condition = state.get(condition, world)?;
                    match condition {
                        Value::Predicate {
                            fresh: true,
                            yes,
                            no,
                        } => {
                            for (target, outcomes) in [(then_block, yes), (else_block, no)] {
                                for outcome in outcomes {
                                    let mut branch = state.copy(world)?;
                                    for (root, phase) in outcome.phases {
                                        if let Some(current) = branch.phases.get_mut(&root) {
                                            *current &= phase;
                                        } else {
                                            return Err(bad("actual Poll predicate lost its root"));
                                        }
                                    }
                                    if branch.phases.values().all(|m| *m != 0) {
                                        offer(world, &cfg, target, branch, &mut seen, &mut queue)?;
                                    }
                                }
                            }
                        }
                        Value::Predicate { fresh: false, .. } => {
                            let other = state.copy(world)?;
                            offer(world, &cfg, then_block, state, &mut seen, &mut queue)?;
                            offer(world, &cfg, else_block, other, &mut seen, &mut queue)?;
                        }
                        Value::Integer(value) => offer(
                            world,
                            &cfg,
                            if value != 0 { then_block } else { else_block },
                            state,
                            &mut seen,
                            &mut queue,
                        )?,
                        _ => {
                            let other = state.copy(world)?;
                            offer(world, &cfg, then_block, state, &mut seen, &mut queue)?;
                            offer(world, &cfg, else_block, other, &mut seen, &mut queue)?;
                        }
                    }
                }
                Terminator::Unreachable => {
                    return Err(bad(
                        "reachable protocol CFG ends without a proved runtime exit",
                    ));
                }
            }
        }
    }
    Ok(())
}

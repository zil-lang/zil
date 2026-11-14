//! Conditional original-state theorems. Source relations are assumptions only
//! at entry/call boundaries; every original return must independently satisfy
//! them. Actual generations, loans, provider drain and cleanup remain separate.
use super::*;
mod canonical;
mod control;
mod outcomes;
mod owners;
const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 4;
const ANY: u8 = OPEN | CLOSING | CLOSED;
fn bit(value: ProtocolState) -> u8 {
    match value {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
fn mask(values: &[ProtocolState], b: &mut Budget) -> CompileResult<u8> {
    b.spend(values.len(), WORK)?;
    Ok(values.iter().fold(0, |state, value| state | bit(*value)))
}
fn ordered(values: &[ProtocolState]) -> bool {
    !values.is_empty() && values.windows(2).all(|pair| pair[0] < pair[1])
}
fn owner(recipe: &SemanticTypeRecipe) -> bool {
    matches!(recipe,SemanticTypeRecipe::Managed(inner) if matches!(inner.as_ref(),SemanticTypeRecipe::AsyncRegistration(_) | SemanticTypeRecipe::Task { .. }))
}
fn borrow(recipe: &SemanticTypeRecipe) -> bool {
    matches!(recipe,SemanticTypeRecipe::Address {mutable:true,inner,..} if owner(inner))
}
fn poll(
    recipe: &SemanticTypeRecipe,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<bool> {
    Ok(inventory.protocol_poll_recipe(recipe, b)?.is_some())
}

fn shape(
    definition: &SemanticDefinition,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    let SemanticDefinitionBody::FunctionWithBody {
        signature,
        protocol_contracts,
        foreign,
        parameters,
        body,
        ..
    } = &definition.body
    else {
        return Ok(());
    };
    if protocol_contracts.is_empty() {
        return Ok(());
    }
    if foreign.is_some()
        || definition.kind != SemanticDefinitionKind::Function
        || !matches!(body, SemanticOriginalFunctionBody::Defined(_))
    {
        return Err(fail(
            "original protocol relation requires its real ordinary defined body",
        ));
    }
    let is_poll = poll(&signature.result, inventory, b)?;
    let mut previous = None;
    for contract in protocol_contracts {
        b.spend(
            contract
                .entry
                .len()
                .saturating_add(contract.exits.len() + signature.parameters.len() + 1),
            WORK,
        )?;
        if contract.protocol != ProtocolKind::AsyncRegistrationV1
            || !ordered(&contract.entry)
            || contract.exits.is_empty()
            || previous.is_some_and(|value| value >= contract.parameter)
        {
            return Err(fail(
                "original protocol role or entry is not complete, unique and canonical",
            ));
        }
        previous = Some(contract.parameter);
        let parameter = signature
            .parameters
            .get(contract.parameter as usize)
            .ok_or_else(|| fail("original protocol selects no original ordered value parameter"))?;
        if contract.parameter as usize >= parameters.len()
            || !(borrow(&parameter.ty) || owner(&parameter.ty))
        {
            return Err(fail(
                "original protocol parameter is not the complete exclusive or owning registration head",
            ));
        }
        if signature.call_kind == CallKind::Suspend && !owner(&parameter.ty) {
            return Err(fail(
                "original borrowed protocol role cannot cross a Suspend boundary",
            ));
        }
        let mut previous_row = None;
        for row in &contract.exits {
            b.spend(row.output.len().saturating_add(1), WORK)?;
            let key = (row.input, row.exit);
            if previous_row.is_some_and(|value| value >= key)
                || !contract.entry.contains(&row.input)
                || (!row.output.is_empty() && !ordered(&row.output))
                || owner(&parameter.ty) != row.output.is_empty()
            {
                return Err(fail(
                    "original protocol transition loses a role, state or retirement obligation",
                ));
            }
            previous_row = Some(key);
            let allowed = if is_poll {
                matches!(
                    row.exit,
                    ProtocolExit::PollPending | ProtocolExit::PollReady
                )
            } else if matches!(signature.result, SemanticTypeRecipe::ErrorUnion { .. }) {
                matches!(row.exit, ProtocolExit::ErrorOk | ProtocolExit::ErrorErr)
            } else {
                row.exit == ProtocolExit::Return
            };
            if !allowed {
                return Err(fail(
                    "original protocol exit is incompatible with the complete original result carrier",
                ));
            }
        }
    }
    for (ordinal, parameter) in signature.parameters.iter().enumerate() {
        b.spend(protocol_contracts.len().saturating_add(1), WORK)?;
        if (borrow(&parameter.ty) || owner(&parameter.ty))
            && !protocol_contracts
                .iter()
                .any(|contract| contract.parameter as usize == ordinal)
        {
            return Err(fail(
                "original protocol omits another actual registration parameter role",
            ));
        }
    }
    Ok(())
}
#[derive(Clone, PartialEq, Eq, Default)]
struct Data {
    alias: Option<u32>,
    // A value may hide access to a tracked original head through a callable
    // or an aggregate. Calls must not preserve a phase through that exposure.
    exposed: bool,
    outcomes: Vec<(ProtocolExit, BTreeMap<u32, u8>)>,
}
#[derive(Clone, PartialEq, Eq)]
struct State {
    phases: BTreeMap<u32, u8>,
    locals: Vec<(String, Data)>,
    alive: bool,
    cleanup_unknown: bool,
}
struct Proof<'a, 'i> {
    definition: &'a SemanticDefinition,
    inventory: &'i Inventory<'a>,
    checked: &'i Checked,
    contract: Option<&'a FunctionProtocolContract>,
    input: Option<ProtocolState>,
    loops: std::cell::RefCell<Vec<control::LoopFlow>>,
}
impl State {
    fn copy(&self, b: &mut Budget) -> CompileResult<Self> {
        let mut locals = Vec::new();
        for (name, data) in &self.locals {
            b.spend(
                name.len().saturating_add(
                    data.outcomes
                        .iter()
                        .map(|(_, states)| states.len() + 1)
                        .sum::<usize>()
                        + 1,
                ),
                WORK,
            )?;
            Solver::push(&mut locals, (name.clone(), data.clone()), b)?;
        }
        b.spend(self.phases.len(), WORK)?;
        Ok(Self {
            phases: self.phases.clone(),
            locals,
            alive: self.alive,
            cleanup_unknown: self.cleanup_unknown,
        })
    }
    fn invalidate(&mut self, b: &mut Budget) -> CompileResult<()> {
        for (_, data) in &mut self.locals {
            b.spend(data.outcomes.len().saturating_add(1), WORK)?;
            data.outcomes.clear();
        }
        Ok(())
    }
    fn opaque(&mut self, b: &mut Budget) -> CompileResult<()> {
        self.invalidate(b)?;
        for value in self.phases.values_mut() {
            b.spend(1, WORK)?;
            if *value != 0 {
                *value = ANY;
            }
        }
        Ok(())
    }
    fn join(&mut self, other: &Self, b: &mut Budget) -> CompileResult<()> {
        if !other.alive {
            return Ok(());
        }
        if !self.alive {
            *self = other.copy(b)?;
            return Ok(());
        }
        if self.locals.len() != other.locals.len() {
            return Err(fail(
                "original protocol branch changes its enclosing lexical inventory",
            ));
        }
        self.cleanup_unknown |= other.cleanup_unknown;
        for (role, value) in &other.phases {
            b.lookup(self.phases.len(), WORK)?;
            *self.phases.entry(*role).or_insert(0) |= *value;
        }
        for ((name, data), (other_name, other_data)) in self.locals.iter_mut().zip(&other.locals) {
            b.spend(
                name.len().saturating_add(
                    other_name.len()
                        + data
                            .outcomes
                            .iter()
                            .map(|(_, states)| states.len() + 1)
                            .sum::<usize>()
                        + other_data
                            .outcomes
                            .iter()
                            .map(|(_, states)| states.len() + 1)
                            .sum::<usize>()
                        + 1,
                ),
                WORK,
            )?;
            if name != other_name {
                return Err(fail("original protocol join changes a lexical binder"));
            }
            if data.alias != other_data.alias {
                return Err(unknown(
                    "original protocol alias join requires its complete possible-head relation",
                ));
            }
            data.exposed |= other_data.exposed;
            if data.outcomes != other_data.outcomes {
                data.outcomes.clear();
            }
        }
        Ok(())
    }
}
impl Proof<'_, '_> {
    fn step(&self, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource("original protocol proof exceeds its finite depth"));
        }
        Ok(())
    }
    fn passive_drop(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        self.step(b, depth)?;
        match &self.checked.arena[self.checked.root(term, b)?] {
            Kind::Atom(ty) => Ok(ty.is_b0_scalar()),
            Kind::Address { .. } => Ok(true),
            Kind::CallResult { hidden, .. } => self.passive_drop(*hidden, b, depth + 1),
            Kind::Array { element, .. } => self.passive_drop(*element, b, depth + 1),
            Kind::ErrorUnion { error, result } => Ok(self.passive_drop(*error, b, depth + 1)?
                && self.passive_drop(*result, b, depth + 1)?),
            Kind::Code { .. } => Ok(true),
            Kind::Tuple(fields) => {
                for field in fields {
                    if !self.passive_drop(*field, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Nominal {
                definition, fields, ..
            } => {
                let definition = self.inventory.definition(*definition, b)?;
                if matches!(
                    definition.body,
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Resource,
                        ..
                    }
                ) {
                    return Ok(false);
                }
                for field in fields {
                    if !self.passive_drop(*field, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn plain(&self, data: &Data) -> CompileResult<()> {
        if !data.outcomes.is_empty() {
            return Err(unknown(
                "original protocol result needs its exact tag-sensitive consuming operation",
            ));
        }
        Ok(())
    }
    fn exit(&self, state: &State, data: &Data, b: &mut Budget) -> CompileResult<()> {
        if !state.alive {
            return Ok(());
        }
        let Some(contract) = self.contract else {
            return Ok(());
        };
        let input = self
            .input
            .ok_or_else(|| fail("original protocol proof omits its own entry state"))?;
        let check =
            |exit: ProtocolExit, states: &BTreeMap<u32, u8>, b: &mut Budget| -> CompileResult<()> {
                let mut actual = *states
                    .get(&contract.parameter)
                    .ok_or_else(|| fail("original return loses its protocol role"))?;
                if state.cleanup_unknown && actual != 0 {
                    actual = ANY;
                }
                b.spend(contract.exits.len().saturating_add(states.len() + 1), WORK)?;
                let row = contract
                    .exits
                    .iter()
                    .find(|row| row.input == input && row.exit == exit)
                    .ok_or_else(|| {
                        fail("original return is absent from its declared protocol upper relation")
                    })?;
                if actual & !mask(&row.output, b)? != 0 {
                    return Err(fail(
                        "original return exceeds its declared protocol post-state upper",
                    ));
                }
                Ok(())
            };
        if data.outcomes.is_empty() {
            check(ProtocolExit::Return, &state.phases, b)
        } else {
            for (exit, states) in &data.outcomes {
                check(*exit, states, b)?;
            }
            Ok(())
        }
    }
    fn expression(
        &self,
        state: &mut State,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Data> {
        self.step(b, depth)?;
        match &value.kind {
            CoreExprKind::Name(name) => {
                for (key, data) in state.locals.iter().rev() {
                    b.spend(
                        key.len().saturating_add(
                            name.len()
                                + data
                                    .outcomes
                                    .iter()
                                    .map(|(_, states)| states.len() + 1)
                                    .sum::<usize>()
                                + 1,
                        ),
                        WORK,
                    )?;
                    if key == name {
                        return Ok(data.clone());
                    }
                }
                Ok(Data::default())
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.expression(state, value, b, depth + 1)
            }
            CoreExprKind::Try(value) => self.try_value(state, value, b, depth + 1),
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Give,
                operand,
            } => self.expression(state, operand, b, depth + 1),
            CoreExprKind::Call { callee, arguments } => {
                let mut actuals = Vec::new();
                for argument in arguments {
                    let data = self.expression(state, argument, b, depth + 1)?;
                    self.plain(&data)?;
                    Solver::push(&mut actuals, data, b)?;
                }
                b.lookup(self.checked.enum_constructors.len(), WORK)?;
                if let Some(constructor) = self
                    .checked
                    .enum_constructors
                    .get(&(self.definition.id, value.id))
                {
                    let Kind::Nominal { .. } =
                        &self.checked.arena[self.checked.root(constructor.value, b)?]
                    else {
                        return Err(fail(
                            "original protocol tag loses its nominal member identity",
                        ));
                    };
                    if self.term_poll(constructor.value, b)?.is_some() {
                        let exit = match constructor.variant {
                            0 => ProtocolExit::PollPending,
                            1 => ProtocolExit::PollReady,
                            _ => {
                                return Err(fail(
                                    "original protocol tag exceeds the actual Poll declaration",
                                ));
                            }
                        };
                        b.spend(state.phases.len(), WORK)?;
                        return Ok(Data {
                            alias: None,
                            exposed: false,
                            outcomes: vec![(exit, state.phases.clone())],
                        });
                    }
                    return Ok(Data::default());
                }
                let mut names = BTreeSet::new();
                for (name, _) in &state.locals {
                    b.spend(name.len().saturating_add(names.len() + 1), WORK)?;
                    names.insert(name.clone());
                }
                let resolution = if let Some(path) = sealed::path(callee, b, depth + 1)? {
                    sealed::resolve(self.inventory, self.definition.id, &path, &names, b)?
                } else {
                    sealed::Resolution::Deferred
                };
                match resolution {
                    sealed::Resolution::Intrinsic(
                        id @ (sealed::Intrinsic::AsyncRequest
                        | sealed::Intrinsic::AsyncPoll
                        | sealed::Intrinsic::AsyncFinish
                        | sealed::Intrinsic::AsyncFinishChecked
                        | sealed::Intrinsic::TaskCancel
                        | sealed::Intrinsic::TaskPoll
                        | sealed::Intrinsic::TaskPollClose
                        | sealed::Intrinsic::TaskFinish),
                    ) => {
                        let [data] = actuals.as_slice() else {
                            return Err(fail(
                                "original registration operation changes its state-role arity",
                            ));
                        };
                        let role=data.alias.ok_or_else(||unknown("original registration operation needs its exact live parameter head"))?;
                        let old = *state.phases.get(&role).ok_or_else(|| {
                            fail("original registration state parameter is absent")
                        })?;
                        if old == 0 {
                            return Err(fail(
                                "original registration operation uses a retired parameter head",
                            ));
                        }
                        state.invalidate(b)?;
                        match id {
                            sealed::Intrinsic::AsyncRequest | sealed::Intrinsic::TaskCancel => {
                                state.phases.insert(
                                    role,
                                    (old & CLOSED)
                                        | if old & (OPEN | CLOSING) != 0 {
                                            CLOSING
                                        } else {
                                            0
                                        },
                                );
                                Ok(Data::default())
                            }
                            sealed::Intrinsic::AsyncFinish
                            | sealed::Intrinsic::AsyncFinishChecked
                            | sealed::Intrinsic::TaskFinish => {
                                if matches!(
                                    id,
                                    sealed::Intrinsic::AsyncFinishChecked
                                        | sealed::Intrinsic::TaskFinish
                                ) && old != CLOSED
                                {
                                    return Err(fail(
                                        "original checked destroy needs the same current Closed parameter",
                                    ));
                                }
                                state.phases.insert(role, 0);
                                Ok(Data::default())
                            }
                            sealed::Intrinsic::AsyncPoll | sealed::Intrinsic::TaskPollClose => {
                                let mut outcomes = Vec::new();
                                if old & (OPEN | CLOSING) != 0 {
                                    b.spend(state.phases.len(), WORK)?;
                                    let mut pending = state.phases.clone();
                                    pending.insert(role, CLOSING);
                                    Solver::push(
                                        &mut outcomes,
                                        (ProtocolExit::PollPending, pending),
                                        b,
                                    )?;
                                }
                                b.spend(state.phases.len(), WORK)?;
                                let mut ready = state.phases.clone();
                                ready.insert(role, CLOSED);
                                Solver::push(&mut outcomes, (ProtocolExit::PollReady, ready), b)?;
                                state.phases.insert(
                                    role,
                                    if old & (OPEN | CLOSING) != 0 {
                                        CLOSING | CLOSED
                                    } else {
                                        CLOSED
                                    },
                                );
                                Ok(Data {
                                    alias: None,
                                    exposed: false,
                                    outcomes,
                                })
                            }
                            sealed::Intrinsic::TaskPoll => {
                                // Ordinary worker progress cannot certify that
                                // the owning frame's close/drain has completed.
                                state.phases.insert(
                                    role,
                                    if old & OPEN != 0 {
                                        ANY
                                    } else {
                                        old | if old & CLOSING != 0 { CLOSED } else { 0 }
                                    },
                                );
                                Ok(Data {
                                    alias: None,
                                    exposed: true,
                                    outcomes: Vec::new(),
                                })
                            }
                            _ => Err(fail(
                                "original protocol operation has another sealed identity",
                            )),
                        }
                    }
                    sealed::Resolution::Intrinsic(
                        sealed::Intrinsic::NumericConvert(_)
                        | sealed::Intrinsic::AsyncValid
                        | sealed::Intrinsic::RegistrationValid
                        | sealed::Intrinsic::SliceLength,
                    ) => Ok(Data::default()),
                    sealed::Resolution::Intrinsic(
                        sealed::Intrinsic::RegisterAsync | sealed::Intrinsic::TaskCreate,
                    ) => {
                        state.call_effect(&actuals, &[], b)?;
                        self.fresh_owner(state, value.id, b)
                    }
                    sealed::Resolution::Definition(target) => {
                        let data = self.call(state, target, &actuals, b)?;
                        let frame = self.checked.frames.get(&target).ok_or_else(|| {
                            fail("original protocol callee has no checked declaration domain")
                        })?;
                        if data.alias.is_none()
                            && data.outcomes.is_empty()
                            && self.async_owner(frame.result, b)?
                        {
                            self.fresh_owner(state, value.id, b)
                        } else {
                            Ok(data)
                        }
                    }
                    sealed::Resolution::Intrinsic(_) => {
                        state.call_effect(&actuals, &[], b)?;
                        Ok(Data::default())
                    }
                    _ => {
                        state.opaque(b)?;
                        Ok(Data::default())
                    }
                }
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expression(state, condition, b, depth + 1)?;
                self.plain(&condition)?;
                let mut right = state.copy(b)?;
                let left_value = self.expression(state, then_value, b, depth + 1)?;
                let right_value = self.expression(&mut right, else_value, b, depth + 1)?;
                state.join(&right, b)?;
                if left_value != right_value {
                    return Err(unknown(
                        "original protocol conditional result requires its complete outcome relation",
                    ));
                }
                Ok(left_value)
            }
            CoreExprKind::Binary {
                op: BinaryOp::And | BinaryOp::Or,
                left,
                right,
            } => {
                let left = self.expression(state, left, b, depth + 1)?;
                self.plain(&left)?;
                let skipped = state.copy(b)?;
                let right = self.expression(state, right, b, depth + 1)?;
                self.plain(&right)?;
                state.join(&skipped, b)?;
                Ok(Data::default())
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Range {
                start: left,
                end: right,
            } => {
                let left = self.expression(state, left, b, depth + 1)?;
                self.plain(&left)?;
                let right = self.expression(state, right, b, depth + 1)?;
                self.plain(&right)?;
                Ok(Data::default())
            }
            CoreExprKind::Index { object, index } => {
                let object = self.expression(state, object, b, depth + 1)?;
                self.plain(&object)?;
                let index = self.expression(state, index, b, depth + 1)?;
                self.plain(&index)?;
                Ok(Data {
                    alias: None,
                    exposed: object.exposed || object.alias.is_some(),
                    outcomes: Vec::new(),
                })
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let data = self.expression(state, operand, b, depth + 1)?;
                self.plain(&data)?;
                Ok(Data {
                    alias: None,
                    exposed: data.exposed || data.alias.is_some(),
                    outcomes: Vec::new(),
                })
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::NumericConvert { value: operand, .. }
            | CoreExprKind::IntegerConvert { value: operand, .. } => {
                let data = self.expression(state, operand, b, depth + 1)?;
                self.plain(&data)?;
                Ok(Data::default())
            }
            CoreExprKind::Member { object, .. } => {
                let data = self.expression(state, object, b, depth + 1)?;
                self.plain(&data)?;
                Ok(Data {
                    alias: None,
                    exposed: data.exposed || data.alias.is_some(),
                    outcomes: Vec::new(),
                })
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                let mut exposed = false;
                for field in fields {
                    let data = self.expression(state, field, b, depth + 1)?;
                    self.plain(&data)?;
                    exposed |= data.exposed;
                    if data.alias.is_some() {
                        return Err(unknown(
                            "original protocol aggregate needs its complete nested owner-place identity",
                        ));
                    }
                }
                Ok(Data {
                    alias: None,
                    exposed,
                    outcomes: Vec::new(),
                })
            }
            CoreExprKind::Record { fields, .. }
            | CoreExprKind::RecordConstructor { fields, .. } => {
                let mut exposed = false;
                for (_, field) in fields {
                    let data = self.expression(state, field, b, depth + 1)?;
                    self.plain(&data)?;
                    exposed |= data.exposed;
                    if data.alias.is_some() {
                        return Err(unknown(
                            "original protocol product needs its complete nested owner-place identity",
                        ));
                    }
                }
                Ok(Data {
                    alias: None,
                    exposed,
                    outcomes: Vec::new(),
                })
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                let data = self.expression(state, base, b, depth + 1)?;
                self.plain(&data)?;
                let mut exposed = data.exposed;
                if data.alias.is_some() {
                    return Err(unknown(
                        "original protocol update needs its complete old and new owning-head identities",
                    ));
                }
                for (_, field) in fields {
                    let data = self.expression(state, field, b, depth + 1)?;
                    self.plain(&data)?;
                    exposed |= data.exposed;
                    if data.alias.is_some() {
                        return Err(unknown(
                            "original protocol update needs its complete nested owner-place identity",
                        ));
                    }
                }
                Ok(Data {
                    alias: None,
                    exposed,
                    outcomes: Vec::new(),
                })
            }
            CoreExprKind::Sequence { bindings, value } => {
                let scope = state.locals.len();
                for (name, value) in bindings {
                    let data = self.expression(state, value, b, depth + 1)?;
                    b.spend(name.len(), WORK)?;
                    Solver::push(&mut state.locals, (name.clone(), data), b)?;
                }
                let result = self.expression(state, value, b, depth + 1)?;
                state.locals.truncate(scope);
                Ok(result)
            }
            CoreExprKind::Slice { object, start, end } => {
                let data = self.expression(state, object, b, depth + 1)?;
                self.plain(&data)?;
                for bound in start.iter().chain(end) {
                    let data = self.expression(state, bound, b, depth + 1)?;
                    self.plain(&data)?;
                }
                Ok(Data {
                    alias: None,
                    exposed: data.exposed || data.alias.is_some(),
                    outcomes: Vec::new(),
                })
            }
            CoreExprKind::Closure { .. } => {
                state.cleanup_unknown = true;
                Ok(Data {
                    alias: None,
                    exposed: state.phases.values().any(|phase| *phase != 0),
                    outcomes: Vec::new(),
                })
            }
            CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::Unit
            | CoreExprKind::Literal(_)
            | CoreExprKind::Character(_)
            | CoreExprKind::String(_) => Ok(Data::default()),
            _ => Err(unknown(
                "original protocol expression requires its complete state and result transfer",
            )),
        }
    }
    fn call(
        &self,
        state: &mut State,
        target: SemanticDefinitionId,
        arguments: &[Data],
        b: &mut Budget,
    ) -> CompileResult<Data> {
        let definition = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts,
            signature,
            ..
        } = &definition.body
        else {
            return Err(fail(
                "original state call selects no actual function declaration",
            ));
        };
        if protocol_contracts.is_empty() {
            if signature.parameters.len() != arguments.len() {
                return Err(fail(
                    "original protocol call changes its complete ordered value arity",
                ));
            }
            let mut consumed = Vec::new();
            for parameter in &signature.parameters {
                Solver::push(&mut consumed, owner(&parameter.ty), b)?;
            }
            state.call_effect(arguments, &consumed, b)?;
            return Ok(Data {
                alias: None,
                exposed: arguments
                    .iter()
                    .any(|argument| argument.exposed || argument.alias.is_some()),
                outcomes: Vec::new(),
            });
        }
        let before = state.copy(b)?;
        state.invalidate(b)?;
        let mut exits: BTreeMap<ProtocolExit, BTreeMap<u32, u8>> = BTreeMap::new();
        let mut mapped = BTreeSet::new();
        let mut outputs: BTreeMap<(u32, ProtocolExit), u8> = BTreeMap::new();
        for contract in protocol_contracts {
            let role = arguments
                .get(contract.parameter as usize)
                .and_then(|data| data.alias)
                .ok_or_else(|| {
                    unknown("original state call needs its actual registration argument identity")
                })?;
            b.lookup(mapped.len(), WORK)?;
            if !mapped.insert(role) {
                return Err(unknown(
                    "original state call aliases distinct exclusive contract parameters",
                ));
            }
            let current = *before
                .phases
                .get(&role)
                .ok_or_else(|| fail("original call state role is absent"))?;
            if current == 0 || current & !mask(&contract.entry, b)? != 0 {
                return Err(fail(
                    "original caller does not satisfy the callee's complete protocol entry",
                ));
            }
            for row in &contract.exits {
                b.spend(
                    before.phases.len().saturating_add(row.output.len() + 1),
                    WORK,
                )?;
                if current & bit(row.input) == 0 {
                    continue;
                }
                b.lookup(outputs.len(), WORK)?;
                *outputs.entry((role, row.exit)).or_insert(0) |= mask(&row.output, b)?;
                exits
                    .entry(row.exit)
                    .or_insert_with(|| before.phases.clone());
            }
        }
        let mut possible = BTreeMap::new();
        for (exit, mut states) in exits {
            let mut complete = true;
            for role in &mapped {
                b.lookup(outputs.len(), WORK)?;
                if let Some(output) = outputs.get(&(*role, exit)) {
                    states.insert(*role, *output);
                } else {
                    complete = false;
                }
            }
            if complete {
                b.lookup(possible.len(), WORK)?;
                possible.insert(exit, states);
            }
        }
        let mut exits = possible;
        // Roles not covered by the call cannot inherit an unproved unchanged
        // fact. A callee may have other effects; retain only ANY/live there.
        for states in exits.values_mut() {
            for (role, value) in states.iter_mut() {
                b.spend(1, WORK)?;
                if !mapped.contains(role) && *value != 0 {
                    *value = ANY;
                }
            }
        }
        let mut union = before.phases;
        for value in union.values_mut() {
            *value = 0;
        }
        let mut outcomes = Vec::new();
        for (exit, states) in exits {
            for (role, value) in &states {
                b.spend(1, WORK)?;
                *union
                    .get_mut(role)
                    .ok_or_else(|| fail("original state call adds an unknown role"))? |= *value;
            }
            Solver::push(&mut outcomes, (exit, states), b)?;
        }
        state.phases = union;
        if outcomes.is_empty() {
            state.alive = false;
        }
        if outcomes.len() == 1 && outcomes[0].0 == ProtocolExit::Return {
            Ok(Data::default())
        } else {
            Ok(Data {
                alias: None,
                exposed: false,
                outcomes,
            })
        }
    }
    fn block(
        &self,
        state: &mut State,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        let scope = state.locals.len();
        for statement in &body.statements {
            self.step(b, depth + 1)?;
            if !state.alive {
                break;
            }
            match &statement.kind {
                CoreStatementKind::Binding {
                    name, initializer, ..
                } => {
                    let data = if let Some(value) = initializer {
                        self.expression(state, value, b, depth + 1)?
                    } else {
                        Data::default()
                    };
                    let frame = self
                        .checked
                        .frames
                        .get(&self.definition.id)
                        .ok_or_else(|| {
                            fail("original protocol binding has no checked source frame")
                        })?;
                    b.lookup(frame.bindings.len(), WORK)?;
                    let term = *frame.bindings.get(&statement.id).ok_or_else(|| {
                        fail("original protocol binding loses its exact source term")
                    })?;
                    if !self.passive_drop(term, b, depth + 1)? {
                        state.cleanup_unknown = true;
                    }
                    b.spend(name.len(), WORK)?;
                    Solver::push(&mut state.locals, (name.clone(), data), b)?;
                }
                CoreStatementKind::Expression(value) => {
                    self.expression(state, value, b, depth + 1)?;
                }
                CoreStatementKind::Return(value) => {
                    let data = if let Some(value) = value {
                        self.expression(state, value, b, depth + 1)?
                    } else {
                        Data::default()
                    };
                    let data = self.return_value(state, value.as_ref(), data, b)?;
                    self.exit(state, &data, b)?;
                    state.alive = false;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let condition = self.expression(state, condition, b, depth + 1)?;
                    self.plain(&condition)?;
                    let mut other = state.copy(b)?;
                    self.block(state, then_body, b, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(&mut other, body, b, depth + 1)?;
                    }
                    state.join(&other, b)?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => self.if_let_block(
                    state,
                    pattern,
                    value,
                    then_body,
                    else_body.as_ref(),
                    b,
                    depth + 1,
                )?,
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => self.while_let_block(state, pattern, value, body, b, depth + 1)?,
                CoreStatementKind::TupleBinding {
                    pattern,
                    initializer,
                    ..
                } => {
                    let data = self.expression(state, initializer, b, depth + 1)?;
                    self.plain(&data)?;
                    self.bind_pattern(state, pattern, &data, b, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    let start = self.expression(state, start, b, depth + 1)?;
                    self.plain(&start)?;
                    let end = self.expression(state, end, b, depth + 1)?;
                    self.plain(&end)?;
                    let scope = state.locals.len();
                    b.spend(name.len(), WORK)?;
                    Solver::push(&mut state.locals, (name.clone(), Data::default()), b)?;
                    self.loop_flow(state, None, body, true, b, depth + 1)?;
                    state.locals.truncate(scope);
                }
                CoreStatementKind::While { condition, body } => {
                    self.loop_block(state, Some(condition), body, b, depth + 1)?;
                }
                CoreStatementKind::Loop(body) => {
                    self.loop_block(state, None, body, b, depth + 1)?
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => self.jump(
                    state,
                    matches!(&statement.kind, CoreStatementKind::Break),
                    b,
                )?,
                CoreStatementKind::Match { value, arms } => {
                    self.match_block(state, value, arms, b, depth + 1)?
                }
                CoreStatementKind::CompoundAssign { target, value, .. } => {
                    let target = self.expression(state, target, b, depth + 1)?;
                    self.plain(&target)?;
                    let value = self.expression(state, value, b, depth + 1)?;
                    self.plain(&value)?;
                    if target.alias.is_some() || value.alias.is_some() {
                        return Err(fail(
                            "original protocol head cannot be a scalar compound-write operand",
                        ));
                    }
                }
                CoreStatementKind::With { name, value, body } => {
                    let data = self.expression(state, value, b, depth + 1)?;
                    let scope = state.locals.len();
                    b.spend(name.len(), WORK)?;
                    Solver::push(&mut state.locals, (name.clone(), data), b)?;
                    self.block(state, body, b, depth + 1)?;
                    state.locals.truncate(scope);
                }
                CoreStatementKind::Assign { target, value } => {
                    let data = self.expression(state, value, b, depth + 1)?;
                    let CoreExprKind::Name(name) = &target.kind else {
                        state.opaque(b)?;
                        continue;
                    };
                    let mut found = false;
                    for (key, old) in state.locals.iter_mut().rev() {
                        b.spend(key.len().saturating_add(name.len() + 1), WORK)?;
                        if key == name {
                            if old.alias.is_some() {
                                return Err(unknown(
                                    "original registration parameter replacement requires its new object state and responsibility",
                                ));
                            }
                            *old = data;
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(fail(
                            "original protocol assignment has no actual lexical binder",
                        ));
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(state, body, b, depth + 1)?
                }
                _ => {
                    return Err(unknown(
                        "original protocol control flow needs its complete loop, pattern and cleanup fixed point",
                    ));
                }
            }
        }
        state.locals.truncate(scope);
        Ok(())
    }
}
pub(super) fn check(
    checked: &Checked,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    // Validate every distributed constructor, including unused definitions.
    // An original ID alone never authenticates a Poll/error/failure shape.
    let mut builtins = BTreeSet::new();
    for definition in inventory.definitions.values() {
        if let SemanticDefinitionBody::Nominal { declaration, .. } = &definition.body {
            if [
                zil_ir::builtin_life::POLL,
                zil_ir::builtin_life::TASK_CREATE_ERROR,
                zil_ir::builtin_life::TASK_POLL_STATE,
                zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD,
                zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE,
            ]
            .contains(declaration)
            {
                b.lookup(builtins.len(), WORK)?;
                if builtins.insert(*declaration) {
                    inventory.protocol_task_definition(*declaration, b)?;
                }
            }
        }
        shape(definition, inventory, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts,
            parameters,
            body: SemanticOriginalFunctionBody::Defined(body),
            ..
        } = &definition.body
        else {
            continue;
        };
        if protocol_contracts.is_empty() {
            let mut active = false;
            for ((owner, _), id) in &checked.intrinsics {
                b.spend(1, WORK)?;
                active |= *owner == definition.id
                    && matches!(
                        id,
                        sealed::Intrinsic::RegisterAsync
                            | sealed::Intrinsic::AsyncRequest
                            | sealed::Intrinsic::AsyncPoll
                            | sealed::Intrinsic::AsyncFinish
                            | sealed::Intrinsic::AsyncFinishChecked
                            | sealed::Intrinsic::TaskCreate
                            | sealed::Intrinsic::TaskPoll
                            | sealed::Intrinsic::TaskCancel
                            | sealed::Intrinsic::TaskPollClose
                            | sealed::Intrinsic::TaskFinish
                    );
            }
            for ((owner, _), call) in &checked.calls {
                b.spend(1, WORK)?;
                if *owner != definition.id {
                    continue;
                }
                let target = inventory.definition(call.target, b)?;
                if let SemanticDefinitionBody::FunctionWithBody {
                    protocol_contracts, ..
                } = &target.body
                {
                    b.spend(protocol_contracts.len(), WORK)?;
                    active |= !protocol_contracts.is_empty();
                }
            }
            if active {
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
                else {
                    return Err(fail(
                        "original state operation lacks its declaration signature",
                    ));
                };
                let mut phases = BTreeMap::new();
                let mut locals = Vec::new();
                let proof = Proof {
                    definition,
                    inventory,
                    checked,
                    contract: None,
                    input: None,
                    loops: std::cell::RefCell::new(Vec::new()),
                };
                proof.captured_heads(&mut phases, &mut locals, b)?;
                for (ordinal, (parameter, recipe)) in
                    parameters.iter().zip(&signature.parameters).enumerate()
                {
                    let alias = if owner(&recipe.ty) || borrow(&recipe.ty) {
                        let ordinal = u32::try_from(ordinal)
                            .map_err(|_| resource("original state parameter ordinal overflow"))?;
                        b.lookup(phases.len(), WORK)?;
                        phases.insert(ordinal, ANY);
                        Some(ordinal)
                    } else {
                        None
                    };
                    b.spend(parameter.name.len(), WORK)?;
                    Solver::push(
                        &mut locals,
                        (
                            parameter.name.clone(),
                            Data {
                                alias,
                                exposed: false,
                                outcomes: Vec::new(),
                            },
                        ),
                        b,
                    )?;
                }
                let mut state = State {
                    phases,
                    locals,
                    alive: true,
                    cleanup_unknown: true,
                };
                proof.block(&mut state, body, b, 0)?;
            }
        }
        for contract in protocol_contracts {
            for input in &contract.entry {
                let mut phases = BTreeMap::new();
                for role in protocol_contracts {
                    let state = if role.parameter == contract.parameter {
                        bit(*input)
                    } else {
                        mask(&role.entry, b)?
                    };
                    b.lookup(phases.len(), WORK)?;
                    phases.insert(role.parameter, state);
                }
                let mut locals = Vec::new();
                for (ordinal, parameter) in parameters.iter().enumerate() {
                    b.spend(parameter.name.len(), WORK)?;
                    let data = Data {
                        alias: phases
                            .contains_key(&(ordinal as u32))
                            .then_some(ordinal as u32),
                        exposed: false,
                        outcomes: Vec::new(),
                    };
                    Solver::push(&mut locals, (parameter.name.clone(), data), b)?;
                }
                let mut state = State {
                    phases,
                    locals,
                    alive: true,
                    cleanup_unknown: false,
                };
                let proof = Proof {
                    definition,
                    inventory,
                    checked,
                    contract: Some(contract),
                    input: Some(*input),
                    loops: std::cell::RefCell::new(Vec::new()),
                };
                let frame = checked.frames.get(&definition.id).ok_or_else(|| {
                    fail("original protocol function lacks its checked value parameters")
                })?;
                for parameter in &frame.parameters {
                    if !proof.passive_drop(*parameter, b, 0)? {
                        state.cleanup_unknown = true;
                    }
                }
                proof.block(&mut state, body, b, 0)?;
                if state.alive {
                    proof.exit(&state, &Data::default(), b)?;
                }
            }
        }
    }
    Ok(())
}

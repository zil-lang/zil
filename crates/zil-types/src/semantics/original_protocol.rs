//! Conditional protocol relations are proved from original operations. No
//! runtime layout, instance specialization, or proposed Typed table is input.
use super::*;

const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 4;
const UNKNOWN: u8 = OPEN | CLOSING | CLOSED;

fn bit(s: ProtocolState) -> u8 {
    match s {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
fn mask(states: &[ProtocolState]) -> u8 {
    states.iter().fold(0, |m, s| m | bit(*s))
}
fn failure(message: &str) -> Diagnostic {
    error("E-SEMANTIC-PROTOCOL", Span::default(), message)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Parameter(SemanticDefinitionId, u32),
    Created(SemanticDefinitionId, NodeId),
}
#[derive(Clone, PartialEq, Eq)]
struct Outcome {
    exit: ProtocolExit,
    phases: BTreeMap<Root, u8>,
}
#[derive(Clone, PartialEq, Eq)]
enum Value {
    Plain,
    Root(Root, bool),
    Valid {
        root: Root,
        fresh: bool,
        negated: bool,
    },
    Poll {
        fresh: bool,
        outcomes: Vec<Outcome>,
    },
    Tuple(Vec<Value>),
    Fields(BTreeMap<String, Value>),
}
impl Value {
    fn work(&self) -> usize {
        match self {
            Self::Poll { outcomes, .. } => outcomes
                .iter()
                .fold(1, |w, o| w.saturating_add(o.phases.len()).saturating_add(1)),
            Self::Tuple(v) => v.iter().fold(1, |w, v| w.saturating_add(v.work())),
            Self::Fields(v) => v.iter().fold(1, |w, (k, v)| {
                w.saturating_add(k.len()).saturating_add(v.work())
            }),
            _ => 1,
        }
    }
    fn roots(&self, out: &mut BTreeSet<Root>) {
        match self {
            Self::Root(root, true) => {
                out.insert(*root);
            }
            Self::Tuple(values) => {
                for value in values {
                    value.roots(out);
                }
            }
            Self::Fields(values) => {
                for value in values.values() {
                    value.roots(out);
                }
            }
            _ => {}
        }
    }
    fn invalidate(&mut self, root: Root, replace: bool) {
        match self {
            Self::Root(id, live) if *id == root && replace => *live = false,
            Self::Poll { fresh, outcomes }
                if outcomes.iter().any(|o| o.phases.contains_key(&root)) =>
            {
                *fresh = false
            }
            Self::Valid {
                root: id, fresh, ..
            } if *id == root => *fresh = false,
            Self::Tuple(values) => {
                for value in values {
                    value.invalidate(root, replace);
                }
            }
            Self::Fields(values) => {
                for value in values.values_mut() {
                    value.invalidate(root, replace);
                }
            }
            _ => {}
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Next,
    Return,
    Break,
    Continue,
}
#[derive(Clone, PartialEq, Eq)]
struct State {
    // Stacks preserve hidden lexical binders, including their stale Polls.
    names: BTreeMap<String, Vec<Value>>,
    roots: BTreeMap<Root, u8>,
    control: Control,
    returned: Value,
    temporaries: Vec<Value>,
}
impl State {
    fn work(&self) -> usize {
        self.names.iter().fold(
            self.temporaries.iter().fold(
                self.roots
                    .len()
                    .saturating_add(self.returned.work())
                    .saturating_add(1),
                |w, v| w.saturating_add(v.work()),
            ),
            |w, (name, values)| {
                values
                    .iter()
                    .fold(w.saturating_add(name.len()).saturating_add(1), |w, v| {
                        w.saturating_add(v.work())
                    })
            },
        )
    }
}
struct Engine<'a, 'b> {
    program: &'a SemanticModule,
    proof: &'b mut Proof,
    limits: &'b Limits,
    left: &'b mut usize,
    active: BTreeSet<SemanticDefinitionId>,
    relevant: BTreeSet<SemanticDefinitionId>,
}
impl<'a> Engine<'a, '_> {
    fn pay(&mut self, n: usize, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original protocol work exceeded", None)
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original protocol depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.pay(self.program.definitions.len(), 0)?;
        self.program
            .definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| failure("protocol target has no original definition"))
    }
    fn copy(&mut self, state: &State, depth: usize) -> R<State> {
        self.pay(state.work(), depth)?;
        Ok(state.clone())
    }
    fn value_copy(&mut self, value: &Value, depth: usize) -> R<Value> {
        self.pay(value.work(), depth)?;
        Ok(value.clone())
    }
    fn root(&mut self, value: &Value, state: &State, depth: usize) -> R<Root> {
        self.pay(value.work().saturating_add(state.roots.len()), depth)?;
        match value {
            Value::Root(root, true) if state.roots.contains_key(root) => Ok(*root),
            _ => Err(failure(
                "protocol operation lacks its live original parameter or owning occurrence",
            )),
        }
    }
    fn change(
        &mut self,
        state: &mut State,
        root: Root,
        phase: u8,
        replace: bool,
        depth: usize,
    ) -> R<()> {
        self.pay(state.work(), depth)?;
        for values in state.names.values_mut() {
            for value in values {
                value.invalidate(root, replace);
            }
        }
        state.returned.invalidate(root, replace);
        for value in &mut state.temporaries {
            value.invalidate(root, replace);
        }
        state.roots.insert(root, phase);
        Ok(())
    }
    fn async_head(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        match ty {
            SemanticTypeRecipe::AsyncRegistration(_) => Ok(true),
            SemanticTypeRecipe::Address { inner, .. }
            | SemanticTypeRecipe::Desc { inner, .. }
            | SemanticTypeRecipe::Managed(inner) => self.async_head(owner, inner, depth + 1),
            SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source,
            } => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    source,
                    self.limits,
                    self.left,
                    depth + 1,
                )?;
                let q = Query::from(source.clone());
                self.pay(q.copy_work().saturating_add(self.proof.types.len()), depth)?;
                let Some(ty) = self.proof.types.get(&(owner, q)) else {
                    return Ok(false);
                };
                let ty = copy_recipe(ty, self.limits, self.left)?;
                self.async_head(owner, &ty, depth + 1)
            }
            // Opaque bodies remain in the defining lexical domain.
            SemanticTypeRecipe::Opaque {
                definition, hidden, ..
            } => self.async_head(*definition, hidden, depth + 1),
            _ => Ok(false),
        }
    }
    fn query_async(&mut self, owner: SemanticDefinitionId, query: Query, depth: usize) -> R<bool> {
        self.pay(
            query.copy_work().saturating_add(self.proof.types.len()),
            depth,
        )?;
        let Some(ty) = self.proof.types.get(&(owner, query)) else {
            return Ok(false);
        };
        let ty = copy_recipe(ty, self.limits, self.left)?;
        self.async_head(owner, &ty, depth + 1)
    }
    fn site(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        depth: usize,
    ) -> R<Option<SemanticSiteId>> {
        let original = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
            return Ok(None);
        };
        self.pay(site_nodes.len(), depth)?;
        let mut matches = site_nodes
            .iter()
            .filter(|r| r.node == SemanticOriginalNode::Expression(node))
            .filter(|r| self.proof.calls.contains_key(&(owner, r.site)));
        let first = matches.next().map(|r| r.site);
        if matches.next().is_some() {
            return Err(failure(
                "original protocol call occurrence has multiple call sites",
            ));
        }
        Ok(first)
    }
    fn output(
        &mut self,
        owner: SemanticDefinitionId,
        state: &State,
        contracts: &[FunctionProtocolContract],
        inputs: &BTreeMap<u32, ProtocolState>,
        depth: usize,
    ) -> R<()> {
        self.pay(
            contracts.iter().fold(state.work(), |w, c| {
                w.saturating_add(c.exits.len().saturating_mul(4))
                    .saturating_add(1)
            }),
            depth,
        )?;
        if contracts.is_empty() {
            return Ok(());
        }
        let outcomes = match &state.returned {
            Value::Poll {
                fresh: true,
                outcomes,
            } => {
                self.pay(state.returned.work(), depth)?;
                outcomes.clone()
            }
            Value::Poll { fresh: false, .. } => {
                return Err(failure(
                    "returned Poll was invalidated by a later operation on its original root",
                ));
            }
            _ => {
                if contracts
                    .iter()
                    .any(|c| c.exits.iter().any(|r| r.exit != ProtocolExit::Return))
                {
                    return Err(pending(
                        "original conditional Error/Poll result lacks its complete tagged source relation",
                    ));
                }
                self.pay(state.roots.len().saturating_add(1), depth)?;
                vec![Outcome {
                    exit: ProtocolExit::Return,
                    phases: state.roots.clone(),
                }]
            }
        };
        for outcome in outcomes {
            for contract in contracts {
                let input = inputs
                    .get(&contract.parameter)
                    .ok_or_else(|| failure("protocol input case is absent"))?;
                let root = Root::Parameter(owner, contract.parameter);
                let actual = outcome
                    .phases
                    .get(&root)
                    .or_else(|| state.roots.get(&root))
                    .copied()
                    .ok_or_else(|| failure("protocol exit lost its original parameter root"))?;
                self.pay(contract.exits.len(), depth)?;
                let row = contract
                    .exits
                    .iter()
                    .find(|r| r.input == *input && r.exit == outcome.exit)
                    .ok_or_else(|| {
                        failure(
                            "reachable original protocol exit is absent from the declared relation",
                        )
                    })?;
                if actual == 0 || actual & !mask(&row.output) != 0 {
                    return Err(failure(
                        "original protocol exit exceeds its declared conditional phase relation",
                    ));
                }
            }
        }
        Ok(())
    }
    fn call(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        arguments: &[SemanticOriginalExpr],
        values: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let Some(site) = self.site(owner, expression.id, depth)? else {
            return self.unknown_call(owner, expression.id, values, state, depth);
        };
        self.pay(self.proof.intrinsics.len(), depth)?;
        if let Some(call) = self.proof.intrinsics.get(&(owner, site)) {
            let work = match &call.kind {
                IntrinsicKind::DynCall {
                    parameter_modes, ..
                } => parameter_modes.len().saturating_add(1),
                IntrinsicKind::RegistrationStart { providers, .. } => {
                    providers.len().saturating_add(1)
                }
                _ => 1,
            };
            self.pay(work, depth)?;
        }
        let intrinsic = self
            .proof
            .intrinsics
            .get(&(owner, site))
            .map(|c| c.kind.clone());
        if let Some(kind) = intrinsic {
            match kind {
                IntrinsicKind::RegistrationStart {
                    asynchronous: true, ..
                } => {
                    let root = Root::Created(owner, expression.id);
                    self.change(state, root, OPEN | CLOSED, true, depth)?;
                    return Ok(Value::Root(root, true));
                }
                IntrinsicKind::RegistrationOperation(
                    op @ (RegistrationOp::AsyncValid
                    | RegistrationOp::AsyncRequest
                    | RegistrationOp::AsyncPollClose
                    | RegistrationOp::AsyncFinish
                    | RegistrationOp::AsyncFinishChecked),
                ) => {
                    let [value] = values else {
                        return Err(failure(
                            "sealed protocol operation changes original argument arity",
                        ));
                    };
                    let root = self.root(value, state, depth)?;
                    let phase = state.roots[&root];
                    if op == RegistrationOp::AsyncValid {
                        return Ok(Value::Valid {
                            root,
                            fresh: true,
                            negated: false,
                        });
                    }
                    if matches!(
                        op,
                        RegistrationOp::AsyncFinish | RegistrationOp::AsyncFinishChecked
                    ) {
                        if phase != CLOSED {
                            return Err(failure(
                                "original registration reclamation is not dominated by Closed for this owning root",
                            ));
                        }
                        self.change(state, root, CLOSED, true, depth)?;
                        return Ok(Value::Plain);
                    }
                    let closing = if phase & (OPEN | CLOSING) != 0 {
                        CLOSING
                    } else {
                        0
                    };
                    if op == RegistrationOp::AsyncRequest {
                        self.change(state, root, closing | (phase & CLOSED), false, depth)?;
                        return Ok(Value::Plain);
                    }
                    self.change(state, root, closing | CLOSED, false, depth)?;
                    self.pay(4, depth)?;
                    let mut outcomes = Vec::new();
                    if closing != 0 {
                        outcomes.push(Outcome {
                            exit: ProtocolExit::PollPending,
                            phases: BTreeMap::from([(root, CLOSING)]),
                        });
                    }
                    outcomes.push(Outcome {
                        exit: ProtocolExit::PollReady,
                        phases: BTreeMap::from([(root, CLOSED)]),
                    });
                    return Ok(Value::Poll {
                        fresh: true,
                        outcomes,
                    });
                }
                // Other sealed primitives do not gain protocol facts. Moving a
                // root through a Task or container requires its separate proof.
                _ => return self.unknown_call(owner, expression.id, values, state, depth),
            }
        }
        self.pay(self.proof.call_targets.len(), depth)?;
        let target = self.proof.call_targets.get(&(owner, site)).copied();
        let Some(target) = target else {
            // An abstract or environment call may reach a captured root even
            // without an explicit resource parameter. It has no neutral-state
            // theorem merely because its value signature is Copy.
            self.pay(state.roots.len(), depth)?;
            let roots: Vec<_> = state.roots.keys().copied().collect();
            for root in roots {
                self.change(state, root, UNKNOWN, false, depth)?;
            }
            return self.unknown_call(owner, expression.id, values, state, depth);
        };
        let callee = self.definition(target)?;
        let SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts,
            parameters,
            ..
        } = &callee.body
        else {
            return self.unknown_call(owner, expression.id, values, state, depth);
        };
        if protocol_contracts.is_empty() {
            return self.unknown_call(owner, expression.id, values, state, depth);
        }
        self.function(target, depth + 1)?;
        if !self.proof.protocol_complete.contains(&target) {
            return Err(failure(
                "callee protocol body is not independently complete",
            ));
        }
        if parameters.len() != arguments.len() || values.len() != arguments.len() {
            return Err(pending(
                "protocol method call requires its full original receiver argument correspondence",
            ));
        }
        self.pay(protocol_contracts.len().saturating_add(values.len()), depth)?;
        let mut roles = Vec::new();
        let mut seen = BTreeSet::new();
        for contract in protocol_contracts {
            let argument = values
                .get(contract.parameter as usize)
                .ok_or_else(|| failure("protocol parameter is not an original call argument"))?;
            let root = self.root(argument, state, depth)?;
            if !seen.insert(root) {
                return Err(failure(
                    "independent protocol roles alias one exclusive original root",
                ));
            }
            let phase = state.roots[&root];
            if phase == 0 || phase & !mask(&contract.entry) != 0 {
                return Err(failure(
                    "caller has not proved the protocol entry state of this actual argument root",
                ));
            }
            roles.push((root, phase, contract));
        }
        self.pay(5usize.saturating_mul(roles.len()), depth)?;
        let mut outcomes = Vec::new();
        for exit in [
            ProtocolExit::Return,
            ProtocolExit::ErrorOk,
            ProtocolExit::ErrorErr,
            ProtocolExit::PollPending,
            ProtocolExit::PollReady,
        ] {
            let mut phases = BTreeMap::new();
            for (root, phase, contract) in &roles {
                let mut output = 0;
                self.pay(contract.exits.len().saturating_mul(4), depth)?;
                for row in &contract.exits {
                    if bit(row.input) & *phase != 0 && row.exit == exit {
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
                outcomes.push(Outcome { exit, phases });
            }
        }
        if outcomes.is_empty() {
            return Err(failure(
                "callee protocol has no reachable result for the proved entry states",
            ));
        }
        for (root, _, _) in &roles {
            let joined = outcomes
                .iter()
                .fold(0, |m, o| m | o.phases.get(root).copied().unwrap_or(0));
            self.change(state, *root, joined, false, depth)?;
        }
        if outcomes.iter().all(|o| o.exit == ProtocolExit::Return) {
            return Ok(Value::Plain);
        }
        if outcomes.iter().any(|o| {
            matches!(
                o.exit,
                ProtocolExit::ErrorOk | ProtocolExit::ErrorErr | ProtocolExit::Return
            )
        }) {
            return Err(pending(
                "original protocol call requires a complete ErrorUnion result correlation",
            ));
        }
        Ok(Value::Poll {
            fresh: true,
            outcomes,
        })
    }
    fn unknown_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        values: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let mut roots = BTreeSet::new();
        for value in values {
            self.pay(value.work(), depth)?;
            value.roots(&mut roots);
        }
        // Missing tables cannot promise absence of a mutation.
        for root in roots {
            self.change(state, root, UNKNOWN, false, depth)?;
        }
        if self.query_async(owner, Query::Expression(node), depth)? {
            let root = Root::Created(owner, node);
            self.change(state, root, UNKNOWN, true, depth)?;
            Ok(Value::Root(root, true))
        } else {
            Ok(Value::Plain)
        }
    }
    fn items(
        &mut self,
        owner: SemanticDefinitionId,
        items: &[SemanticOriginalExpr],
        state: &mut State,
        depth: usize,
    ) -> R<Vec<Value>> {
        self.pay(items.len(), depth)?;
        let start = state.temporaries.len();
        for item in items {
            let value = self.expr(owner, item, state, depth + 1)?;
            state.temporaries.push(value);
        }
        Ok(state.temporaries.drain(start..).collect())
    }
    fn expr(
        &mut self,
        owner: SemanticDefinitionId,
        expr: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        self.pay(1, depth)?;
        match &expr.kind {
            CoreExprKind::Name(name) => {
                self.pay(name.len().saturating_add(state.names.len()), depth)?;
                if let Some(value) = state.names.get(name).and_then(|s| s.last()) {
                    self.value_copy(value, depth)
                } else {
                    Ok(Value::Plain)
                }
            }
            CoreExprKind::Call { callee, arguments } => {
                self.expr(owner, callee, state, depth + 1)?;
                let values = self.items(owner, arguments, state, depth + 1)?;
                self.call(owner, expr, arguments, &values, state, depth + 1)
            }
            CoreExprKind::Unary { op, operand } => {
                let value = self.expr(owner, operand, state, depth + 1)?;
                if *op == UnaryOp::Not {
                    if let Value::Valid {
                        root,
                        fresh,
                        negated,
                    } = value
                    {
                        return Ok(Value::Valid {
                            root,
                            fresh,
                            negated: !negated,
                        });
                    }
                    return Ok(Value::Plain);
                }
                if matches!(
                    op,
                    UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Give | UnaryOp::Dereference
                ) {
                    Ok(value)
                } else {
                    Ok(Value::Plain)
                }
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.expr(owner, value, state, depth + 1)
            }
            CoreExprKind::TypeApply { callee, .. } | CoreExprKind::GenericApply { callee, .. } => {
                self.expr(owner, callee, state, depth + 1)
            }
            CoreExprKind::Tuple(items) | CoreExprKind::Array(items) => {
                Ok(Value::Tuple(self.items(owner, items, state, depth + 1)?))
            }
            CoreExprKind::Record { fields, .. }
            | CoreExprKind::RecordConstructor { fields, .. } => {
                self.pay(fields.len(), depth)?;
                let start = state.temporaries.len();
                for (_, expr) in fields {
                    let value = self.expr(owner, expr, state, depth + 1)?;
                    state.temporaries.push(value);
                }
                let values: Vec<_> = state.temporaries.drain(start..).collect();
                let mut out = BTreeMap::new();
                for ((name, _), value) in fields.iter().zip(values) {
                    self.pay(name.len().saturating_add(1), depth)?;
                    out.insert(name.clone(), value);
                }
                Ok(Value::Fields(out))
            }
            CoreExprKind::Member { object, member, .. } => {
                let value = self.expr(owner, object, state, depth + 1)?;
                match value {
                    Value::Fields(mut fields) => Ok(fields.remove(member).unwrap_or(Value::Plain)),
                    Value::Tuple(mut values) => Ok(member
                        .parse::<usize>()
                        .ok()
                        .filter(|i| *i < values.len())
                        .map(|i| values.remove(i))
                        .unwrap_or(Value::Plain)),
                    _ => Ok(Value::Plain),
                }
            }
            CoreExprKind::Index { object, index } => {
                let value = self.expr(owner, object, state, depth + 1)?;
                self.expr(owner, index, state, depth + 1)?;
                let mut roots = BTreeSet::new();
                self.pay(value.work(), depth)?;
                value.roots(&mut roots);
                if !roots.is_empty() {
                    return Err(pending(
                        "protocol root projection requires its exact original constant index",
                    ));
                }
                Ok(Value::Plain)
            }
            CoreExprKind::Sequence { bindings, value } => {
                let mut old = Vec::new();
                self.pay(bindings.len(), depth)?;
                for (name, expr) in bindings {
                    let value = self.expr(owner, expr, state, depth + 1)?;
                    self.pay(name.len().saturating_add(1), depth)?;
                    old.push((name.clone(), state.names.get(name).map_or(0, Vec::len)));
                    state.names.entry(name.clone()).or_default().push(value);
                }
                let result = self.expr(owner, value, state, depth + 1)?;
                self.restore(state, &old, depth)?;
                Ok(result)
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expr(owner, condition, state, depth + 1)?;
                let mut left = self.copy(state, depth)?;
                let mut right = self.copy(state, depth)?;
                self.condition(&condition, true, &mut left, depth)?;
                self.condition(&condition, false, &mut right, depth)?;
                let a = self.expr(owner, then_value, &mut left, depth + 1)?;
                let b = self.expr(owner, else_value, &mut right, depth + 1)?;
                // Unknown result provenance cannot supply a phase refinement.
                self.pay(a.work().saturating_add(b.work()), depth)?;
                let result = if a == b { a } else { Value::Plain };
                self.merge(state, &left, &right, depth)?;
                Ok(result)
            }
            CoreExprKind::Try(value) => {
                let result = self.expr(owner, value, state, depth + 1)?;
                let definition = self.definition(owner)?;
                if matches!(&definition.body,SemanticDefinitionBody::FunctionWithBody{protocol_contracts,..} if !protocol_contracts.is_empty())
                {
                    return Err(pending(
                        "protocol definition propagation requires its complete error exit and cleanup relation",
                    ));
                }
                if matches!(result, Value::Poll { .. }) {
                    return Err(failure("Poll cannot supply an ErrorUnion protocol edge"));
                }
                // For a caller without output obligations, keeping the normal
                // edge is conservative for subsequent entry checks. Actual CFG
                // checking independently proves cleanup on the error edge.
                Ok(result)
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Range {
                start: left,
                end: right,
            } => {
                self.expr(owner, left, state, depth + 1)?;
                self.expr(owner, right, state, depth + 1)?;
                Ok(Value::Plain)
            }
            CoreExprKind::NumericConvert { value, .. }
            | CoreExprKind::IntegerConvert { value, .. } => {
                self.expr(owner, value, state, depth + 1)?;
                Ok(Value::Plain)
            }
            CoreExprKind::Closure { .. } => {
                // Child bodies are independently universally checked. A captured
                // resource needs a creator/child root boundary, never life Eq.
                if self.query_async(owner, Query::Expression(expr.id), depth)? {
                    return Err(pending(
                        "closure protocol root requires its original capture boundary",
                    ));
                }
                Ok(Value::Plain)
            }
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Character(_)
            | CoreExprKind::Unit => Ok(Value::Plain),
            CoreExprKind::CompileTime(_)
            | CoreExprKind::CompileTimeBlock(_)
            | CoreExprKind::SyntaxQuote { .. } => Ok(Value::Plain),
            _ => Err(pending(
                "original protocol operation requires its complete source evaluation rule",
            )),
        }
    }
    fn restore(&mut self, state: &mut State, old: &[(String, usize)], depth: usize) -> R<()> {
        self.pay(
            old.iter().fold(state.names.len(), |w, (n, _)| {
                w.saturating_add(n.len()).saturating_add(1)
            }),
            depth,
        )?;
        for (name, len) in old.iter().rev() {
            if *len == 0 {
                state.names.remove(name);
            } else if let Some(values) = state.names.get_mut(name) {
                values.truncate(*len);
            }
        }
        Ok(())
    }
    fn condition(&mut self, value: &Value, truth: bool, state: &mut State, depth: usize) -> R<()> {
        self.pay(value.work().saturating_add(state.roots.len()), depth)?;
        if let Value::Valid {
            root,
            fresh: true,
            negated,
        } = value
        {
            if truth == *negated {
                let phase = state.roots.get_mut(root).ok_or_else(|| {
                    failure("validity predicate lost its exact original owning root")
                })?;
                *phase &= CLOSED;
            }
        }
        Ok(())
    }
    fn merge(&mut self, output: &mut State, a: &State, b: &State, depth: usize) -> R<()> {
        self.pay(a.work().saturating_add(b.work()), depth)?;
        *output = a.clone();
        for (root, phase) in &b.roots {
            *output.roots.entry(*root).or_default() |= *phase;
        }
        for (name, values) in &mut output.names {
            if let Some(other) = b.names.get(name) {
                if values.len() != other.len() {
                    return Err(failure("protocol lexical merge changes binder scope"));
                }
                for (value, other) in values.iter_mut().zip(other) {
                    if value != other {
                        *value = Value::Plain;
                    }
                }
            } else {
                for value in values {
                    *value = Value::Plain;
                }
            }
        }
        for (value, other) in output.temporaries.iter_mut().zip(&b.temporaries) {
            if value != other {
                *value = Value::Plain;
            }
        }
        Ok(())
    }
    fn pattern(
        &mut self,
        pattern: &SemanticOriginalPattern,
        value: Value,
        state: &mut State,
        depth: usize,
    ) -> R<()> {
        self.pay(value.work().saturating_add(1), depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.pay(name.len().saturating_add(1), depth)?;
                state.names.entry(name.clone()).or_default().push(value);
            }
            CorePatternKind::Wildcard
            | CorePatternKind::Integer(_)
            | CorePatternKind::Bool(_)
            | CorePatternKind::Float(_)
            | CorePatternKind::String(_)
            | CorePatternKind::Character(_) => {}
            CorePatternKind::Reference { pattern, .. } => {
                self.pattern(pattern, value, state, depth + 1)?
            }
            CorePatternKind::Tuple(patterns) => {
                let values = if let Value::Tuple(values) = value {
                    values
                } else {
                    self.pay(patterns.len(), depth)?;
                    vec![Value::Plain; patterns.len()]
                };
                if patterns.len() != values.len() {
                    return Err(failure("protocol tuple binding changes original arity"));
                }
                for (pattern, value) in patterns.iter().zip(values) {
                    self.pattern(pattern, value, state, depth + 1)?;
                }
            }
            CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                for pattern in fields {
                    self.pattern(pattern, Value::Plain, state, depth + 1)?;
                }
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                let mut values = if let Value::Fields(values) = value {
                    values
                } else {
                    BTreeMap::new()
                };
                for (name, pattern) in fields {
                    self.pattern(
                        pattern,
                        values.remove(name).unwrap_or(Value::Plain),
                        state,
                        depth + 1,
                    )?;
                }
            }
            CorePatternKind::ErrorArm { .. } => {
                return Err(pending(
                    "protocol ErrorUnion pattern needs its complete owning payload relation",
                ));
            }
        }
        Ok(())
    }
    fn selected(
        &mut self,
        owner: SemanticDefinitionId,
        pattern: &SemanticOriginalPattern,
        value: &Value,
        state: &State,
        depth: usize,
    ) -> R<Vec<State>> {
        self.pay(value.work(), depth)?;
        if let Value::Poll { fresh, outcomes } = value {
            if !fresh {
                return Err(failure(
                    "a stale original Poll cannot refine a later generation of its root",
                ));
            }
            let variant = match &pattern.kind {
                CorePatternKind::Variant { variant, .. }
                | CorePatternKind::Constructor { variant, .. } => Some(variant.as_str()),
                CorePatternKind::Name(_) | CorePatternKind::Wildcard => None,
                _ => {
                    return Err(failure(
                        "protocol Poll tag selection is not a nominal constructor pattern",
                    ));
                }
            };
            // Γ already proved the exact subject nominal and variant. These
            // spellings select its closed Poll arms; they grant no type identity.
            let wanted = match variant {
                Some("Pending") => Some(ProtocolExit::PollPending),
                Some("Ready") => Some(ProtocolExit::PollReady),
                None => None,
                _ => {
                    return Err(failure(
                        "original Poll pattern selects a nonexistent sealed arm",
                    ));
                }
            };
            let mut out = Vec::new();
            for outcome in outcomes {
                if wanted.is_some_and(|w| w != outcome.exit) {
                    continue;
                }
                let mut branch = self.copy(state, depth)?;
                for (root, phase) in &outcome.phases {
                    if !branch.roots.contains_key(root) {
                        return Err(failure("Poll refinement lost its original root"));
                    }
                    branch.roots.insert(*root, *phase);
                }
                let bound = self.value_copy(value, depth)?;
                self.pattern(pattern, bound, &mut branch, depth + 1)?;
                out.push(branch);
            }
            Ok(out)
        } else {
            let mut state = self.copy(state, depth)?;
            let value = self.value_copy(value, depth)?;
            self.pattern(pattern, value, &mut state, depth + 1)?;
            let _ = owner;
            Ok(vec![state])
        }
    }
    fn offer(&mut self, seen: &mut Vec<State>, state: State, depth: usize) -> R<bool> {
        for old in seen.iter() {
            self.pay(old.work().saturating_add(state.work()), depth)?;
            if old == &state {
                return Ok(false);
            }
        }
        self.pay(state.work().saturating_add(1), depth)?;
        seen.push(state);
        Ok(true)
    }
    fn loop_body(
        &mut self,
        owner: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        condition: Option<&SemanticOriginalExpr>,
        pattern: Option<&SemanticOriginalPattern>,
        may_exit: bool,
        initial: State,
        depth: usize,
    ) -> R<Vec<State>> {
        let mut seen = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        let mut exits = Vec::new();
        let saved = self.copy(&initial, depth)?;
        self.offer(&mut seen, saved, depth)?;
        queue.push_back(initial);
        while let Some(mut state) = queue.pop_front() {
            self.pay(1, depth)?;
            let scope = self.scope(&state, depth)?;
            let mut entries = Vec::new();
            if let Some(condition) = condition {
                let value = self.expr(owner, condition, &mut state, depth + 1)?;
                // Both condition outcomes are real source paths. A Poll
                // constructor pattern refines only its own matching edge.
                exits.push(self.copy(&state, depth)?);
                if let Some(pattern) = pattern {
                    entries = self.selected(owner, pattern, &value, &state, depth + 1)?;
                } else {
                    entries.push(state);
                }
            } else {
                if may_exit {
                    exits.push(self.copy(&state, depth)?);
                }
                entries.push(state);
            }
            for entry in entries {
                for mut next in self.block(owner, body, entry, depth + 1)? {
                    self.leave(&mut next, &scope, depth)?;
                    match next.control {
                        Control::Break => {
                            next.control = Control::Next;
                            exits.push(next);
                        }
                        Control::Return => exits.push(next),
                        Control::Next | Control::Continue => {
                            next.control = Control::Next;
                            // No fresh κ, epoch counter, or SSA equality is
                            // created at a backedge. Existing stale aliases stay
                            // stale; repeated complete states are the fixed point.
                            let saved = self.copy(&next, depth)?;
                            if self.offer(&mut seen, saved, depth)? {
                                queue.push_back(next);
                            }
                        }
                    }
                }
            }
        }
        Ok(exits)
    }
    fn scope(&mut self, state: &State, depth: usize) -> R<Vec<(String, usize)>> {
        self.pay(
            state
                .names
                .keys()
                .fold(0usize, |w, n| w.saturating_add(n.len()).saturating_add(1)),
            depth,
        )?;
        Ok(state
            .names
            .iter()
            .map(|(name, values)| (name.clone(), values.len()))
            .collect())
    }
    fn leave(&mut self, state: &mut State, scope: &[(String, usize)], depth: usize) -> R<()> {
        self.pay(
            scope
                .iter()
                .fold(state.names.len(), |w, (name, _)| {
                    w.saturating_add(name.len()).saturating_add(1)
                })
                .saturating_mul(state.names.len().saturating_add(1)),
            depth,
        )?;
        state.names.retain(|name, values| {
            if let Some((_, len)) = scope.iter().find(|(n, _)| n == name) {
                values.truncate(*len);
                true
            } else {
                false
            }
        });
        Ok(())
    }
    fn cleanup(
        &mut self,
        owner: SemanticDefinitionId,
        state: State,
        defers: &[&SemanticOriginalBlock],
        scope: &[(String, usize)],
        depth: usize,
    ) -> R<Vec<State>> {
        let control = state.control;
        let mut states = vec![state];
        for cleanup in defers.iter().rev() {
            let mut next = Vec::new();
            for mut state in states {
                state.control = Control::Next;
                for state in self.block(owner, cleanup, state, depth + 1)? {
                    if state.control != Control::Next {
                        return Err(failure(
                            "original protocol cleanup escapes its lexical exit",
                        ));
                    }
                    self.pay(1, depth)?;
                    next.push(state);
                }
            }
            states = next;
        }
        for state in &mut states {
            state.control = control;
            self.leave(state, scope, depth)?;
        }
        Ok(states)
    }
    fn block(
        &mut self,
        owner: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        state: State,
        depth: usize,
    ) -> R<Vec<State>> {
        self.pay(
            body.statements
                .len()
                .saturating_add(state.roots.len())
                .saturating_add(1),
            depth,
        )?;
        if state.roots.values().any(|phase| *phase == 0) {
            return Ok(Vec::new());
        }
        let scope = self.scope(&state, depth)?;
        let mut active = vec![state];
        let mut ended = Vec::new();
        let mut defers = Vec::new();
        for statement in &body.statements {
            if active.is_empty() {
                break;
            }
            if let CoreStatementKind::Defer(cleanup) = &statement.kind {
                self.pay(1, depth)?;
                defers.push(cleanup);
                continue;
            }
            let mut next = Vec::new();
            for state in active {
                for state in self.statement(owner, statement, state, depth + 1)? {
                    self.pay(1, depth)?;
                    if state.control == Control::Next {
                        next.push(state);
                    } else {
                        ended.extend(self.cleanup(owner, state, &defers, &scope, depth + 1)?);
                    }
                }
            }
            active = next;
        }
        for state in active {
            ended.extend(self.cleanup(owner, state, &defers, &scope, depth + 1)?);
        }
        Ok(ended)
    }
    fn statement(
        &mut self,
        owner: SemanticDefinitionId,
        statement: &SemanticOriginalStatement,
        mut state: State,
        depth: usize,
    ) -> R<Vec<State>> {
        self.pay(1, depth)?;
        match &statement.kind {
            CoreStatementKind::Binding {
                name, initializer, ..
            } => {
                let value = if let Some(expr) = initializer {
                    self.expr(owner, expr, &mut state, depth + 1)?
                } else {
                    Value::Plain
                };
                self.pay(name.len().saturating_add(1), depth)?;
                state.names.entry(name.clone()).or_default().push(value);
            }
            CoreStatementKind::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                let value = self.expr(owner, initializer, &mut state, depth + 1)?;
                self.pattern(pattern, value, &mut state, depth + 1)?;
            }
            CoreStatementKind::Return(value) => {
                state.returned = if let Some(expr) = value {
                    self.expr(owner, expr, &mut state, depth + 1)?
                } else {
                    Value::Plain
                };
                state.control = Control::Return;
            }
            CoreStatementKind::Expression(expr) => {
                self.expr(owner, expr, &mut state, depth + 1)?;
            }
            CoreStatementKind::Assign { target, value }
            | CoreStatementKind::CompoundAssign { target, value, .. } => {
                let value = self.expr(owner, value, &mut state, depth + 1)?;
                if let CoreExprKind::Name(name) = &target.kind {
                    self.pay(state.names.len().saturating_add(name.len()), depth)?;
                    let Some(binding) = state.names.get_mut(name).and_then(|s| s.last_mut()) else {
                        return Err(failure(
                            "protocol assignment has no original lexical binding",
                        ));
                    };
                    *binding = value;
                } else {
                    let target = self.expr(owner, target, &mut state, depth + 1)?;
                    let mut roots = BTreeSet::new();
                    self.pay(target.work().saturating_add(value.work()), depth)?;
                    target.roots(&mut roots);
                    value.roots(&mut roots);
                    if !roots.is_empty() {
                        return Err(pending(
                            "protocol field assignment requires its exact owning projection boundary",
                        ));
                    }
                }
            }
            CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                return self.block(owner, body, state, depth + 1);
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let condition = self.expr(owner, condition, &mut state, depth + 1)?;
                let mut right = self.copy(&state, depth)?;
                self.condition(&condition, true, &mut state, depth)?;
                self.condition(&condition, false, &mut right, depth)?;
                let mut out = self.block(owner, then_body, state, depth + 1)?;
                if let Some(body) = else_body {
                    out.extend(self.block(owner, body, right, depth + 1)?);
                } else {
                    out.push(right);
                }
                return Ok(out);
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                let value = self.expr(owner, value, &mut state, depth + 1)?;
                let scope = self.scope(&state, depth)?;
                let mut out = Vec::new();
                for branch in self.selected(owner, pattern, &value, &state, depth + 1)? {
                    for mut branch in self.block(owner, then_body, branch, depth + 1)? {
                        self.leave(&mut branch, &scope, depth)?;
                        out.push(branch);
                    }
                }
                // The complement remains an overapproximation. It receives no
                // successful tag's phase facts.
                if let Some(body) = else_body {
                    out.extend(self.block(owner, body, state, depth + 1)?);
                } else {
                    out.push(state);
                }
                return Ok(out);
            }
            CoreStatementKind::Match { value, arms } => {
                let value = self.expr(owner, value, &mut state, depth + 1)?;
                let scope = self.scope(&state, depth)?;
                self.pay(arms.len(), depth)?;
                let mut out = Vec::new();
                for arm in arms {
                    for branch in self.selected(owner, &arm.pattern, &value, &state, depth + 1)? {
                        for mut branch in self.block(owner, &arm.body, branch, depth + 1)? {
                            self.leave(&mut branch, &scope, depth)?;
                            out.push(branch);
                        }
                    }
                }
                return Ok(out);
            }
            CoreStatementKind::Loop(body) => {
                return self.loop_body(owner, body, None, None, false, state, depth + 1);
            }
            CoreStatementKind::While { condition, body } => {
                return self.loop_body(owner, body, Some(condition), None, false, state, depth + 1);
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => {
                return self.loop_body(
                    owner,
                    body,
                    Some(value),
                    Some(pattern),
                    false,
                    state,
                    depth + 1,
                );
            }
            CoreStatementKind::ForRange {
                start, end, body, ..
            } => {
                self.expr(owner, start, &mut state, depth + 1)?;
                self.expr(owner, end, &mut state, depth + 1)?;
                // Zero and arbitrary finite iterations are both included.
                let zero = self.copy(&state, depth)?;
                let mut out = self.loop_body(owner, body, None, None, true, state, depth + 1)?;
                out.push(zero);
                return Ok(out);
            }
            CoreStatementKind::With { name, value, body } => {
                let value = self.expr(owner, value, &mut state, depth + 1)?;
                let scope = self.scope(&state, depth)?;
                self.pay(name.len().saturating_add(1), depth)?;
                state.names.entry(name.clone()).or_default().push(value);
                let mut out = self.block(owner, body, state, depth + 1)?;
                for state in &mut out {
                    self.leave(state, &scope, depth)?;
                }
                return Ok(out);
            }
            CoreStatementKind::Break => state.control = Control::Break,
            CoreStatementKind::Continue => state.control = Control::Continue,
            CoreStatementKind::CompileTime(_) => {}
            CoreStatementKind::ForEach { .. }
            | CoreStatementKind::Yield(_)
            | CoreStatementKind::Unsupported(_) => {
                return Err(pending(
                    "original protocol control form requires its complete CFG relation",
                ));
            }
            CoreStatementKind::Defer(_) => unreachable!(),
        }
        Ok(vec![state])
    }
    fn function(&mut self, owner: SemanticDefinitionId, depth: usize) -> R<()> {
        self.pay(
            self.proof
                .protocol_complete
                .len()
                .saturating_add(self.active.len())
                .saturating_add(1),
            depth,
        )?;
        if self.proof.protocol_complete.contains(&owner) {
            return Ok(());
        }
        if !self.proof.complete.contains(&owner) {
            return Err(failure(
                "protocol definition lacks a complete independent original type proof",
            ));
        }
        if !self.active.insert(owner) {
            return Err(pending(
                "recursive protocol contracts require a jointly proved conditional fixed point",
            ));
        }
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts,
            parameters,
            body,
            ..
        } = &definition.body
        else {
            self.active.remove(&owner);
            self.proof.protocol_complete.insert(owner);
            return Ok(());
        };
        self.pay(
            parameters.len().saturating_add(protocol_contracts.len()),
            depth,
        )?;
        let mut roles = BTreeSet::new();
        for contract in protocol_contracts {
            crate::registration::charge_contract(contract, |work| self.pay(work, depth))?;
            let parameter = contract.parameter as usize;
            if parameter >= parameters.len()
                || !roles.insert(contract.parameter)
                || contract.entry.is_empty()
            {
                return Err(failure(
                    "original protocol table has an absent, duplicate, or empty parameter role",
                ));
            }
            if !self.query_async(owner, Query::Parameter(contract.parameter), depth + 1)? {
                return Err(failure(
                    "original protocol role is not a genuine AsyncRegistration parameter",
                ));
            }
            let mut entry = BTreeSet::new();
            for input in &contract.entry {
                if !entry.insert(*input) {
                    return Err(failure("original protocol entry state is duplicated"));
                }
            }
            let mut exits = BTreeSet::new();
            for row in &contract.exits {
                if !entry.contains(&row.input)
                    || !exits.insert((row.input, row.exit))
                    || row.output.is_empty()
                {
                    return Err(failure(
                        "original protocol relation has an invalid input, duplicate exit, or empty output",
                    ));
                }
                let mut output = BTreeSet::new();
                for phase in &row.output {
                    if !output.insert(*phase) {
                        return Err(failure("original protocol output state is duplicated"));
                    }
                }
            }
        }
        let SemanticOriginalFunctionBody::Defined(body) = body else {
            if !protocol_contracts.is_empty() {
                return Err(pending(
                    "bodyless original protocol relation has no independent implementation proof",
                ));
            }
            self.active.remove(&owner);
            self.proof.protocol_complete.insert(owner);
            return Ok(());
        };
        if !self.relevant.contains(&owner) {
            self.active.remove(&owner);
            self.proof.protocol_complete.insert(owner);
            return Ok(());
        }
        let mut cases = vec![BTreeMap::new()];
        for contract in protocol_contracts {
            let mut next = Vec::new();
            for case in cases {
                for input in &contract.entry {
                    self.pay(case.len().saturating_add(1), depth)?;
                    let mut case = case.clone();
                    case.insert(contract.parameter, *input);
                    next.push(case);
                }
            }
            cases = next;
        }
        for inputs in cases {
            let mut state = State {
                names: BTreeMap::new(),
                roots: BTreeMap::new(),
                control: Control::Next,
                returned: Value::Plain,
                temporaries: Vec::new(),
            };
            for (index, parameter) in parameters.iter().enumerate() {
                self.pay(parameter.name.len().saturating_add(1), depth)?;
                let value = if self.query_async(owner, Query::Parameter(index as u32), depth + 1)? {
                    let root = Root::Parameter(owner, index as u32);
                    state.roots.insert(
                        root,
                        inputs
                            .get(&(index as u32))
                            .copied()
                            .map(bit)
                            .unwrap_or(UNKNOWN),
                    );
                    Value::Root(root, true)
                } else {
                    Value::Plain
                };
                state.names.insert(parameter.name.clone(), vec![value]);
            }
            for state in self.block(owner, body, state, depth + 1)? {
                if matches!(state.control, Control::Break | Control::Continue) {
                    return Err(failure(
                        "original protocol loop control escaped its function",
                    ));
                }
                self.output(owner, &state, protocol_contracts, &inputs, depth + 1)?;
            }
        }
        self.active.remove(&owner);
        self.proof.protocol_complete.insert(owner);
        Ok(())
    }
}

pub(super) fn check(
    program: &SemanticModule,
    proof: &mut Proof,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut engine = Engine {
        program,
        proof,
        limits,
        left,
        active: BTreeSet::new(),
        relevant: BTreeSet::new(),
    };
    engine.pay(
        program
            .definitions
            .len()
            .saturating_add(engine.proof.intrinsics.len()),
        0,
    )?;
    for definition in &program.definitions {
        if matches!(&definition.body,SemanticDefinitionBody::FunctionWithBody{protocol_contracts,..}if !protocol_contracts.is_empty())
        {
            engine.relevant.insert(definition.id);
        }
    }
    for ((owner, _), intrinsic) in &engine.proof.intrinsics {
        if matches!(
            intrinsic.kind,
            IntrinsicKind::RegistrationStart {
                asynchronous: true,
                ..
            } | IntrinsicKind::RegistrationOperation(
                RegistrationOp::AsyncValid
                    | RegistrationOp::AsyncRequest
                    | RegistrationOp::AsyncPollClose
                    | RegistrationOp::AsyncFinish
                    | RegistrationOp::AsyncFinishChecked
            )
        ) {
            engine.relevant.insert(*owner);
        }
    }
    loop {
        engine.pay(
            engine
                .proof
                .call_targets
                .len()
                .saturating_add(engine.relevant.len()),
            0,
        )?;
        let mut changed = false;
        for ((caller, site), callee) in &engine.proof.call_targets {
            if !engine.proof.compile_time_calls.contains(&(*caller, *site))
                && engine.relevant.contains(callee)
            {
                changed |= engine.relevant.insert(*caller);
            }
        }
        if !changed {
            break;
        }
    }
    for definition in &program.definitions {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) {
            engine.function(definition.id, 0)?;
        }
    }
    Ok(())
}

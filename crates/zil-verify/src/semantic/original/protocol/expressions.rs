use super::*;

impl Engine<'_> {
    pub(super) fn expression(
        &mut self,
        node: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
        b: &mut Budget,
    ) -> Check<Value> {
        self.depth(depth, b)?;
        let mut value = self.expression_inner(node, state, depth, b)?;
        if let Some(ty) = self.original.expressions.get(&(self.current, node.id)) {
            let ty = self.original.resolved(ty, b)?;
            if let Some(borrowed) = owner_type(&ty) {
                match &mut value {
                    Value::Owner {
                        borrowed: actual, ..
                    } => *actual |= borrowed,
                    Value::Fresh | Value::FreshTask if !borrowed => {}
                    _ => {
                        return Err(unsupported(
                            "original owner expression has no exact initialized slot or proved return provenance",
                            Some(node.span),
                        ));
                    }
                }
            }
        }
        Ok(value)
    }
    fn expression_inner(
        &mut self,
        node: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
        b: &mut Budget,
    ) -> Check<Value> {
        use CoreExprKind::*;
        match &node.kind {
            Name(_) => {
                let Some(binder) = self.original.value_bindings.get(&(self.current, node.id))
                else {
                    return Ok(Value::Plain);
                };
                b.spend(state.width())?;
                let value = state.values.get(binder).cloned().unwrap_or(Value::Plain);
                if let Value::Owner { root, .. } = &value {
                    state.owner(root, b)?;
                }
                Ok(value)
            }
            Group(inner) | Ascribed { value: inner, .. } | CompileTime(inner) => {
                self.expression(inner, state, depth + 1, b)
            }
            Unary { op, operand } => {
                let value = self.expression(operand, state, depth + 1, b)?;
                match (op, value) {
                    (UnaryOp::Borrow | UnaryOp::BorrowMut, value @ Value::Product(_)) => {
                        Ok(tasks::task_borrow_value(value))
                    }
                    (UnaryOp::Give, value @ Value::Product(_)) => {
                        if let Some(place) = self.task_place(operand, b)? {
                            self.task_take_place(state, &place, b)?;
                        }
                        Ok(value)
                    }
                    (UnaryOp::Borrow | UnaryOp::BorrowMut, Value::Owner { root, .. }) => {
                        Ok(Value::Owner {
                            root,
                            borrowed: true,
                        })
                    }
                    (
                        UnaryOp::Give,
                        Value::Owner {
                            root,
                            borrowed: false,
                        },
                    ) => {
                        if let Some(place) = self.task_place(operand, b)? {
                            self.task_take_place(state, &place, b)?;
                        }
                        Ok(Value::Owner {
                            root,
                            borrowed: false,
                        })
                    }
                    (UnaryOp::Dereference, Value::Owner { root, .. }) => Ok(Value::Owner {
                        root,
                        borrowed: false,
                    }),
                    (_, Value::Plain) => Ok(Value::Plain),
                    (_, value @ Value::Poll { .. })
                        if matches!(op, UnaryOp::Borrow | UnaryOp::BorrowMut) =>
                    {
                        Ok(value)
                    }
                    _ => Err(unsupported(
                        "original protocol value passes through an unsupported unary role",
                        Some(node.span),
                    )),
                }
            }
            Call { callee, arguments } => {
                self.expression(callee, state, depth + 1, b)?;
                let mut values = Vec::new();
                for argument in arguments {
                    values.push(self.expression(argument, state, depth + 1, b)?);
                }
                self.call(node, &values, state, b)
            }
            TypeApply { callee, .. } | GenericApply { callee, .. } => {
                self.expression(callee, state, depth + 1, b)
            }
            Binary {
                op: BinaryOp::And | BinaryOp::Or,
                left,
                right,
            } => {
                self.expression(left, state, depth + 1, b)?;
                let before = state.copy(b)?;
                self.expression(right, state, depth + 1, b)?;
                *state = self.join(&before, state, b)?;
                Ok(Value::Plain)
            }
            Binary { left, right, .. }
            | Range {
                start: left,
                end: right,
            }
            | Index {
                object: left,
                index: right,
            }
            | Pipeline {
                input: left,
                target: right,
            } => {
                let a = self.expression(left, state, depth + 1, b)?;
                let c = self.expression(right, state, depth + 1, b)?;
                if tasks::owner_value(&a) || tasks::owner_value(&c) {
                    return Err(unsupported(
                        "original registration projection needs its complete owner path ledger",
                        Some(node.span),
                    ));
                }
                Ok(Value::Plain)
            }
            NumericConvert { value, .. }
            | IntegerConvert { value, .. }
            | Spread(value)
            | Splice(value) => {
                self.expression(value, state, depth + 1, b)?;
                Ok(Value::Plain)
            }
            Try(value) => {
                let value = self.expression(value, state, depth + 1, b)?;
                if let Value::FallibleTaskOwned(inputs) = &value {
                    let mut failed = state.copy(b)?;
                    for input in inputs {
                        self.task_transfer_value(&mut failed, input, b)?;
                    }
                    let pending = self.error_unwind.clone();
                    let failed = self.unwind(failed, &pending, 0, depth + 1, b)?;
                    self.error_unwind = pending;
                    self.check_exit(failed, Value::Plain, b)?;
                    for input in inputs {
                        self.task_transfer_value(state, input, b)?;
                    }
                    return Ok(Value::FreshTask);
                }
                if value == Value::FallibleTask {
                    b.spend(self.error_unwind.len())?;
                    let deferred = self.error_unwind.clone();
                    let error = self.unwind(state.copy(b)?, &deferred, 0, depth + 1, b)?;
                    self.error_unwind = deferred;
                    self.check_exit(error, Value::Plain, b)?;
                    return Ok(Value::FreshTask);
                }
                if !matches!(value, Value::Plain) {
                    return Err(unsupported(
                        "original protocol propagation needs its full error-exit cleanup relation",
                        Some(node.span),
                    ));
                }
                Ok(Value::Plain)
            }
            If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, state, depth + 1, b)?;
                let mut yes = state.copy(b)?;
                let a = self.expression(then_value, &mut yes, depth + 1, b)?;
                let mut no = state.copy(b)?;
                let c = self.expression(else_value, &mut no, depth + 1, b)?;
                *state = self.join(&yes, &no, b)?;
                union_value(&a, &c, b)
            }
            Tuple(fields) | Array(fields) => {
                let array = matches!(node.kind, Array(_));
                let mut values = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    let value = self.expression(field, state, depth + 1, b)?;
                    values.push((
                        if array {
                            format!("[{index}]")
                        } else {
                            index.to_string()
                        },
                        value,
                    ));
                }
                Ok(Value::Product(values))
            }
            Record { fields, .. } | RecordConstructor { fields, .. } => {
                let mut values = Vec::new();
                for (name, field) in fields {
                    values.push((name.clone(), self.expression(field, state, depth + 1, b)?));
                }
                Ok(Value::Product(values))
            }
            RecordUpdate { base, fields, .. } => {
                let base = self.expression(base, state, depth + 1, b)?;
                let Value::Product(mut values) = base else {
                    return Ok(Value::Plain);
                };
                for (name, field) in fields {
                    let value = self.expression(field, state, depth + 1, b)?;
                    let slot = values
                        .iter_mut()
                        .find(|(key, _)| key == name)
                        .ok_or_else(|| {
                            bad("Task record update changes its original product field")
                        })?;
                    self.task_cleanup_value(state, &slot.1, Some(&value), b)?;
                    slot.1 = value;
                }
                Ok(Value::Product(values))
            }
            Member { object, member, .. } => {
                let value = self.expression(object, state, depth + 1, b)?;
                match value {
                    Value::Product(fields) => fields
                        .into_iter()
                        .find(|(name, _)| name == member)
                        .map(|(_, value)| value)
                        .ok_or_else(|| {
                            bad("Task projected read changes its real original product field")
                        }),
                    value => {
                        self.reject_packed_owner(&value, node)?;
                        Ok(Value::Plain)
                    }
                }
            }
            Slice { object, start, end } => {
                let value = self.expression(object, state, depth + 1, b)?;
                self.reject_packed_owner(&value, node)?;
                for part in [start, end].into_iter().flatten() {
                    self.expression(part, state, depth + 1, b)?;
                }
                Ok(Value::Plain)
            }
            Sequence { bindings, value } => {
                for (name, initializer) in bindings {
                    let actual = self.expression(initializer, state, depth + 1, b)?;
                    self.bind(
                        state,
                        Binder::Local {
                            definition: self.current,
                            node: initializer.id,
                            name: name.clone(),
                        },
                        actual,
                        b,
                    )?;
                }
                self.expression(value, state, depth + 1, b)
            }
            Closure { .. } => {
                let child = self
                    .function()?
                    .closures
                    .get(&node.id)
                    .ok_or_else(|| bad("protocol closure has no exact original creator"))?;
                self.task_capture(*child, state, b)?;
                if let Some(free) = self.original.free_values.get(child) {
                    for binding in free.values() {
                        b.spend(1)?;
                        if contains_owner(&binding.ty) && !tasks::is_task(&binding.ty) {
                            return Err(unsupported(
                                "original closure captures a registration without a complete owner generation seal",
                                Some(node.span),
                            ));
                        }
                    }
                }
                Ok(Value::Plain)
            }
            MakeDynBorrow { source, .. } | MakeDynOwned { source, .. } => {
                let value = self.expression(source, state, depth + 1, b)?;
                self.reject_packed_owner(&value, node)?;
                Ok(Value::Plain)
            }
            CallDynSlot {
                callee, arguments, ..
            } => {
                let value = self.expression(callee, state, depth + 1, b)?;
                self.reject_packed_owner(&value, node)?;
                for argument in arguments {
                    let value = self.expression(argument, state, depth + 1, b)?;
                    self.reject_packed_owner(&value, node)?;
                }
                Ok(Value::Plain)
            }
            Literal(_) | Integer(_) | Float(_) | Bool(_) | String(_) | Unit | Character(_)
            | ReceiverMember(_) => Ok(Value::Plain),
            _ => Err(unsupported(
                "original protocol expression lacks a complete control/owner rule",
                Some(node.span),
            )),
        }
    }
    fn reject_packed_owner(&self, value: &Value, node: &SemanticOriginalExpr) -> Check<()> {
        if tasks::owner_value(value) {
            Err(unsupported(
                "original aggregate registration needs its complete projected owner/epoch ledger",
                Some(node.span),
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn binder(
        &self,
        node: &SemanticOriginalExpr,
        b: &mut Budget,
    ) -> Check<Option<Binder>> {
        let mut node = node;
        loop {
            b.spend(1)?;
            match &node.kind {
                CoreExprKind::Group(inner)
                | CoreExprKind::Ascribed { value: inner, .. }
                | CoreExprKind::Unary {
                    op: UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Give,
                    operand: inner,
                } => node = inner,
                _ => {
                    b.spend(1)?;
                    return Ok(self
                        .original
                        .value_bindings
                        .get(&(self.current, node.id))
                        .cloned());
                }
            }
        }
    }
    fn site(&self, node: &SemanticOriginalExpr, b: &mut Budget) -> Check<Option<SemanticSiteId>> {
        let function = self.function()?;
        b.spend(function.nodes.len())?;
        let mut selected = None;
        for (&site, source) in &function.nodes {
            if matches!(source,index::Node::Expression(expression)if expression.id==node.id)
                && (self
                    .original
                    .intrinsic_calls
                    .contains_key(&(self.current, site))
                    || self
                        .original
                        .call_targets
                        .contains_key(&(self.current, site)))
            {
                if selected.replace(site).is_some() {
                    return Err(bad(
                        "original protocol call has multiple exact source sites",
                    ));
                }
            }
        }
        Ok(selected)
    }
    fn call(
        &mut self,
        node: &SemanticOriginalExpr,
        values: &[Value],
        state: &mut State,
        b: &mut Budget,
    ) -> Check<Value> {
        let site = self.site(node, b)?;
        if let Some(use_) =
            site.and_then(|site| self.original.intrinsic_calls.get(&(self.current, site)))
        {
            if let Some(value) = self.task_call(use_.kind, values, state, b)? {
                return Ok(value);
            }
            match use_.kind {
                Intrinsic::AsyncStart => return Ok(Value::Fresh),
                Intrinsic::AsyncValid
                | Intrinsic::AsyncRequest
                | Intrinsic::AsyncPoll
                | Intrinsic::AsyncFinish
                | Intrinsic::AsyncFinishChecked => {
                    let [Value::Owner { root, .. }] = values else {
                        return Err(bad(
                            "original registration operation selects no exact owner slot",
                        ));
                    };
                    let old = state.owner(root, b)?.states;
                    return match use_.kind {
                        Intrinsic::AsyncValid => Ok(Value::Plain),
                        Intrinsic::AsyncRequest => {
                            let next = if old & CLOSED != 0 { CLOSED } else { 0 }
                                | if old & (OPEN | CLOSING) != 0 {
                                    CLOSING
                                } else {
                                    0
                                };
                            self.update(state, root, next, b)?;
                            Ok(Value::Plain)
                        }
                        Intrinsic::AsyncPoll => {
                            let pending = if old & (OPEN | CLOSING) != 0 {
                                CLOSING
                            } else {
                                0
                            };
                            let ready = CLOSED;
                            let generation = self.update(state, root, pending | ready, b)?;
                            Ok(Value::Poll {
                                relations: vec![Conditional {
                                    root: root.clone(),
                                    generation,
                                    pending,
                                    ready,
                                }],
                                tag: None,
                            })
                        }
                        Intrinsic::AsyncFinish | Intrinsic::AsyncFinishChecked => {
                            if use_.kind == Intrinsic::AsyncFinishChecked && old != CLOSED {
                                return Err(bad(
                                    "original checked registration destruction lacks current-generation Closed proof",
                                ));
                            }
                            self.update(state, root, DEAD, b)?;
                            Ok(Value::Plain)
                        }
                        _ => unreachable!(),
                    };
                }
                _ => {
                    for value in values {
                        if let Value::Owner { root, .. } = value {
                            self.update(state, root, LIVE, b)?;
                        }
                    }
                    return Ok(Value::Plain);
                }
            }
        }
        if let Some((site, target)) = site.and_then(|site| {
            self.original
                .call_targets
                .get(&(self.current, site))
                .map(|target| (site, *target))
        }) {
            return self.static_call(target, site, node, values, state, b);
        }
        // A callable's hidden environment is not a declared parameter. Without
        // its exact owner transport, no old observation survives the call.
        b.spend(state.width())?;
        let roots = state.owners.keys().cloned().collect::<Vec<_>>();
        for root in roots {
            if state
                .owners
                .get(&root)
                .is_some_and(|owner| owner.states & DEAD == 0)
            {
                self.update(state, &root, LIVE, b)?;
            }
        }
        self.result_value(node, b)
    }
    fn result_value(&self, node: &SemanticOriginalExpr, b: &mut Budget) -> Check<Value> {
        let Some(ty) = self.original.expressions.get(&(self.current, node.id)) else {
            return Ok(Value::Plain);
        };
        let ty = self.original.resolved(ty, b)?;
        match owner_type(&ty) {
            Some(false) => Ok(if tasks::is_task(&ty) {
                Value::FreshTask
            } else {
                Value::Fresh
            }),
            Some(true) => Err(unsupported(
                "original helper borrowed registration result requires its exact source role",
                Some(node.span),
            )),
            None => Ok(Value::Plain),
        }
    }
    fn static_call(
        &mut self,
        target: SemanticDefinitionId,
        site: SemanticSiteId,
        node: &SemanticOriginalExpr,
        values: &[Value],
        state: &mut State,
        b: &mut Budget,
    ) -> Check<Value> {
        let function = self.index.definitions.get(&target).ok_or_else(|| {
            bad("original protocol callee is outside its complete definition inventory")
        })?;
        let SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts, ..
        } = &function.definition.body
        else {
            return Err(bad("original protocol callee has no complete head"));
        };
        b.spend(
            values
                .len()
                .saturating_mul(protocol_contracts.len().saturating_add(1))
                .saturating_add(state.width()),
        )?;
        let poll = self
            .original
            .expressions
            .get(&(self.current, node.id))
            .map(|ty| self.poll_type(ty, b))
            .transpose()?
            .unwrap_or(false);
        let mut pending = Vec::new();
        let mut seen = BTreeSet::new();
        // Read every actual operand from the same pre-call state before any
        // owner is updated. Two formal roles cannot strong-update one owner.
        for (ordinal, value) in values.iter().enumerate() {
            if let Value::Product(_) = value {
                if protocol_contracts
                    .iter()
                    .any(|role| role.parameter as usize == ordinal)
                {
                    return Err(bad(
                        "registration protocol role cannot select an aggregate Task source",
                    ));
                }
                let mut leaves = Vec::new();
                tasks::task_roots(value, &mut leaves, b)?;
                for root in leaves {
                    if !seen.insert(root.clone()) {
                        return Err(bad(
                            "Task call aliases independently selected owner parameter roles",
                        ));
                    }
                    let owner = state.owner(&root, b)?;
                    if !owner.task {
                        return Err(unsupported(
                            "aggregate registration call lacks its exact original protocol roles",
                            None,
                        ));
                    }
                    pending.push((
                        root,
                        if self.observing_parameter(target, site, ordinal, b)? {
                            LIVE
                        } else {
                            DEAD
                        },
                        None,
                    ));
                }
                continue;
            }
            let Value::Owner { root, .. } = value else {
                if protocol_contracts
                    .iter()
                    .any(|role| role.parameter as usize == ordinal)
                {
                    return Err(bad("original protocol actual has no initialized owner"));
                }
                continue;
            };
            if !seen.insert(root.clone()) {
                return Err(bad(
                    "original protocol call aliases distinct parameter roles",
                ));
            }
            let old = state.owner(root, b)?;
            let role = protocol_contracts
                .iter()
                .find(|role| role.parameter as usize == ordinal);
            if let Some(role) = role {
                if old.states & !mask(&role.entry) != 0 {
                    return Err(bad(
                        "original caller owner state does not satisfy the callee entry relation",
                    ));
                }
                let mut plain = 0;
                let mut low = 0;
                let mut high = 0;
                for row in &role.exits {
                    b.spend(row.output.len().saturating_add(1))?;
                    if old.states & bit(row.input) == 0 {
                        continue;
                    }
                    let output = if row.output.is_empty() {
                        DEAD
                    } else {
                        mask(&row.output)
                    };
                    match row.exit {
                        ProtocolExit::Return => plain |= output,
                        ProtocolExit::PollPending => low |= output,
                        ProtocolExit::PollReady => high |= output,
                        _ => {
                            return Err(unsupported(
                                "original protocol error-arm call needs its complete correlation",
                                Some(node.span),
                            ));
                        }
                    }
                }
                let all = plain | low | high;
                if all == 0 {
                    return Err(bad(
                        "original protocol call relation has no possible actual exit",
                    ));
                }
                pending.push((
                    root.clone(),
                    all,
                    if poll { Some((low, high)) } else { None },
                ));
            } else {
                pending.push((
                    root.clone(),
                    if self.observing_parameter(target, site, ordinal, b)? {
                        LIVE
                    } else {
                        DEAD
                    },
                    None,
                ));
            }
        }
        let mut relations = Vec::new();
        for (root, states, conditional) in pending {
            let generation = self.update(state, &root, states, b)?;
            if let Some((pending, ready)) = conditional {
                relations.push(Conditional {
                    root,
                    generation,
                    pending,
                    ready,
                });
            }
        }
        if poll && !relations.is_empty() {
            Ok(Value::Poll {
                relations,
                tag: None,
            })
        } else {
            self.result_value(node, b)
        }
    }
    fn observing_parameter(
        &self,
        target: SemanticDefinitionId,
        site: SemanticSiteId,
        ordinal: usize,
        b: &mut Budget,
    ) -> Check<bool> {
        let prototype = self
            .original
            .prototypes
            .get(&target)
            .ok_or_else(|| bad("original owner call has no complete callee prototype"))?;
        let mode = *prototype
            .modes
            .get(ordinal)
            .ok_or_else(|| bad("original owner call parameter mode is missing"))?;
        if matches!(
            mode,
            PassingMode::Borrow
                | PassingMode::BorrowMut
                | PassingMode::Describe
                | PassingMode::Copy
        ) {
            return Ok(true);
        }
        let mut parameter = self.original.resolved(
            prototype
                .parameters
                .get(ordinal)
                .ok_or_else(|| bad("original owner call parameter is missing"))?,
            b,
        )?;
        if let Ty::Variable(definition, formal) = parameter {
            if definition != target {
                return Err(unsupported(
                    "original owner call needs its complete lexical parameter specialization",
                    None,
                ));
            }
            let actual = self
                .original
                .calls
                .get(&(self.current, site))
                .and_then(|args| args.get(formal as usize))
                .ok_or_else(|| bad("original owner call lacks its full-ordinal Type actual"))?;
            let Arg::Type(ty) = actual else {
                return Err(bad(
                    "original owner call Type formal was replaced by another kind",
                ));
            };
            parameter = self.original.resolved(ty, b)?;
        }
        match owner_type(&parameter) {
            Some(borrowed) => Ok(borrowed),
            None => Err(unsupported(
                "original owner argument has no independently resolved passing category",
                None,
            )),
        }
    }
}

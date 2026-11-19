use super::*;

enum CallPreparation {
    Complete(Term),
    Arguments {
        parameters: Vec<Term>,
        result: Term,
        prepared_arguments: BTreeMap<NodeId, Term>,
    },
}

impl Solver<'_, '_> {
    // Finish selecting the callable before descending into its ordinary arguments.
    pub(super) fn call_expression(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        callee: &SemanticOriginalExpr,
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        let (parameters, result, mut prepared_arguments) =
            match self.prepare_expression_call(frame, value, callee, arguments, b, depth)? {
                CallPreparation::Complete(term) => return Ok(term),
                CallPreparation::Arguments { parameters, result, prepared_arguments } =>
                    (parameters, result, prepared_arguments),
            };
        if parameters.len() != arguments.len() {
            return Err(fail(
                "original call value arity differs from its declared scheme",
            ));
        }
        // The exact receiving expression constrains this call's
        // unresolved result roles before its arguments are checked.
        // This opens no equation between two rigid occurrences.
        if let Some(expected) = self.receiving_for(frame, value.id, b)? {
            self.call_result_context(result, expected, b, depth + 1)?;
        }
        let mut outer_inputs = Vec::new();
        for (ordinal, (argument, parameter)) in arguments.iter().zip(parameters).enumerate() {
            b.lookup(prepared_arguments.len(), WORK)?;
            let prepared = prepared_arguments.remove(&argument.id);
            let actual = if let Some(actual) = self.temporary_description_argument(
                frame, value.id, ordinal, argument, parameter, prepared, b, depth + 1,
            )? {
                actual
            } else if let Some(actual) = prepared {
                actual
            } else {
                self.call_input_value(frame, argument, parameter, b, depth + 1)?
            };
            if frame.alive {
                self.observe_code_argument(frame.definition, value.id, ordinal, actual, b)?;
            }
            if let Some(input) = self.collect_call_input(frame, argument, actual, parameter, ordinal, b, depth + 1)? {
                Self::push(&mut outer_inputs, input, b)?;
            }
        }
        b.lookup(self.state.calls.len(), WORK)?;
        b.lookup(self.state.abstract_calls.len(), WORK)?;
        let prototype = self.state.calls.get(&(frame.definition, value.id)).map(|call| call.target)
            .or_else(|| self.state.abstract_calls.get(&(frame.definition, value.id)).map(|call| call.signature_owner));
        self.collect_result_inputs(prototype, result, &mut outer_inputs, b)?;
        self.settle_call_inputs(frame, value.id, prototype, &outer_inputs, b, depth + 1)?;
        if !prepared_arguments.is_empty() { return Err(fail("original staged call leaves an unchecked argument")); }
        self.finish_abstract_call(frame.definition, value.id, b)?;
        b.lookup(self.state.intrinsics.len(), WORK)?;
        let ty = if self
            .state
            .intrinsics
            .contains_key(&(frame.definition, value.id))
            || self
                .state
                .abstract_calls
                .contains_key(&(frame.definition, value.id))
            || self
                .state
                .code_calls
                .contains_key(&(frame.definition, value.id))
        {
            // Unit has no hidden callee result domain. Its actual opcode
            // is bound separately at this exact original call site.
            result
        } else {
            self.term(
                Kind::CallResult {
                    caller: frame.definition,
                    node: value.id,
                    hidden: result,
                },
                b,
            )?
        };
        self.record_expression(frame, value.id, ty, b)
    }

    fn prepare_expression_call(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        callee: &SemanticOriginalExpr,
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<CallPreparation> {
        let definition = self.inventory.definition(frame.definition, b)?;
        let recipe = self.original_site(definition, value.id, b)?;
        if matches!(recipe.kind, SemanticSiteRecipeKind::Construct { .. }) {
            let term = self.enum_call(frame, value.id, callee, arguments, b, depth + 1)?
                .ok_or_else(|| fail("original formation lacks its exact enum constructor"))?;
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original constructor node is reused"));
            }
            return Ok(CallPreparation::Complete(term));
        }
        if !matches!(
            recipe.kind,
            SemanticSiteRecipeKind::OriginalCall
                | SemanticSiteRecipeKind::Call { .. }
                | SemanticSiteRecipeKind::CallableCall { .. }
        ) {
            return Err(fail("original call is linked to another operation kind"));
        }
        if let Some(term) =
            self.slice_call(frame, value.id, callee, arguments, b, depth + 1)?
        {
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original slice operation node is reused"));
            }
            return Ok(CallPreparation::Complete(term));
        }
        if let Some(term) =
            self.enum_call(frame, value.id, callee, arguments, b, depth + 1)?
        {
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original constructor node is reused"));
            }
            return Ok(CallPreparation::Complete(term));
        }
        if let Some(term) =
            self.original_trait_call(frame, value.id, callee, arguments, b, depth + 1)?
        {
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original trait call node is reused"));
            }
            return Ok(CallPreparation::Complete(term));
        }
        let mut head = callee;
        while let CoreExprKind::Group(inner) = &head.kind {
            self.step(depth, b)?;
            head = inner;
        }
        let mut type_written = Vec::new();
        let (head, written) = match &head.kind {
            CoreExprKind::GenericApply { callee, arguments } => {
                (callee.as_ref(), arguments.as_slice())
            }
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => {
                for argument in type_arguments {
                    crate::dyn_metadata::recipe(argument, b)?;
                    Self::push(
                        &mut type_written,
                        CoreGenericArgument::Type(argument.clone()),
                        b,
                    )?;
                }
                (callee.as_ref(), type_written.as_slice())
            }
            _ => (head, &[][..]),
        };
        if matches!(
            head.kind,
            CoreExprKind::GenericApply { .. } | CoreExprKind::TypeApply { .. }
        ) {
            return Err(unknown(
                "original nested generic application needs every mixed layer interpreted",
            ));
        }
        let evaluated_head = matches!(head.kind,
            CoreExprKind::Call { .. } | CoreExprKind::Closure { .. }
            | CoreExprKind::If { .. } | CoreExprKind::Sequence { .. }
            | CoreExprKind::Index { .. }
            | CoreExprKind::Unary { op: UnaryOp::Borrow | UnaryOp::BorrowMut, .. });
        let local = if evaluated_head {
            Some(self.expression(frame, head, b, depth + 1)?)
        } else if let CoreExprKind::Name(name) = &head.kind {
            self.local(frame, name, b)?
        } else {
            self.place(frame, head, false, b, depth + 1)?
                .filter(|place| !place.path.is_empty())
                .map(|place| place.selected)
        };
        let mut prepared_arguments = BTreeMap::new();
        let (parameters, result) = if let Some(local) = local {
            if !written.is_empty() {
                return Err(unknown(
                    "original generic callable needs its complete method instantiation",
                ));
            }
            self.demand_code_input(frame, head, local, arguments.len(), b, depth + 1)?;
            if let Some((parameters, result)) =
                self.abstract_scheme(frame, head, value.id, local, b, depth + 1)?
            {
                b.lookup(self.state.abstract_calls.len(), WORK)?;
                if self.state.abstract_calls.get(&(frame.definition, value.id))
                    .is_some_and(|call| call.kind == CallableKind::FnOnce) {
                    self.prepare_call_value(frame.definition, head.id, local, b)?;
                }
                (parameters, result)
            } else if let Some(call) =
                self.code_call(frame, head, value.id, local, b, depth + 1)?
            {
                call
            } else {
                let callee_value = local;
                let mut local = self.type_root(local, b)?;
                let borrowed = if let Kind::Address { mutable, inner, .. } = self.state.arena[local] {
                    local = self.type_root(inner, b)?;
                    Some((mutable, inner))
                } else { None };
                b.spend(
                    match &self.state.arena[local] {
                        Kind::Closure {
                            parameters,
                            environment,
                            ..
                        } => parameters.len().saturating_add(environment.len() + 1),
                        _ => 1,
                    },
                    WORK,
                )?;
                let Kind::Closure {
                    definition: target,
                    parameters,
                    result,
                    ..
                } = self.state.arena[local].clone()
                else {
                    return Err(unknown(
                        "original indirect call needs its full callable premise",
                    ));
                };
                // Resolve a real original owning/borrowed place once.
                // This is the same complete source used by the actual
                // field projection, not a Name/temporary reconstruction.
                let receiver_place = if evaluated_head { None }
                    else { Some(self.borrow_place(frame, head, b, depth + 1)?) };
                let receiver = if let Some(source) = self.explicit_callable_borrow(frame, head, b)? {
                    source
                } else if let Some(place) = &receiver_place {
                    b.spend(place.path.len().saturating_add(8), WORK)?;
                    let inner = if let Some((_, inner)) = borrowed { inner } else { place.inner };
                    if borrowed.is_some() && (place.indirect || !place.path.is_empty()) {
                        return Err(unknown("stored callable address needs its complete post-load pointer source"));
                    }
                    Borrow { role: None, anchor: place.anchor, expression: place.expression,
                        index: place.index, root: place.root, path: place.path.clone(),
                        indirect: place.indirect || borrowed.is_some(), inner, slice: None }
                } else {
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.get(&head.id).copied() != Some(callee_value) {
                        return Err(fail("temporary callable head loses its exact evaluated original expression"));
                    }
                    Borrow { role: None, anchor: None, expression: Some(head.id), index: None,
                        root: callee_value, path: Vec::new(), indirect: borrowed.is_some(),
                        inner: borrowed.map_or(callee_value, |(_, inner)| inner), slice: None }
                };
                let subject = borrowed.map_or(callee_value, |(_, inner)| inner);
                let (requested_once, operation) = self.known_call_operation(subject, target, b)?;
                let consuming = self.known_capture_call(frame, head, target, requested_once, operation,
                    borrowed.map(|(mutable, _)| mutable), receiver_place.as_ref(), b)?;
                if consuming { self.prepare_call_value(frame.definition, head.id, callee_value, b)?; }
                let declaration = self.inventory.definition(target, b)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                    &declaration.body
                else {
                    return Err(fail(
                        "original local callable has no full declared invocation signature",
                    ));
                };
                let lives =
                    self.invocation_actuals(signature.invocation_lifetimes, b)?;
                if parameters.len() != signature.parameters.len() {
                    return Err(fail("original known closure changes its declared parameter arity"));
                }
                let mut opened = Vec::new();
                for (parameter, declared) in parameters.into_iter().zip(&signature.parameters) {
                    let parameter =
                        self.open_invocations(parameter, target, &lives, b, depth + 1)?;
                    // Only the genuine original signature grants a
                    // call outer role. A whole T instantiated as an
                    // Address remains an invariant complete Type.
                    self.mark_call_parameter(parameter, &declared.ty, b)?;
                    Self::push(&mut opened, parameter, b)?;
                }
                let parameters = opened;
                let result =
                    self.open_invocations(result, target, &lives, b, depth + 1)?;
                let result = self.known_receiver_output(frame.definition, value.id,
                    subject, target, consuming, result, b)?;
                b.spend(
                    self.state.calls.len().saturating_add(parameters.len() + 1),
                    WORK,
                )?;
                self.state.calls.insert(
                    (frame.definition, value.id),
                    Call {
                        operation: Some(operation),
                        target,
                        actuals: Vec::new(),
                        parameters: parameters.clone(),
                        receiver: Some(receiver),
                    },
                );
                (parameters, result)
            }
        } else {
            let path = sealed::path(head, b, depth + 1)?.ok_or_else(|| {
                unknown("original expression call needs its full source resolution")
            })?;
            // A qualified lexical path is resolved by its complete
            // Names binding. A value root shadows every static path;
            // neither a member spelling nor ABI shape selects code.
            let mut names = BTreeSet::new();
            for local in &frame.locals {
                b.spend(local.name.len().saturating_add(names.len() + 1), WORK)?;
                names.insert(local.name.clone());
            }
            let resolution =
                sealed::resolve(self.inventory, frame.definition, &path, &names, b)?;
            match resolution {
                sealed::Resolution::Intrinsic(sealed::Intrinsic::CCallback) => {
                    let output = self.callback_value(
                        frame,
                        value.id,
                        written,
                        arguments,
                        b,
                        depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, output).is_some() {
                        return Err(fail("original callback call node is reused"));
                    }
                    return Ok(CallPreparation::Complete(output));
                }
                sealed::Resolution::Intrinsic(sealed::Intrinsic::TakeStored) => {
                    let [field] = arguments else { return Err(fail("move_out requires one original stored field")); };
                    if !written.is_empty() || !matches!(field.kind, CoreExprKind::Member { .. }) {
                        return Err(fail("move_out requires its exact original field place"));
                    }
                    let slot = self.borrow_place(frame, field, b, depth + 1)?;
                    if !slot.mutable || !matches!(self.state.arena[self.type_root(slot.inner, b)?], Kind::Address { .. }) {
                        return Err(fail("move_out requires an actual address slot of a mutable holder"));
                    }
                    b.lookup(self.state.intrinsics.len(), WORK)?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if self.state.intrinsics.insert((frame.definition, value.id), sealed::Intrinsic::TakeStored).is_some()
                        || frame.expressions.insert(value.id, slot.inner).is_some() {
                        return Err(fail("move_out repeats its actual original occurrence"));
                    }
                    return Ok(CallPreparation::Complete(slot.inner));
                }
                sealed::Resolution::Intrinsic(sealed::Intrinsic::OwnedDyn) => {
                    let result = self.owned_dyn_call(
                        frame,
                        value.id,
                        written,
                        arguments,
                        b,
                        depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, result).is_some() {
                        return Err(fail("original owned Dyn call node is reused"));
                    }
                    return Ok(CallPreparation::Complete(result));
                }
                sealed::Resolution::Intrinsic(
                    id @ (sealed::Intrinsic::StoreNew
                    | sealed::Intrinsic::StoreReserve
                    | sealed::Intrinsic::StorePush
                    | sealed::Intrinsic::StorePop
                    | sealed::Intrinsic::StoreLen
                    | sealed::Intrinsic::StoreCapacity
                    | sealed::Intrinsic::StoreAt
                    | sealed::Intrinsic::StoreAtMut),
                ) => {
                    let result = self.store_call(
                        frame,
                        value.id,
                        id,
                        written,
                        arguments,
                        b,
                        depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, result).is_some() {
                        return Err(fail("original Store call node is reused"));
                    }
                    return Ok(CallPreparation::Complete(result));
                }
                sealed::Resolution::Intrinsic(
                    id @ (sealed::Intrinsic::BytesNew
                    | sealed::Intrinsic::BytesReserve
                    | sealed::Intrinsic::BytesPush
                    | sealed::Intrinsic::BytesTruncate
                    | sealed::Intrinsic::BytesLen
                    | sealed::Intrinsic::BytesView { .. }
                    | sealed::Intrinsic::BytesCapacity),
                ) => {
                    let result = self.buffer_call(
                        frame,
                        value.id,
                        id,
                        written,
                        arguments,
                        b,
                        depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, result).is_some() {
                        return Err(fail("original ByteBuffer call node is reused"));
                    }
                    return Ok(CallPreparation::Complete(result));
                }
                sealed::Resolution::Intrinsic(sealed::Intrinsic::ScopedForeignCallback) => {
                    let result = self.scoped_foreign_callback(
                        frame, value.id, written, arguments, b, depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, result).is_some() {
                        return Err(fail("original scoped foreign callback node is reused"));
                    }
                    return Ok(CallPreparation::Complete(result));
                }
                sealed::Resolution::Intrinsic(
                    id @ (sealed::Intrinsic::Register
                    | sealed::Intrinsic::RegisterAsync),
                ) => {
                    let result = self.registration_start(
                        frame,
                        value.id,
                        id,
                        written,
                        arguments,
                        b,
                        depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, result).is_some() {
                        return Err(fail(
                            "original registration installation node is reused",
                        ));
                    }
                    return Ok(CallPreparation::Complete(result));
                }
                sealed::Resolution::Intrinsic(
                    id @ (sealed::Intrinsic::AsyncRequest
                    | sealed::Intrinsic::AsyncPoll
                    | sealed::Intrinsic::AsyncFinish
                    | sealed::Intrinsic::AsyncFinishChecked
                    | sealed::Intrinsic::AsyncValid
                    | sealed::Intrinsic::RegistrationValid
                    | sealed::Intrinsic::TaskValid
                    | sealed::Intrinsic::RegistrationClose),
                ) => {
                    let result = self.registration_call(
                        frame,
                        value.id,
                        id,
                        written,
                        arguments,
                        b,
                        depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, result).is_some() {
                        return Err(fail("original registration call node is reused"));
                    }
                    return Ok(CallPreparation::Complete(result));
                }
                sealed::Resolution::Intrinsic(sealed::Intrinsic::TaskYield) => {
                    if !written.is_empty() || !arguments.is_empty() {
                        return Err(fail(
                            "original task_yield has neither generic nor value arguments",
                        ));
                    }
                    b.lookup(self.state.intrinsics.len(), WORK)?;
                    if self
                        .state
                        .intrinsics
                        .insert(
                            (frame.definition, value.id),
                            sealed::Intrinsic::TaskYield,
                        )
                        .is_some()
                    {
                        return Err(fail(
                            "original core operation has competing scheme resolutions",
                        ));
                    }
                    (Vec::new(), self.atom(Type::Unit, b)?)
                }
                sealed::Resolution::Intrinsic(
                    id @ sealed::Intrinsic::NumericConvert(number),
                ) => {
                    if !written.is_empty() || arguments.len() != 1 {
                        return Err(fail(
                            "original numeric conversion changes its closed source scheme",
                        ));
                    }
                    let input = self.var(b)?;
                    Self::push(&mut self.numeric, input, b)?;
                    b.lookup(self.state.intrinsics.len(), WORK)?;
                    if self
                        .state
                        .intrinsics
                        .insert((frame.definition, value.id), id)
                        .is_some()
                    {
                        return Err(fail(
                            "original numeric conversion has competing resolutions",
                        ));
                    }
                    b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
                    self.state
                        .intrinsic_inputs
                        .insert((frame.definition, value.id), vec![input]);
                    (vec![input], self.atom(number.ty(), b)?)
                }
                sealed::Resolution::Intrinsic(
                    id @ (sealed::Intrinsic::MutexCreate
                    | sealed::Intrinsic::MutexLock
                    | sealed::Intrinsic::MutexValue
                    | sealed::Intrinsic::SyncCellScope(_)
                    | sealed::Intrinsic::SyncCellOp(..)
                    | sealed::Intrinsic::SyncCellCompareExchange
                    | sealed::Intrinsic::ThreadScope),
                ) => {
                    let result = self.mutex_call(
                        frame,
                        value.id,
                        id,
                        written,
                        arguments,
                        b,
                        depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, result).is_some() {
                        return Err(fail(
                            "original synchronization call node is reused",
                        ));
                    }
                    return Ok(CallPreparation::Complete(result));
                }
                sealed::Resolution::Intrinsic(
                    id @ (sealed::Intrinsic::TaskAwait
                    | sealed::Intrinsic::TaskCreate
                    | sealed::Intrinsic::TaskCreateOwned
                    | sealed::Intrinsic::TaskPoll
                    | sealed::Intrinsic::TaskPollClose
                    | sealed::Intrinsic::TaskCancel
                    | sealed::Intrinsic::TaskFinish),
                ) => {
                    let result = self.task_call(
                        frame,
                        value.id,
                        id,
                        written,
                        arguments,
                        b,
                        depth + 1,
                    )?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(value.id, result).is_some() {
                        return Err(fail("original Task call node is reused"));
                    }
                    return Ok(CallPreparation::Complete(result));
                }
                sealed::Resolution::Intrinsic(_) => {
                    return Err(unknown(
                        "original core operation needs its complete parameter, mode and cleanup scheme",
                    ));
                }
                sealed::Resolution::Definition(target) => {
                    // Reading a lexical type for inference does
                    // not evaluate or consume the argument. Apply
                    // it before querying dependent result types;
                    // the ordinary call below still evaluates and
                    // checks each actual exactly once in order.
                    let mut hints = Vec::new();
                    for argument in arguments {
                        let hint =
                            self.lexical_input_hint(frame, argument, b, depth + 1)?;
                        Self::push(&mut hints, hint, b)?;
                    }
                    let definition = self.inventory.definition(target, b)?;
                    let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else {
                        return Err(fail("original call inference has no full static signature"));
                    };
                    // A nested argument of an abstract Type must be
                    // evaluated once before dependent trait output
                    // formation. Stage the complete ordered vector
                    // only when all inputs have that direct Type
                    // role and no receiving context is being lost.
                    b.spend(signature.parameters.len().saturating_add(arguments.len().saturating_mul(2)), WORK)?;
                    let abstract_inputs = signature.parameters.iter().all(|parameter|
                        matches!(parameter.ty, SemanticTypeRecipe::Formal(SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0), .. })));
                    let nested = hints.iter().zip(arguments).any(|(hint, argument)| hint.is_none()
                        && matches!(argument.kind, CoreExprKind::Call { .. }));
                    if written.is_empty() && abstract_inputs && nested
                        && self.receiving_for(frame, value.id, b)?.is_none()
                        && hints.iter().zip(arguments).all(|(hint, argument)| hint.is_some()
                            || matches!(argument.kind, CoreExprKind::Call { .. }))
                    {
                        for (ordinal, argument) in arguments.iter().enumerate() {
                            let borrowed = if let Some(Some(hint)) = hints.get(ordinal) {
                                matches!(self.state.arena[self.type_root(*hint, b)?], Kind::Address { .. })
                            } else { false };
                            let actual = if borrowed { self.borrowed_argument(frame, argument, b, depth + 1)? }
                                else { self.expression(frame, argument, b, depth + 1)? };
                            b.lookup(prepared_arguments.len(), WORK)?;
                            if prepared_arguments.insert(argument.id, actual).is_some() {
                                return Err(fail("staged call argument reuses an original source identity"));
                            }
                            self.prepare_call_value(frame.definition, argument.id, actual, b)?;
                            if let Some(hint) = hints.get_mut(ordinal) { *hint = Some(actual); }
                        }
                    }
                    let (actuals, parameters, result) = self.scheme(
                        frame,
                        Some(value.id),
                        target,
                        written,
                        Some(&hints),
                        b,
                        depth + 1,
                    )?;
                    b.spend(
                        self.state
                            .calls
                            .len()
                            .saturating_add(actuals.len())
                            .saturating_add(parameters.len()),
                        WORK,
                    )?;
                    self.state.calls.insert(
                        (frame.definition, value.id),
                        Call {
                            operation: None,
                            target,
                            actuals,
                            parameters: parameters.clone(),
                            receiver: None,
                        },
                    );
                    (parameters, result)
                }
                sealed::Resolution::Deferred => {
                    return Err(unknown(
                        "original call has no complete lexical declaration or core scheme",
                    ));
                }
            }
        };
        Ok(CallPreparation::Arguments { parameters, result, prepared_arguments })
    }
}

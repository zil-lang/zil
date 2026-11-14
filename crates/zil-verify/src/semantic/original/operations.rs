//! Closed operational schemes in the universal original world. Runtime owner,
//! element, worker, provider and phase authority is independently replayed from
//! the actual instruction graph; these rows are only scoped type obligations.
use super::*;
use intrinsics::{BufferSource, Intrinsic, Use};
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn operation(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        kind: Intrinsic,
        site: SemanticSiteId,
        written: &[Arg],
        expected: Option<&Ty>,
        arguments: &[SemanticOriginalExpr],
        env: &Env,
        bindings: &BTreeMap<String, Binding>,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Option<Ty>> {
        if matches!(
            kind,
            Intrinsic::Convert(_)
                | Intrinsic::TaskYield
                | Intrinsic::BytesNew
                | Intrinsic::BytesReserve
                | Intrinsic::BytesPush
                | Intrinsic::BytesTruncate
                | Intrinsic::BytesLen
                | Intrinsic::BytesCapacity
        ) {
            return Ok(None);
        }
        let mut use_ = Use {
            kind,
            buffer: None,
            worker: None,
            providers: None,
        };
        if kind != Intrinsic::StoreNew && !written.is_empty() {
            return Err(bad("operational intrinsic has unexpected mixed actuals"));
        }
        let result = match kind {
            Intrinsic::StoreNew => {
                if !arguments.is_empty() || written.len() > 1 {
                    return Err(bad("store_new has another complete scheme arity"));
                }
                let explicit = match written.first() {
                    Some(Arg::Type(ty)) => Some(ty.clone()),
                    Some(_) => return Err(bad("store_new actual is not an element Type")),
                    None => None,
                };
                let contextual = expected.map(|ty| self.resolved(ty, b)).transpose()?;
                let element = match contextual {
                    Some(Ty::Managed(inner)) => match *inner {
                        Ty::Store(element) => Some(*element),
                        _ => None,
                    },
                    Some(Ty::Store(element)) => Some(*element),
                    _ => None,
                };
                let element = match (explicit, element) {
                    (Some(a), Some(c)) => {
                        self.unify(&a, &c, b)?;
                        a
                    }
                    (Some(a), _) | (_, Some(a)) => a,
                    _ => self.fresh(b)?,
                };
                b.spend(1)?;
                self.element_obligations
                    .push((element.clone(), env.clone()));
                Ty::Managed(Box::new(Ty::Store(Box::new(element))))
            }
            Intrinsic::StoreReserve
            | Intrinsic::StorePush
            | Intrinsic::StorePop
            | Intrinsic::StoreGet
            | Intrinsic::StoreGetMut
            | Intrinsic::StoreLen
            | Intrinsic::StoreCapacity => {
                let arity = if matches!(
                    kind,
                    Intrinsic::StoreReserve
                        | Intrinsic::StorePush
                        | Intrinsic::StoreGet
                        | Intrinsic::StoreGetMut
                ) {
                    2
                } else {
                    1
                };
                self.operation_arity(arguments, arity)?;
                let (buffer, source) = self.operation_observe(
                    module,
                    context,
                    &arguments[0],
                    env,
                    bindings,
                    active,
                    b,
                )?;
                use_.buffer = Some(source);
                let (payload, life) =
                    self.operation_payload(&buffer, env.current()?, arguments[0].id, b)?;
                let Ty::Store(element) = payload else {
                    return Err(bad("Store operation has another complete resource payload"));
                };
                self.element_obligations
                    .push((*element.clone(), env.clone()));
                match kind {
                    Intrinsic::StoreReserve => {
                        self.operation_scalar(
                            module,
                            context,
                            &arguments[1],
                            Type::I64,
                            env,
                            bindings,
                            active,
                            b,
                        )?;
                        Ty::Atom(Type::Bool)
                    }
                    Intrinsic::StorePush => {
                        let actual = self.expression(
                            module,
                            context,
                            &arguments[1],
                            env,
                            bindings,
                            Some(&element),
                            active,
                            b,
                        )?;
                        self.unify(&actual, &element, b)?;
                        Ty::ErrorUnion(
                            Box::new(element.as_ref().clone()),
                            Box::new(Ty::Atom(Type::Unit)),
                        )
                    }
                    Intrinsic::StorePop => Ty::ErrorUnion(Box::new(Ty::Atom(Type::Unit)), element),
                    Intrinsic::StoreGet | Intrinsic::StoreGetMut => {
                        self.operation_scalar(
                            module,
                            context,
                            &arguments[1],
                            Type::I64,
                            env,
                            bindings,
                            active,
                            b,
                        )?;
                        Ty::Address {
                            mutable: kind == Intrinsic::StoreGetMut,
                            life,
                            inner: element,
                        }
                    }
                    _ => Ty::Atom(Type::I64),
                }
            }
            Intrinsic::BytesView | Intrinsic::BytesViewMut => {
                self.operation_arity(arguments, 1)?;
                let (buffer, source) = self.operation_observe(
                    module,
                    context,
                    &arguments[0],
                    env,
                    bindings,
                    active,
                    b,
                )?;
                use_.buffer = Some(source);
                let (payload, life) =
                    self.operation_payload(&buffer, env.current()?, arguments[0].id, b)?;
                if payload != Ty::Atom(Type::ByteBuffer) {
                    return Err(bad("byte view has another actual resource payload"));
                }
                Ty::Address {
                    mutable: kind == Intrinsic::BytesViewMut,
                    life,
                    inner: Box::new(Ty::Slice(Box::new(Ty::Atom(Type::U8)))),
                }
            }
            Intrinsic::MutexCreate => {
                self.operation_arity(arguments, 1)?;
                let value = self.expression(
                    module,
                    context,
                    &arguments[0],
                    env,
                    bindings,
                    None,
                    active,
                    b,
                )?;
                self.structural_obligation(
                    module,
                    context,
                    &value,
                    DynStructuralBound::Value,
                    env,
                    active,
                    b,
                )?;
                self.structural_obligation(
                    module,
                    context,
                    &value,
                    DynStructuralBound::SyncDrop,
                    env,
                    active,
                    b,
                )?;
                Ty::ErrorUnion(
                    Box::new(Ty::SealedError(SealedError::MutexCreate)),
                    Box::new(Ty::Managed(Box::new(Ty::Carrier {
                        kind: CarrierKind::Mutex,
                        child: Box::new(value),
                        retained: Box::default(),
                    }))),
                )
            }
            Intrinsic::MutexLock => {
                self.operation_arity(arguments, 1)?;
                let (buffer, source) = self.operation_observe(
                    module,
                    context,
                    &arguments[0],
                    env,
                    bindings,
                    active,
                    b,
                )?;
                use_.buffer = Some(source);
                let (payload, life) =
                    self.operation_payload(&buffer, env.current()?, arguments[0].id, b)?;
                let Ty::Carrier {
                    kind: CarrierKind::Mutex,
                    child,
                    retained,
                } = payload
                else {
                    return Err(bad("mutex_lock has another complete manager type"));
                };
                if *retained != Retained::default() {
                    return Err(bad("Mutex payload invents a retained guard domain"));
                }
                let leaf = Ty::Carrier {
                    kind: CarrierKind::Mutex,
                    child: child.clone(),
                    retained: Box::default(),
                };
                Ty::Carrier {
                    kind: CarrierKind::MutexGuard,
                    child,
                    retained: Box::new(Retained {
                        domain: vec![ClassPart::One(life.clone())],
                        ports: vec![PortPart::One {
                            leaf,
                            mode: StoredBorrowMode::SharedAddress,
                            class: life,
                        }],
                        ..Retained::default()
                    }),
                }
            }
            Intrinsic::MutexValue | Intrinsic::MutexUnlock => {
                self.operation_arity(arguments, 1)?;
                let actual = if kind == Intrinsic::MutexValue {
                    self.expression(
                        module,
                        context,
                        &arguments[0],
                        env,
                        bindings,
                        None,
                        active,
                        b,
                    )?
                } else {
                    self.expression(
                        module,
                        context,
                        &arguments[0],
                        env,
                        bindings,
                        None,
                        active,
                        b,
                    )?
                };
                let (payload, life) = match self.resolved(&actual, b)? {
                    Ty::Address {
                        mutable: true,
                        life,
                        inner,
                    } if kind == Intrinsic::MutexValue => (*inner, life),
                    value if kind == Intrinsic::MutexUnlock => {
                        (value, Life::Local(env.current()?, arguments[0].id))
                    }
                    _ => return Err(bad("mutex_value requires an actual exclusive guard borrow")),
                };
                let Ty::Carrier {
                    kind: CarrierKind::MutexGuard,
                    child,
                    ..
                } = payload
                else {
                    return Err(bad("guard operation has another complete guard type"));
                };
                if kind == Intrinsic::MutexValue {
                    use_.buffer = Some(self.operation_source(&arguments[0], env, bindings, b)?);
                    Ty::Address {
                        mutable: true,
                        life,
                        inner: child,
                    }
                } else {
                    Ty::Atom(Type::Unit)
                }
            }
            Intrinsic::TaskCreate
            | Intrinsic::TaskCreateOwned
            | Intrinsic::TaskValid
            | Intrinsic::TaskPoll
            | Intrinsic::TaskRequest
            | Intrinsic::TaskPollClose
            | Intrinsic::TaskFinish => self.task_operation(
                module, context, kind, arguments, env, bindings, &mut use_, active, b,
            )?,
            Intrinsic::AsyncStart => {
                self.operation_arity(arguments, 4)?;
                if self.unsafe_depth.get(&env.current()?).copied().unwrap_or(0) == 0 {
                    return Err(bad(
                        "async registration construction lacks its original unsafe scope",
                    ));
                }
                let mut providers = Vec::new();
                for argument in &arguments[..3] {
                    let value =
                        self.expression(module, context, argument, env, bindings, None, active, b)?;
                    let Ty::Function(id) = value else {
                        return Err(bad(
                            "async provider is not a real original foreign declaration",
                        ));
                    };
                    providers.push(id);
                }
                let threaded = self.operation_providers(module, context, &providers, b)?;
                use_.providers = Some([providers[0], providers[1], providers[2]]);
                let callback = self.expression(
                    module,
                    context,
                    &arguments[3],
                    env,
                    bindings,
                    None,
                    active,
                    b,
                )?;
                let signature =
                    self.operation_callable(module, context, &callback, env, active, b)?;
                if signature.parameters != [Ty::Atom(Type::I32)]
                    || signature.result != Ty::Atom(Type::I32)
                    || signature.call_kind != CallKind::Direct
                    || !matches!(
                        signature.callable_kind,
                        Some(CallableKind::Fn | CallableKind::FnMut)
                    )
                {
                    return Err(bad(
                        "async callback has another complete original FnMut signature/effect",
                    ));
                }
                self.builtin_obligation(
                    module,
                    context,
                    &callback,
                    SemanticBuiltinTrait::ClosedEnvironment,
                    env,
                    active,
                    b,
                )?;
                self.structural_obligation(
                    module,
                    context,
                    &callback,
                    DynStructuralBound::SyncDrop,
                    env,
                    active,
                    b,
                )?;
                if threaded {
                    self.builtin_obligation(
                        module,
                        context,
                        &callback,
                        SemanticBuiltinTrait::Transfer,
                        env,
                        active,
                        b,
                    )?;
                    self.builtin_obligation(
                        module,
                        context,
                        &callback,
                        SemanticBuiltinTrait::ThreadCallable,
                        env,
                        active,
                        b,
                    )?;
                }
                Ty::Managed(Box::new(Ty::Carrier {
                    kind: CarrierKind::AsyncRegistration,
                    child: Box::new(callback),
                    retained: Box::default(),
                }))
            }
            Intrinsic::AsyncValid
            | Intrinsic::AsyncRequest
            | Intrinsic::AsyncPoll
            | Intrinsic::AsyncFinish
            | Intrinsic::AsyncFinishChecked => {
                // Checked destruction is admitted only if the mandatory
                // original protocol replay proves Closed at this exact call.
                self.operation_arity(arguments, 1)?;
                let destroy =
                    matches!(kind, Intrinsic::AsyncFinish | Intrinsic::AsyncFinishChecked);
                let value = if destroy {
                    if !matches!(
                        arguments[0].kind,
                        CoreExprKind::Unary {
                            op: UnaryOp::Give,
                            ..
                        }
                    ) {
                        return Err(bad("registration destroy omits explicit give"));
                    }
                    self.expression(
                        module,
                        context,
                        &arguments[0],
                        env,
                        bindings,
                        None,
                        active,
                        b,
                    )?
                } else {
                    let (value, source) = self.operation_observe(
                        module,
                        context,
                        &arguments[0],
                        env,
                        bindings,
                        active,
                        b,
                    )?;
                    use_.buffer = Some(source);
                    value
                };
                let (payload, _) =
                    self.operation_payload(&value, env.current()?, arguments[0].id, b)?;
                if !matches!(
                    payload,
                    Ty::Carrier {
                        kind: CarrierKind::AsyncRegistration,
                        ..
                    }
                ) {
                    return Err(bad(
                        "async operation has another complete registration environment",
                    ));
                }
                if matches!(kind, Intrinsic::AsyncRequest | Intrinsic::AsyncPoll)
                    && !matches!(self.resolved(&value, b)?, Ty::Address { mutable: true, .. })
                {
                    return Err(bad("async close lacks its exclusive original owner borrow"));
                }
                match kind {
                    Intrinsic::AsyncValid => Ty::Atom(Type::Bool),
                    Intrinsic::AsyncPoll => self.poll_type(module, Ty::Atom(Type::Unit), b)?,
                    _ => Ty::Atom(Type::Unit),
                }
            }
            Intrinsic::ThreadScope => {
                return Err(unsupported(
                    "scoped thread requires its complete original worker/body/manager role proof",
                    None,
                ));
            }
            _ => return Err(bad("operation entered another closed scheme")),
        };
        self.execute_effect(env, CallKind::Direct, b)?;
        b.spend(1)?;
        self.intrinsic_calls.insert((env.current()?, site), use_);
        Ok(Some(result))
    }
    pub(super) fn operation_arity(
        &self,
        args: &[SemanticOriginalExpr],
        expected: usize,
    ) -> Check<()> {
        if args.len() == expected {
            Ok(())
        } else {
            Err(bad(
                "operational intrinsic has another complete value arity",
            ))
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn operation_scalar(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        value: &SemanticOriginalExpr,
        ty: Type,
        env: &Env,
        bindings: &BTreeMap<String, Binding>,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        let expected = Ty::Atom(ty);
        let actual = self.expression(
            module,
            context,
            value,
            env,
            bindings,
            Some(&expected),
            active,
            b,
        )?;
        self.unify(&actual, &expected, b)
    }
    fn operation_source(
        &self,
        expression: &SemanticOriginalExpr,
        env: &Env,
        bindings: &BTreeMap<String, Binding>,
        b: &mut Budget,
    ) -> Check<BufferSource> {
        let mut node = expression;
        loop {
            b.spend(1)?;
            match &node.kind {
                CoreExprKind::Group(inner)
                | CoreExprKind::Unary {
                    op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                    operand: inner,
                } => node = inner,
                CoreExprKind::Name(name) => {
                    return match &bindings
                        .get(name)
                        .ok_or_else(|| bad("operation owner name lacks its original binder"))?
                        .identity
                    {
                        Binder::Parameter {
                            definition,
                            ordinal,
                        } => Ok(BufferSource::Parameter {
                            definition: *definition,
                            ordinal: *ordinal,
                        }),
                        Binder::Local {
                            definition, node, ..
                        } => Ok(BufferSource::Local {
                            definition: *definition,
                            node: *node,
                        }),
                        _ if self.value_places.contains_key(&(env.current()?, node.id)) => {
                            Ok(BufferSource::Place {
                                definition: env.current()?,
                                node: node.id,
                            })
                        }
                        _ => Err(unsupported(
                            "operation captured owner needs its complete actual capture source bridge",
                            None,
                        )),
                    };
                }
                _ if self.value_places.contains_key(&(env.current()?, node.id)) => {
                    return Ok(BufferSource::Place {
                        definition: env.current()?,
                        node: node.id,
                    });
                }
                _ => {
                    return Err(unsupported(
                        "operation projected owner needs its complete original typed path bridge",
                        None,
                    ));
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn operation_observe(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        expression: &SemanticOriginalExpr,
        env: &Env,
        bindings: &BTreeMap<String, Binding>,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<(Ty, BufferSource)> {
        let (value, place) = self.place(module, context, expression, env, bindings, active, b)?;
        if let Some(place) = place {
            self.read_original(module, context, &place, &value, env, false, active, b)?;
        }
        Ok((
            self.resolved(&value, b)?,
            self.operation_source(expression, env, bindings, b)?,
        ))
    }
    pub(super) fn operation_payload(
        &self,
        ty: &Ty,
        owner: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<(Ty, Life)> {
        Ok(match self.resolved(ty, b)? {
            Ty::Managed(inner) => (*inner, Life::Local(owner, node)),
            Ty::Desc { life, inner } => (*inner, life),
            Ty::Address { life, inner, .. } => match *inner {
                Ty::Managed(payload) => (*payload, life),
                payload => (payload, life),
            },
            _ => {
                return Err(bad(
                    "operational input is not a real owner, description or owner address",
                ));
            }
        })
    }
    pub(super) fn operation_function(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        id: SemanticDefinitionId,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Prototype> {
        let definition = context.definition(module, id, b)?;
        if !definition.parameters.is_empty() {
            return Err(unsupported(
                "worker function value needs its complete original written specialization",
                None,
            ));
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            body: SemanticOriginalFunctionBody::Defined(_),
            ..
        } = &definition.body
        else {
            return Err(bad("worker is not a real original executable body"));
        };
        let env = self.environment(module, context, id, active, b)?;
        self.call_signature(module, context, id, signature, &env, active, b)
    }
    fn operation_callable(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        ty: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Prototype> {
        match self.resolved(ty, b)? {
            Ty::Closure(id, ..) => self.prototypes.get(&id).cloned().ok_or_else(|| {
                bad("callback lacks its complete universally checked original entry")
            }),
            ty => self
                .callable_premise(module, context, &ty, env, active, b)?
                .ok_or_else(|| {
                    bad("abstract callback lacks its original complete Callable premise")
                }),
        }
    }
    fn operation_providers(
        &self,
        module: &TypedModule,
        context: &CheckedContext,
        providers: &[SemanticDefinitionId],
        b: &mut Budget,
    ) -> Check<bool> {
        let mut roles = Vec::new();
        for id in providers {
            let definition = context.definition(module, *id, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::External,
                foreign: Some(foreign),
                ..
            } = &definition.body
            else {
                return Err(bad(
                    "registration provider is missing its complete original external header",
                ));
            };
            self.function_head(definition, b)?;
            if !foreign.static_callbacks {
                return Err(bad(
                    "registration provider lacks the original static callback promise",
                ));
            }
            roles.push(
                foreign.scoped_callback.ok_or_else(|| {
                    bad("original registration provider has no exact protocol role")
                })?,
            );
        }
        if roles
            == [
                ScopedCallbackRole::AsyncRegister,
                ScopedCallbackRole::AsyncRequestClose,
                ScopedCallbackRole::AsyncPollDrain,
            ]
        {
            Ok(false)
        } else if roles
            == [
                ScopedCallbackRole::ThreadAsyncRegister,
                ScopedCallbackRole::ThreadAsyncRequestClose,
                ScopedCallbackRole::ThreadAsyncPollDrain,
            ]
        {
            Ok(true)
        } else {
            Err(bad(
                "registration providers mix or substitute original role families",
            ))
        }
    }
}

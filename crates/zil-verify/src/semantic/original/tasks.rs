//! Universal Task schemes retain whole symbolic input domains. Constructing a
//! frame references a worker; it never executes that worker or its Suspend edge.
use super::*;
use intrinsics::{Intrinsic, Use};

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn task_operation(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        kind: Intrinsic,
        site: SemanticSiteId,
        arguments: &[SemanticOriginalExpr],
        env: &Env,
        bindings: &BTreeMap<String, Binding>,
        use_: &mut Use,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Ty> {
        if matches!(kind, Intrinsic::TaskCreate | Intrinsic::TaskCreateOwned) {
            if arguments.is_empty() {
                return Err(bad("Task creation omits its original worker"));
            }
            let (worker, worker_environment, worker_actuals, mut prototype) =
                self.task_worker(module, context, &arguments[0], env, bindings, active, b)?;
            self.operation_arity(&arguments[1..], prototype.parameters.len())?;
            if !matches!(prototype.call_kind, CallKind::Direct | CallKind::Suspend) {
                return Err(bad("Task worker has another complete call convention"));
            }
            let mut retained = Retained::default();
            let mut inputs = Vec::new();
            b.spend(prototype.parameters.len())?;
            for (ordinal, ((argument, parameter), mode)) in arguments[1..]
                .iter()
                .zip(&prototype.parameters)
                .zip(&prototype.modes)
                .enumerate()
            {
                let parameter =
                    self.normalize_original_type(module, context, parameter, env, active, b)?;
                let actual = self.argument(
                    module, context, argument, &parameter, *mode, env, bindings, active, b,
                )?;
                self.unify(&actual, &parameter, b)?;
                let actual = self.resolved(&actual, b)?;
                self.structural_obligation(
                    module,
                    context,
                    &actual,
                    DynStructuralBound::Value,
                    env,
                    active,
                    b,
                )?;
                if kind != Intrinsic::TaskCreateOwned
                    && !self
                        .task_shape(module, context, &actual, false, active, b, 0)?
                        .has_task()
                {
                    self.structural_obligation(
                        module,
                        context,
                        &actual,
                        DynStructuralBound::SyncDrop,
                        env,
                        active,
                        b,
                    )?;
                }

                // All(T) is kept even if this particular sample happens to be
                // zero. The actual interpreter expands its full κ directory.
                if !self.task_zero_domain(module, context, &actual, env, active, b, 0)? {
                    let classes = vec![ClassPart::All {
                        ty: actual.clone(),
                        path: vec![],
                    }];
                    retained.domain.extend(classes.clone());
                    retained.ports.push(PortPart::FromType {
                        ty: actual.clone(),
                        classes: classes.clone(),
                    });
                    retained.retained.push(RetainedPart::All {
                        path: vec![LifeTypeProjection::Parameter(ordinal as u32)],
                        parents: classes.clone(),
                    });
                    retained.nominal.push(NominalPart::FromType {
                        path: vec![LifeTypeProjection::Parameter(ordinal as u32)],
                        ty: actual.clone(),
                        classes,
                    });
                }
                inputs.push(actual);
            }
            prototype.result =
                self.normalize_original_type(module, context, &prototype.result, env, active, b)?;
            self.structural_obligation(
                module,
                context,
                &prototype.result,
                DynStructuralBound::Value,
                env,
                active,
                b,
            )?;
            // Cancellation and an unobserved result must settle the complete
            // result. A concrete Copy sample cannot establish abstract cleanup.
            self.structural_obligation(
                module,
                context,
                &prototype.result,
                DynStructuralBound::SyncDrop,
                env,
                active,
                b,
            )?;
            if !self.task_zero_domain(module, context, &prototype.result, env, active, b, 0)? {
                retained.retained.push(RetainedPart::All {
                    path: vec![LifeTypeProjection::Result],
                    parents: vec![ClassPart::All {
                        ty: prototype.result.clone(),
                        path: vec![],
                    }],
                });
            }
            let worker_definition = context.definition(module, worker, b)?;
            for requirement in &worker_definition.requirements {
                self.obligation(
                    module,
                    context,
                    requirement,
                    &worker_environment,
                    env,
                    active,
                    b,
                )?;
            }
            for formal in &worker_definition.parameters {
                if let SemanticFormal::Type { requirements } = formal {
                    for requirement in requirements {
                        self.obligation(
                            module,
                            context,
                            requirement,
                            &worker_environment,
                            env,
                            active,
                            b,
                        )?;
                    }
                }
            }
            self.calls.insert((env.current()?, site), worker_actuals);
            self.call_targets.insert((env.current()?, site), worker);
            use_.worker = Some(worker);
            let task = Ty::Managed(Box::new(Ty::Carrier {
                kind: CarrierKind::Task,
                child: Box::new(prototype.result),
                retained: Box::new(retained),
            }));
            let error = if kind == Intrinsic::TaskCreateOwned {
                self.task_failure_type(
                    module,
                    context,
                    Ty::Tuple(inputs),
                    task.clone(),
                    env,
                    active,
                    b,
                )?
            } else {
                self.task_error_type(module, zil_ir::builtin_life::TASK_CREATE_ERROR, b)?
            };
            return Ok(Ty::ErrorUnion(Box::new(error), Box::new(task)));
        }
        self.operation_arity(arguments, 1)?;
        let value = if kind == Intrinsic::TaskFinish {
            if !matches!(
                arguments[0].kind,
                CoreExprKind::Unary {
                    op: UnaryOp::Give,
                    ..
                }
            ) {
                return Err(bad("Task destruction omits its explicit owning transfer"));
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
            let (value, source) =
                self.operation_observe(module, context, &arguments[0], env, bindings, active, b)?;
            use_.buffer = Some(source);
            value
        };
        let value = self.resolved(&value, b)?;
        if kind == Intrinsic::TaskFinish && !matches!(value, Ty::Managed(_)) {
            return Err(bad(
                "Task destruction consumes something other than its owning head",
            ));
        }
        if matches!(
            kind,
            Intrinsic::TaskPoll | Intrinsic::TaskRequest | Intrinsic::TaskPollClose
        ) && !matches!(value,Ty::Address{mutable:true,ref inner,..} if matches!(inner.as_ref(),Ty::Managed(_)))
        {
            return Err(bad(
                "Task driving operation lacks an exclusive owning-head borrow",
            ));
        }
        let (payload, _) = self.operation_payload(&value, env.current()?, arguments[0].id, b)?;
        let Ty::Carrier {
            kind: CarrierKind::Task,
            child,
            ..
        } = payload
        else {
            return Err(bad(
                "Task operation selects another complete frame/result type",
            ));
        };
        Ok(match kind {
            Intrinsic::TaskValid => Ty::Atom(Type::Bool),
            Intrinsic::TaskPoll => Ty::ErrorUnion(
                Box::new(self.task_error_type(module, zil_ir::builtin_life::TASK_POLL_STATE, b)?),
                Box::new(self.poll_type(module, *child, b)?),
            ),
            Intrinsic::TaskPollClose => self.poll_type(module, Ty::Atom(Type::Unit), b)?,
            Intrinsic::TaskRequest | Intrinsic::TaskFinish => Ty::Atom(Type::Unit),
            _ => return Err(bad("Task scheme received another intrinsic role")),
        })
    }

    /// A worker is instantiated as a value; its body is not executed in the
    /// constructor's effect context. All source where clauses remain caller
    /// obligations after complete argument inference.
    #[allow(clippy::too_many_arguments)]
    fn task_worker(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        expression: &SemanticOriginalExpr,
        caller: &Env,
        bindings: &BTreeMap<String, Binding>,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<(SemanticDefinitionId, Env, Vec<Arg>, Prototype)> {
        let (base, types, mixed) = match &expression.kind {
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => (callee.as_ref(), Some(type_arguments.as_slice()), None),
            CoreExprKind::GenericApply { callee, arguments } => {
                (callee.as_ref(), None, Some(arguments.as_slice()))
            }
            _ => (expression, None, None),
        };
        let worker = self.expression(module, context, base, caller, bindings, None, active, b)?;
        let Ty::Function(worker) = self.resolved(&worker, b)? else {
            return Err(bad(
                "Task worker is not its exact original static definition",
            ));
        };
        let definition = context.definition(module, worker, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            body: SemanticOriginalFunctionBody::Defined(_),
            ..
        } = &definition.body
        else {
            return Err(bad(
                "Task worker is not a complete original executable definition",
            ));
        };
        if !definition.environment.is_empty() {
            return Err(bad(
                "static Task worker cannot manufacture a sealed closure environment",
            ));
        }
        let written = types.map_or_else(
            || mixed.map_or(0, <[CoreGenericArgument<SemanticTypeRecipe>]>::len),
            <[SemanticTypeRecipe]>::len,
        );
        if written > definition.parameters.len() {
            return Err(bad("Task worker has excess full-ordinal actuals"));
        }
        let mut selected = self.environment(module, context, worker, active, b)?;
        let mut arguments = Vec::new();
        for (ordinal, formal) in definition.parameters.iter().enumerate() {
            b.spend(1)?;
            let value = if let Some(ty) = types.and_then(|types| types.get(ordinal)) {
                Some(Arg::Type(self.ty(module, context, ty, caller, active, b)?))
            } else {
                match mixed.and_then(|mixed| mixed.get(ordinal)) {
                    Some(CoreGenericArgument::Type(ty)) => {
                        Some(Arg::Type(self.ty(module, context, ty, caller, active, b)?))
                    }
                    Some(CoreGenericArgument::Lifetime(name)) => {
                        let named = self
                            .named(
                                module,
                                context,
                                caller.current()?,
                                name,
                                SemanticOriginalNamespace::Lifetime,
                                b,
                            )?
                            .ok_or_else(|| {
                                bad("Task worker Life actual lacks its true lexical binding")
                            })?;
                        let SemanticOriginalNameValue::Recipe(
                            recipe @ SemanticActualRecipe::Life(_),
                        ) = named
                        else {
                            return Err(bad("Task worker Life actual selects another kind"));
                        };
                        Some(self.actual(module, context, recipe, caller, active, b)?)
                    }
                    Some(CoreGenericArgument::Const(expression)) => {
                        let SemanticFormal::Const { carrier } = formal else {
                            return Err(bad(
                                "Task worker Const actual selects another full-ordinal kind",
                            ));
                        };
                        let carrier = self.ty(module, context, carrier, &selected, active, b)?;
                        Some(Arg::Const(self.task_constant(
                            module, context, expression, &carrier, caller, active, b,
                        )?))
                    }
                    None => None,
                }
            };
            let value = match (value, formal) {
                (Some(value), _) => value,
                (None, SemanticFormal::Type { .. }) => Arg::Type(self.fresh(b)?),
                (None, SemanticFormal::Lifetime) => Arg::Life(self.fresh_life(b)?),
                (None, SemanticFormal::Const { .. }) => {
                    return Err(bad("Task worker Const actual is not explicitly determined"));
                }
            };
            if !matches!(
                (formal, &value),
                (SemanticFormal::Type { .. }, Arg::Type(_))
                    | (SemanticFormal::Lifetime, Arg::Life(_))
                    | (SemanticFormal::Const { .. }, Arg::Const(_))
            ) {
                return Err(bad("Task worker changes its full-ordinal argument kind"));
            }
            if let (SemanticFormal::Const { carrier }, Arg::Const(value)) = (formal, &value) {
                let carrier = self.ty(module, context, carrier, &selected, active, b)?;
                self.unify(&carrier, &value.carrier, b)?;
            }
            selected.lexical[0].actuals[ordinal] = value.clone();
            arguments.push(value);
        }
        let prototype =
            self.call_signature(module, context, worker, signature, &selected, active, b)?;
        Ok((worker, selected, arguments, prototype))
    }
    fn task_constant(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        expression: &SemanticOriginalExpr,
        carrier: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Const> {
        b.spend(1)?;
        let value = match &expression.kind {
            CoreExprKind::Group(value) | CoreExprKind::CompileTime(value) => {
                return self.task_constant(module, context, value, carrier, env, active, b);
            }
            CoreExprKind::Name(name) => {
                let Arg::Const(value) = self.bound_name(module, context, name, env, active, b)?
                else {
                    return Err(bad("Task worker Const name has another lexical kind"));
                };
                self.unify(&value.carrier, carrier, b)?;
                return Ok(value);
            }
            CoreExprKind::Literal(value) => {
                self.unify(&Ty::Atom(value.ty()), carrier, b)?;
                ConstValue::Value(value.clone())
            }
            CoreExprKind::Bool(value) => {
                self.unify(&Ty::Atom(Type::Bool), carrier, b)?;
                ConstValue::Value(Constant::Bool(*value))
            }
            CoreExprKind::Integer(text) | CoreExprKind::Float(text) => {
                let recipe = if matches!(expression.kind, CoreExprKind::Integer(_)) {
                    SemanticConstRecipe::IntegerLiteral(text.clone())
                } else {
                    SemanticConstRecipe::FloatLiteral(text.clone())
                };
                return self.constant(module, context, &recipe, carrier, env, active, b);
            }
            CoreExprKind::Unary { op, operand } => ConstValue::Unary(
                *op,
                Box::new(self.task_constant(module, context, operand, carrier, env, active, b)?),
            ),
            CoreExprKind::Binary { op, left, right } => ConstValue::Binary(
                *op,
                Box::new(self.task_constant(module, context, left, carrier, env, active, b)?),
                Box::new(self.task_constant(module, context, right, carrier, env, active, b)?),
            ),
            _ => {
                return Err(unsupported(
                    "Task worker Const argument needs a complete original scalar expression",
                    Some(expression.span),
                ));
            }
        };
        Ok(Const {
            carrier: Box::new(carrier.clone()),
            value,
        })
    }

    fn task_error_type(
        &self,
        module: &TypedModule,
        id: LifeDeclarationId,
        b: &mut Budget,
    ) -> Check<Ty> {
        let expected = if id == zil_ir::builtin_life::TASK_CREATE_ERROR {
            &["Allocation"][..]
        } else {
            &["Complete", "Cancelled", "Invalid"][..]
        };
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("Task error scheme lacks canonical source declarations"))?;
        b.spend(program.definitions.len())?;
        let mut rows=program.definitions.iter().filter(|row|matches!(&row.body,SemanticDefinitionBody::Nominal{declaration,..}if *declaration==id));
        let row = rows
            .next()
            .ok_or_else(|| bad("Task error scheme has no actual canonical original"))?;
        if rows.next().is_some()
            || !row.parameters.is_empty()
            || !row.requirements.is_empty()
            || !row.environment.is_empty()
            || row.parent.is_some()
        {
            return Err(bad("Task error changes its complete original scope"));
        }
        let SemanticDefinitionBody::Nominal {
            kind: LifeNominalKind::Enum,
            body: SemanticNominalBody::Enum { variants },
            ..
        } = &row.body
        else {
            return Err(bad("Task error has another original declaration kind"));
        };
        if variants.len() != expected.len()
            || !variants
                .iter()
                .zip(expected)
                .all(|(row, name)| row.name == *name && row.fields.is_empty())
        {
            return Err(bad("Task error changes its actual complete variant scheme"));
        }
        Ok(Ty::Nominal(row.id, vec![]))
    }

    fn task_failure_type(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        inputs: Ty,
        task: Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Ty> {
        let copy = self.is_copy(module, context, &inputs, env, active, b)?;
        let wanted = if copy {
            zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
        } else {
            zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
        };
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("Task failure lacks its original declaration inventory"))?;
        b.spend(program.definitions.len())?;
        let mut rows=program.definitions.iter().filter(|row|matches!(&row.body,SemanticDefinitionBody::Nominal{declaration,..} if *declaration==wanted));
        let definition = rows
            .next()
            .ok_or_else(|| bad("Task failure has no exact canonical original declaration"))?;
        if rows.next().is_some()
            || definition.parameters.len() != 2
            || !definition
                .parameters
                .iter()
                .all(|p| matches!(p,SemanticFormal::Type{requirements}if requirements.is_empty()))
            || !definition.requirements.is_empty()
            || !definition.environment.is_empty()
            || definition.parent.is_some()
        {
            return Err(bad(
                "Task failure changes its complete original mixed declaration scheme",
            ));
        }
        let SemanticDefinitionBody::Nominal {
            kind,
            body: SemanticNominalBody::Product { fields },
            ..
        } = &definition.body
        else {
            return Err(bad("Task failure has another complete original product"));
        };
        if *kind
            != if copy {
                LifeNominalKind::Record
            } else {
                LifeNominalKind::Package
            }
            || fields.len() != 2
            || fields[0].name != "reason"
            || fields[1].name != "inputs"
            || fields[1].ty
                != SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: 0,
                })
        {
            return Err(bad("Task failure changes its original input-return fields"));
        }
        let mut selected = self.environment(module, context, definition.id, active, b)?;
        selected.lexical[0].actuals = vec![Arg::Type(inputs.clone()), Arg::Type(task.clone())];
        let SemanticTypeRecipe::Nominal {
            definition: reason,
            arguments,
        } = &fields[0].ty
        else {
            return Err(bad(
                "Task failure reason lacks its canonical original declaration",
            ));
        };
        if !arguments.is_empty()
            || !matches!(context.definition(module,*reason,b)?.body,SemanticDefinitionBody::Nominal{declaration,..}if declaration==zil_ir::builtin_life::TASK_CREATE_ERROR)
        {
            return Err(bad(
                "Task failure reason selects another complete error identity",
            ));
        }
        Ok(Ty::Nominal(
            definition.id,
            vec![Arg::Type(inputs), Arg::Type(task)],
        ))
    }

    /// An empty input Context is never evidence about an unknown result or a
    /// phantom argument. This proof traverses every original nominal actual.
    pub(super) fn task_zero_domain(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        value: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
        depth: usize,
    ) -> Check<bool> {
        b.spend(1)?;
        if depth >= b.limits.max_call_depth.min(128) {
            return Err(limit("original Task full-domain recursion exceeded"));
        }
        Ok(match self.resolved(value, b)? {
            Ty::Atom(_) | Ty::SealedError(_) | Ty::Function(_) => true,
            Ty::Address {
                life: Life::Static,
                inner,
                ..
            }
            | Ty::Desc {
                life: Life::Static,
                inner,
            } => self.task_zero_domain(module, context, &inner, env, active, b, depth + 1)?,
            Ty::Address { .. }
            | Ty::Desc { .. }
            | Ty::Variable(..)
            | Ty::SelfType(_)
            | Ty::Projection { .. }
            | Ty::Infer(_) => false,
            Ty::Managed(child) | Ty::Store(child) | Ty::Slice(child) | Ty::Array(child, _) => {
                self.task_zero_domain(module, context, &child, env, active, b, depth + 1)?
            }
            Ty::Tuple(fields) => {
                for field in fields {
                    if !self.task_zero_domain(module, context, &field, env, active, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            Ty::ErrorUnion(error, result) => {
                self.task_zero_domain(module, context, &error, env, active, b, depth + 1)?
                    && self.task_zero_domain(module, context, &result, env, active, b, depth + 1)?
            }
            Ty::Nominal(id, args) => {
                for argument in &args {
                    match argument {
                        Arg::Life(_) => return Ok(false),
                        Arg::Type(ty) => {
                            if !self.task_zero_domain(
                                module,
                                context,
                                ty,
                                env,
                                active,
                                b,
                                depth + 1,
                            )? {
                                return Ok(false);
                            }
                        }
                        Arg::Const(c) => {
                            if !self.task_zero_domain(
                                module,
                                context,
                                &c.carrier,
                                env,
                                active,
                                b,
                                depth + 1,
                            )? {
                                return Ok(false);
                            }
                        }
                    }
                }
                let definition = context.definition(module, id, b)?;
                let SemanticDefinitionBody::Nominal { body, .. } = &definition.body else {
                    return Err(bad("Task domain nominal has no complete original body"));
                };
                let mut selected = self.environment(module, context, id, active, b)?;
                selected.lexical[0].actuals = args;
                let fields = match body {
                    SemanticNominalBody::Product { fields } => {
                        fields.iter().map(|f| &f.ty).collect::<Vec<_>>()
                    }
                    SemanticNominalBody::Enum { variants } => variants
                        .iter()
                        .flat_map(|v| v.fields.iter())
                        .collect::<Vec<_>>(),
                };
                b.spend(fields.len())?;
                for field in fields {
                    let field = self.ty(module, context, field, &selected, active, b)?;
                    if !self.task_zero_domain(module, context, &field, env, active, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            Ty::Opaque(owner, args) => {
                for argument in &args {
                    match argument {
                        Arg::Life(_) => return Ok(false),
                        Arg::Type(ty) => {
                            if !self.task_zero_domain(
                                module,
                                context,
                                ty,
                                env,
                                active,
                                b,
                                depth + 1,
                            )? {
                                return Ok(false);
                            }
                        }
                        Arg::Const(c) => {
                            if !self.task_zero_domain(
                                module,
                                context,
                                &c.carrier,
                                env,
                                active,
                                b,
                                depth + 1,
                            )? {
                                return Ok(false);
                            }
                        }
                    }
                }
                let hidden = self.opaque_hidden(module, context, owner, &args, active, b)?;
                self.task_zero_domain(module, context, &hidden, env, active, b, depth + 1)?
            }
            Ty::Carrier {
                child, retained, ..
            } => {
                retained.domain.is_empty()
                    && retained.ports.is_empty()
                    && retained.retained.is_empty()
                    && retained.nominal.is_empty()
                    && self.task_zero_domain(module, context, &child, env, active, b, depth + 1)?
            }
            Ty::Code {
                parameters, result, ..
            } => {
                for p in parameters {
                    if !self.task_zero_domain(module, context, &p, env, active, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                self.task_zero_domain(module, context, &result, env, active, b, depth + 1)?
            }
            Ty::Closure(..) => false,
        })
    }
}

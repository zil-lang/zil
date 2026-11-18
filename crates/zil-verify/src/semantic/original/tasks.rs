//! Universal Task schemes retain whole symbolic input domains. Constructing a
//! frame references a worker; it never executes that worker or its Suspend edge.
use super::*;
use intrinsics::{Intrinsic, Use};

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn return_expression(
        &mut self, module: &TypedModule, context: &CheckedContext,
        expression: &SemanticOriginalExpr, env: &Env, bindings: &BTreeMap<String, Binding>,
        expected: &Ty, active: &mut ActiveQueries, b: &mut Budget,
    ) -> Check<Ty> {
        if let CoreExprKind::Call { callee, arguments } = &expression.kind
            && matches!(&callee.kind, CoreExprKind::Name(name) if name == "task_create")
            && self.intrinsic_name(module, context, callee, env, bindings, b)? == Some(Intrinsic::TaskCreate)
        {
            // Keep the original expression in the worker's coordinates; its
            // exact typed creation will have to prove that physical result.
            let actual = self.expression(module, context, expression, env, bindings, None, active, b)?;
            if let Some(public) = self.project_constructor_return(&actual, expected, arguments, env, bindings, b)? {
                self.relate_value(&public, expected, b)?;
            } else {
                self.relate_value(&actual, expected, b)?;
            }
            return Ok(actual);
        }
        let actual = self.expression(module, context, expression, env, bindings, Some(expected), active, b)?;
        self.relate_value(&actual, expected, b)?;
        Ok(actual)
    }

    fn project_constructor_return(
        &self, actual: &Ty, expected: &Ty, arguments: &[SemanticOriginalExpr],
        env: &Env, bindings: &BTreeMap<String, Binding>, b: &mut Budget,
    ) -> Check<Option<Ty>> {
        fn task_context(ty: &mut Ty) -> Option<&mut Retained> {
            let Ty::ErrorUnion(_, success) = ty else { return None; };
            let Ty::Managed(task) = success.as_mut() else { return None; };
            let Ty::Carrier { kind: CarrierKind::Task, retained, .. } = task.as_mut() else { return None; };
            Some(retained)
        }
        if self.same(actual, expected, b)? {
            return Ok(None);
        }
        let mut projected = self.resolved(actual, b)?;
        let Some(context) = task_context(&mut projected) else { return Ok(None); };
        let owner = env.current()?;
        let mut origins = BTreeMap::new();
        let mut sources = BTreeSet::new();
        let retained = context.retained.iter_mut().map(|row| match row {
            RetainedPart::All { path, .. } | RetainedPart::One { path, .. } => path,
        });
        let nominal = context.nominal.iter_mut().map(|row| match row {
            NominalPart::FromType { path, .. } | NominalPart::One { path, .. } => path,
        });
        for path in retained.chain(nominal) {
            b.spend(path.len().saturating_add(origins.len().checked_ilog2().unwrap_or(0) as usize).saturating_add(1))?;
            let Some(LifeTypeProjection::Parameter(worker)) = path.first_mut() else { continue; };
            if let Some(factory) = origins.get(worker) {
                *worker = *factory;
                continue;
            }
            let Some(argument) = arguments.get(*worker as usize + 1) else { return Ok(None); };
            let CoreExprKind::Name(name) = &argument.kind else { return Ok(None); };
            b.spend(bindings.len().saturating_add(1).saturating_mul(name.len().saturating_add(1)))?;
            let Some(binding) = bindings.get(name) else { return Ok(None); };
            let Binder::Parameter { definition, ordinal } = binding.identity else { return Ok(None); };
            if definition != owner || binding.mutable || !sources.insert(ordinal) {
                return Ok(None);
            }
            origins.insert(*worker, ordinal);
            *worker = ordinal;
        }
        Ok(Some(projected))
    }

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
            let outer_floor = self.next_inference;
            let (worker, worker_environment, worker_actuals, mut prototype) =
                self.task_worker(module, context, &arguments[0], env, bindings, active, b)?;
            self.operation_arity(&arguments[1..], prototype.parameters.len())?;
            if !matches!(prototype.call_kind, CallKind::Direct | CallKind::Suspend) {
                return Err(bad("Task worker has another complete call convention"));
            }
            let outer_heads = self.defer_call_heads(&prototype, &worker_actuals, outer_floor, b)?;
            let index = self.source_index(module, &[env.current()?], b)?;
            let function = index.definitions.get(&env.current()?)
                .ok_or_else(|| bad("Task constructor has no original caller body"))?;
            let Some(index::Node::Expression(call)) = function.nodes.get(&site) else {
                return Err(bad("Task constructor has no exact original Call occurrence"));
            };
            let call_node = call.id;
            self.original_call_site(env.current()?, call_node, b)?;
            let mut inputs = Vec::new();
            b.spend(prototype.parameters.len())?;
            for ((argument, parameter), mode) in arguments[1..]
                .iter()
                .zip(&prototype.parameters)
                .zip(&prototype.modes)
            {
                let parameter =
                    self.normalize_original_type(module, context, parameter, env, active, b)?;
                let actual = self.argument(
                    module, context, argument, &parameter, *mode, env, bindings, active, b,
                )?;
                self.relate_value(&actual, &parameter, b)?;
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

                // The worker receives this exact call's root borrow adapter.
                // The source expression keeps sigma and its invariant child.
                inputs.push(self.task_input_view(&actual, &parameter, b)?);
            }
            self.finish_call_heads(&outer_heads, call_node, b)?;
            self.remember_call_region_parameters(&prototype.parameters, call_node, env, b)?;
            let mut retained = Retained::default();
            for (ordinal, input) in inputs.iter().enumerate() {
                // Expand every complete adapted input domain, including all
                // phantom classes, independently of the worker result.
                if !self.task_zero_domain(module, context, input, env, active, b, 0)? {
                    let classes = vec![ClassPart::All {
                        ty: input.clone(),
                        path: vec![],
                    }];
                    retained.domain.extend(classes.clone());
                    retained.ports.push(PortPart::FromType {
                        ty: input.clone(),
                        classes: classes.clone(),
                    });
                    retained.retained.push(RetainedPart::All {
                        path: vec![LifeTypeProjection::Parameter(ordinal as u32)],
                        parents: classes.clone(),
                    });
                    retained.nominal.push(NominalPart::FromType {
                        path: vec![LifeTypeProjection::Parameter(ordinal as u32)],
                        ty: input.clone(),
                        classes,
                    });
                }
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
            // The common Task constructor below preserves All(Result R),
            // even at N0, without manufacturing a target or frame permission.
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
            let mut context_retained = retained.clone();
            context_retained.retained.retain(|part| match part {
                RetainedPart::One { path, .. } | RetainedPart::All { path, .. } => {
                    path.as_slice() != [LifeTypeProjection::Result]
                }
            });
            // Failure exposes only recovered inputs. Its neutral Unit result
            // marker carries N0, never the unproduced worker result's domain.
            let failure_context=self.task_carrier(Ty::Atom(Type::Unit),context_retained,b)?;
            let task=Ty::Managed(Box::new(self.task_carrier(prototype.result,retained,b)?));
            let error = if kind == Intrinsic::TaskCreateOwned {
                self.task_failure_type(
                    module,
                    context,
                    Ty::Tuple(inputs),
                    failure_context,
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
        self.calls.insert((env.current()?,site),vec![Arg::Type(*child.clone())]);
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

    fn task_input_view(&mut self, actual: &Ty, parameter: &Ty, b: &mut Budget) -> Check<Ty> {
        let actual = self.resolved(actual, b)?;
        let parameter = self.resolved(parameter, b)?;
        match (actual, parameter) {
            (Ty::Address{mutable,inner,..}, Ty::Address{mutable:target,life,inner:expected})
                if mutable == target || mutable && !target => {
                    self.unify(&inner, &expected, b)?;
                    Ok(Ty::Address{mutable:target,life,inner})
                },
            (Ty::Desc{inner,..}, Ty::Desc{life,inner:expected}) => {
                self.unify(&inner, &expected, b)?;
                Ok(Ty::Desc{life,inner})
            },
            (actual, _) => Ok(actual),
        }
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
        let written_slots = Self::written_original_slots(&definition.parameters, types, mixed, b)?;
        let mut selected = self.environment(module, context, worker, active, b)?;
        let mut arguments = Vec::new();
        for (ordinal, formal) in definition.parameters.iter().enumerate() {
            b.spend(1)?;
            let value = if let Some(ty) = types.and_then(|types| written_slots[ordinal].and_then(|index| types.get(index))) {
                Some(Arg::Type(self.ty(module, context, ty, caller, active, b)?))
            } else {
                match mixed.and_then(|mixed| written_slots[ordinal].and_then(|index| mixed.get(index))) {
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
                // The checked opaque Result role supplies its actual creator;
                // an ambient caller with the same type shape does not. Only a
                // raw original Result that is itself this child Closure may
                // select that owner domain (not a Closure injected through T).
                let mut creator=self.environment(module,context,owner,active,b)?;
                creator.lexical[0].actuals=self.resolve_arguments(&args,0,b)?;
                let result_role=SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Result};
                if self.closure_source_creator(module,context,&result_role,&hidden,&creator,b)?{
                    self.task_zero_domain(module,context,&hidden,&creator,active,b,depth+1)?
                }else{
                    self.task_zero_domain(module, context, &hidden, env, active, b, depth + 1)?
                }
            }
            Ty::Carrier {
                child, retained, ..
            } => {
                self.retained_zero_domain(module,context,&retained,env,active,b,depth+1)?
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
            Ty::Dyn(value)=>{let mut types=Vec::new();value.types(&mut types,b)?;if value.explicit_life(b)?{return Ok(false);}for ty in types{if !self.task_zero_domain(module,context,ty,env,active,b,depth+1)?{return Ok(false);}}true},
            Ty::Closure(owner, arguments, sealed) => {
                // This is a complete original type-domain proof, not a CE or
                // literal-root premise. In particular an empty physical capture
                // list says nothing about free signature or phantom classes.
                if !self.checked.contains(&owner) { return Ok(false); }
                for argument in arguments.iter().chain(&sealed) {
                    let ty=match argument {
                        Arg::Life(_)=>return Ok(false),
                        Arg::Type(ty)=>ty,
                        Arg::Const(value)=>value.carrier.as_ref(),
                    };
                    if !self.task_zero_domain(module,context,ty,env,active,b,depth+1)? { return Ok(false); }
                }
                let declaration=context.definition(module,owner,b)?;
                let Some(parent)=declaration.parent else { return Ok(false); };
                b.spend(env.lexical.len())?;
                if !env.lexical.iter().any(|frame|frame.definition==parent) {
                    // A missing real creator is not permission to reconstruct
                    // its formal world from the closure's physical shape.
                    return Ok(false);
                }
                let selected=self.closure_creator_environment(module,context,owner,&arguments,&sealed,env,active,b)?;
                let captures=self.closure_capture_fields(module,context,owner,&arguments,&sealed,b)?;
                for (name,capture) in captures {
                    if !self.capture_field_owns(owner,&name,b)? || !self.task_zero_domain(module,context,&capture,&selected,active,b,depth+1)? { return Ok(false); }
                }
                let Some(prototype)=self.prototypes.get(&owner) else { return Ok(false); };
                // The complete raw prototype is substituted simultaneously
                // exactly once. Invocation roles are neither erased nor freshened.
                let signature=self.specialize_code_prototype(prototype,&selected,b)?;
                for parameter in &signature.parameters {
                    if !self.task_zero_domain(module,context,parameter,&signature.environment,active,b,depth+1)? { return Ok(false); }
                }
                self.task_zero_domain(module,context,&signature.result,&signature.environment,active,b,depth+1)?
            },
        })
    }
}

#[cfg(test)]
mod task_input_role_tests {
    use super::*;
    fn address(life: Life, child: Ty) -> Ty {
        Ty::Address { mutable: true, life, inner: Box::new(child) }
    }
    #[test]
    fn a_task_retains_its_worker_adapter_and_every_original_source_without_retagging_children() {
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};
        let mut original=CheckedOriginal::default();original.current=Some(owner);
        let mut b=Budget::new(&Limits::default());
        let fresh=original.fresh_life(&mut b).unwrap();let Life::Infer(id)=fresh else{unreachable!()};
        original.outer_inferences.insert(id,vec![]);
        let rigid=Life::Formal(owner,0);
        let child=Ty::Nominal(owner,vec![Arg::Life(rigid.clone())]);
        let first=address(Life::Local(owner,NodeId(10)),child.clone());
        let second=address(Life::Local(owner,NodeId(11)),child.clone());
        let parameter=address(fresh.clone(),child.clone());
        original.relate_value(&first,&parameter,&mut b).unwrap();
        original.relate_value(&second,&parameter,&mut b).unwrap();
        let input=original.task_input_view(&first,&parameter,&mut b).unwrap();
        original.finish_call_heads(&[(id,vec![1,0],false)],NodeId(12),&mut b).unwrap();
        assert_eq!(original.resolved(&input,&mut b).unwrap(),address(Life::CallRegion(id),child.clone()));
        assert_eq!(original.call_regions[&id].3,vec![Life::Local(owner,NodeId(10)),Life::Local(owner,NodeId(11))]);
        assert_eq!(original.resolved(&first,&mut b).unwrap(),first,"the original expression keeps sigma");
        assert!(!original.same_life(&Life::CallRegion(id),&rigid,&mut b).unwrap());
        let changed=address(fresh,Ty::Nominal(owner,vec![Arg::Life(Life::Formal(owner,1))]));
        assert!(original.task_input_view(&first,&changed,&mut b).is_err(),"a root adapter cannot rename a nominal child lifetime");
        let shared=Ty::Address{mutable:false,life:rigid.clone(),inner:Box::new(Ty::Atom(Type::I32))};
        assert!(original.relate_value(&shared,&address(rigid,Ty::Atom(Type::I32)),&mut b).is_err(),"shared source cannot supply an exclusive worker input");
        assert!(original.relate_outer(&Life::CallRegion(id),&Life::Formal(owner,1),&mut b).is_err(),"a frame cannot export an unsupported rigid lifetime");
    }
}

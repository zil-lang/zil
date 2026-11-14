//! Complete sealed language schemes, independently matched to real opcodes.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IntrinsicKind {
    DynCall {
        declaration: SemanticDefinitionId,
        parent: SemanticDefinitionId,
        member: u32,
        receiver_mode: PassingMode,
        parameter_modes: Vec<PassingMode>,
        result_mode: PassingMode,
        call_kind: CallKind,
    },
    TaskCreate {
        worker: SemanticDefinitionId,
        owned: bool,
        await_result: bool,
    },
    TaskYield,
    TaskOperation(TaskOp),
    RegistrationStart {
        providers: Vec<SemanticDefinitionId>,
        asynchronous: bool,
        threaded_capable: bool,
    },
    RegistrationOperation(RegistrationOp),
    OwnedDyn,
    NominalConstructor {
        definition: SemanticDefinitionId,
        variant: u32,
    },
    MutexCreate,
    MutexLock,
    MutexValue,
    MutexUnlock,
    StoreNew,
    StoreReserve,
    StorePush,
    StorePop,
    StoreLen,
    StoreCapacity,
    StoreBorrow {
        mutable: bool,
    },
    BytesNew,
    BytesReserve,
    BytesTruncate,
    BytesPush,
    BytesLen,
    BytesCapacity,
    BytesBorrow {
        mutable: bool,
    },
    Length,
    Numeric {
        target: Type,
    },
    ThreadScope {
        worker: SemanticDefinitionId,
        body: SemanticDefinitionId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskOp {
    Valid,
    Poll,
    RequestCancel,
    PollClose,
    FinishDestroy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationOp {
    Valid,
    Close,
    AsyncValid,
    AsyncRequest,
    AsyncPollClose,
    AsyncFinish,
    AsyncFinishChecked,
}
#[derive(Clone)]
pub(crate) struct IntrinsicCall {
    pub kind: IntrinsicKind,
    pub parameters: Vec<SemanticTypeRecipe>,
    pub result: SemanticTypeRecipe,
    pub effect: CallKind,
    pub worker_scheme: Option<(SemanticSignatureRecipe, Vec<SemanticActualRecipe>)>,
}

impl Check<'_, '_> {
    pub(super) fn intrinsic(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(1, depth)?;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            unreachable!()
        };
        let path = self.declaration_path(callee, variables, depth + 1)?;
        if let Some(path) = &path {
            if self.declaration_target(owner, path, depth + 1)?.is_some() {
                return Ok(None);
            }
        }
        if let Some(result) =
            self.store_intrinsic(owner, site, expression, variables, wanted, depth + 1)?
        {
            return Ok(Some(result));
        }
        if let Some(result) =
            self.execution_intrinsic(owner, site, expression, variables, wanted, depth + 1)?
        {
            return Ok(Some(result));
        }
        let mut base = callee.as_ref();
        while let CoreExprKind::GenericApply { callee, .. }
        | CoreExprKind::TypeApply { callee, .. } = &base.kind
        {
            self.pay(1, depth)?;
            base = callee;
        }
        // A builtin is selected only after exact lexical/declaration resolution;
        // a matching spelling hidden by a local remains an ordinary callable.
        let numeric = |name: &str| match name {
            "i8" => Some(Type::I8),
            "i16" => Some(Type::I16),
            "i32" => Some(Type::I32),
            "i64" => Some(Type::I64),
            "isize" => Some(Type::Isize),
            "u8" => Some(Type::U8),
            "u16" => Some(Type::U16),
            "u32" => Some(Type::U32),
            "u64" => Some(Type::U64),
            "usize" => Some(Type::Usize),
            "f32" => Some(Type::F32),
            "f64" => Some(Type::F64),
            _ => None,
        };
        let (kind, arity, receiver) = match (&base.kind, &path) {
            (CoreExprKind::Name(name), Some(path)) if path.len() == 1 => match name.as_str() {
                "mutex_create" => (IntrinsicKind::MutexCreate, 1, None),
                "mutex_lock" => (IntrinsicKind::MutexLock, 1, None),
                "mutex_value" => (IntrinsicKind::MutexValue, 1, None),
                "mutex_unlock" => (IntrinsicKind::MutexUnlock, 1, None),
                "bytes_new" => (IntrinsicKind::BytesNew, 0, None),
                "bytes_reserve" => (IntrinsicKind::BytesReserve, 2, None),
                "bytes_truncate" => (IntrinsicKind::BytesTruncate, 2, None),
                "bytes_push" => (IntrinsicKind::BytesPush, 2, None),
                "bytes_len" => (IntrinsicKind::BytesLen, 1, None),
                "bytes_capacity" => (IntrinsicKind::BytesCapacity, 1, None),
                "bytes_view" => (IntrinsicKind::BytesBorrow { mutable: false }, 1, None),
                "bytes_view_mut" => (IntrinsicKind::BytesBorrow { mutable: true }, 1, None),
                "thread_scope" => {
                    if arguments.len() != 3 {
                        return Err(bad(
                            "original thread_scope changes its three-operand scheme",
                        ));
                    }
                    let worker_path = self
                        .declaration_path(&arguments[0], variables, depth + 1)?
                        .ok_or_else(|| {
                            bad("original thread worker is not a lexical static declaration")
                        })?;
                    let worker = self
                        .declaration_target(owner, &worker_path, depth + 1)?
                        .ok_or_else(|| bad("original thread worker has no real declaration"))?;
                    let worker_definition = self.definition(worker)?;
                    if worker_definition.kind != SemanticDefinitionKind::Function {
                        return Err(bad(
                            "original thread worker must be a real ordinary declaration",
                        ));
                    }
                    if !self.proof.complete.contains(&worker) && !self.active.contains(&worker) {
                        self.function(worker_definition, None)?;
                    }
                    let SemanticDefinitionBody::FunctionWithBody {
                        signature,
                        body: SemanticOriginalFunctionBody::Defined(_),
                        ..
                    } = &worker_definition.body
                    else {
                        return Err(bad("original thread worker must have a real language body"));
                    };
                    if signature.parameters.len() != 1 || signature.call_kind != CallKind::Direct {
                        return Err(bad(
                            "original thread worker has an incompatible call scheme",
                        ));
                    }
                    let written = self.written(
                        owner,
                        &arguments[0],
                        &worker_definition.parameters,
                        variables,
                        depth + 1,
                    )?;
                    self.pay(written.len(), depth)?;
                    let written = written
                        .into_iter()
                        .map(|actual| actual.unwrap_or(SemanticActualRecipe::Infer))
                        .collect::<Vec<_>>();
                    let (worker_actuals, worker_signature) = self.solve_signature(
                        owner,
                        &arguments[0],
                        &[&arguments[1]],
                        signature,
                        &worker_definition.parameters,
                        &written,
                        variables,
                        None,
                        false,
                        depth + 1,
                    )?;
                    self.call_requirements(owner, worker_definition, &worker_actuals, depth + 1)?;
                    self.same(
                        &worker_signature.result,
                        &SemanticTypeRecipe::Atom(Type::I32),
                    )?;
                    let payload =
                        copy_recipe(&worker_signature.parameters[0].ty, self.limits, self.left)?;
                    let body_ty = self.expr(owner, &arguments[2], variables, None, depth + 1)?;
                    let SemanticTypeRecipe::Closure {
                        definition: body, ..
                    } = &body_ty
                    else {
                        return Err(bad("original thread body is not a concrete closure"));
                    };
                    let body_definition = self.definition(*body)?;
                    let SemanticDefinitionBody::FunctionWithBody {
                        signature: body_signature,
                        ..
                    } = &body_definition.body
                    else {
                        return Err(bad("original thread body omits its complete scheme"));
                    };
                    if !body_signature.parameters.is_empty() {
                        return Err(bad("original thread body requires a zero-argument scheme"));
                    }
                    let expected_body = SemanticSignatureRecipe {
                        bound_lifetimes: 0,
                        invocation_lifetimes: 0,
                        parameters: Vec::new(),
                        result: SemanticTypeRecipe::Atom(Type::I32),
                        result_mode: PassingMode::Infer,
                        call_kind: CallKind::Direct,
                    };
                    self.callable_requirement(
                        owner,
                        &body_ty,
                        CallableKind::FnOnce,
                        &expected_body,
                        depth + 1,
                    )?;
                    let result = SemanticTypeRecipe::Tuple(vec![
                        SemanticTypeRecipe::Atom(Type::Bool),
                        SemanticTypeRecipe::Atom(Type::I32),
                        SemanticTypeRecipe::Atom(Type::I32),
                    ]);
                    let kind = IntrinsicKind::ThreadScope {
                        worker,
                        body: *body,
                    };
                    self.record_intrinsic(
                        owner,
                        site,
                        kind,
                        vec![payload, body_ty],
                        &result,
                        wanted,
                        depth + 1,
                    )?;
                    self.proof
                        .intrinsics
                        .get_mut(&(owner, site))
                        .ok_or_else(|| bad("thread scheme receipt disappeared"))?
                        .worker_scheme = Some((worker_signature, worker_actuals));
                    return Ok(Some(result));
                }
                name => match numeric(name) {
                    Some(target) => (IntrinsicKind::Numeric { target }, 1, None),
                    None => return Ok(None),
                },
            },
            (CoreExprKind::Member { object, member, .. }, None) if member == "len" => {
                let receiver = self.place(owner, object, variables, depth + 1)?;
                let mut receiver = self.observation_type(owner, &receiver, depth + 1)?;
                while let SemanticTypeRecipe::Address { inner, .. } = receiver {
                    self.pay(1, depth)?;
                    receiver = *inner;
                }
                if !matches!(
                    receiver,
                    SemanticTypeRecipe::Array { .. } | SemanticTypeRecipe::Slice(_)
                ) {
                    return Ok(None);
                }
                (IntrinsicKind::Length, 0, Some(object.as_ref()))
            }
            _ => return Ok(None),
        };
        if base.id != callee.id {
            return Err(bad(
                "original sealed intrinsic has no written generic argument scheme",
            ));
        }
        if arguments.len() != arity {
            return Err(bad("original intrinsic changes its declared operand arity"));
        }
        self.pay(arguments.len().saturating_add(8), depth)?;
        let mut parameters = Vec::new();
        if let Some(receiver) = receiver {
            parameters.push(self.place(owner, receiver, variables, depth + 1)?);
        }
        for (ordinal, argument) in arguments.iter().enumerate() {
            let expected = match (&kind, ordinal) {
                (IntrinsicKind::BytesReserve | IntrinsicKind::BytesTruncate, 1) => {
                    Some(SemanticTypeRecipe::Atom(Type::I64))
                }
                (IntrinsicKind::BytesPush, 1) => Some(SemanticTypeRecipe::Atom(Type::U8)),
                _ => None,
            };
            parameters.push(self.expr(owner, argument, variables, expected.as_ref(), depth + 1)?);
        }
        let result = match &kind {
            IntrinsicKind::MutexCreate => {
                let payload = copy_recipe(&parameters[0], self.limits, self.left)?;
                if !self.ordinary_value(owner, &payload, depth + 1)? {
                    return Err(bad(
                        "original mutex payload lacks its complete ordinary Value premise",
                    ));
                }
                SemanticTypeRecipe::ErrorUnion {
                    error: Box::new(SemanticTypeRecipe::Exact(Box::new(
                        SemanticExactTypeRecipe {
                            ty: crate::generic_mutex::create_error(),
                            life: StorageLifeShape {
                                binder_count: 0,
                                slots: Vec::new(),
                                retained: Vec::new(),
                                nominal_arguments: Vec::new(),
                            },
                            classes: Vec::new(),
                        },
                    ))),
                    result: Box::new(SemanticTypeRecipe::Managed(Box::new(
                        SemanticTypeRecipe::Mutex(Box::new(payload)),
                    ))),
                }
            }
            IntrinsicKind::MutexLock => {
                let SemanticTypeRecipe::Desc {
                    lifetime: SemanticOuterLifeRecipe::Class(lifetime),
                    inner,
                } = &parameters[0]
                else {
                    return Err(bad(
                        "original mutex lock requires a real scoped head descriptor",
                    ));
                };
                let SemanticTypeRecipe::Mutex(payload) = inner.as_ref() else {
                    return Err(bad("original mutex lock changes the sealed head kind"));
                };
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                SemanticTypeRecipe::MutexGuard {
                    payload: Box::new(copy_recipe(payload, self.limits, self.left)?),
                    retained: Box::new(SemanticRetainedShapeRecipe {
                        domain: SemanticClassSequenceRecipe {
                            parts: vec![SemanticClassSequencePart::One(lifetime.clone())],
                        },
                        ports: vec![SemanticPortPart::One {
                            leaf: Box::new(copy_recipe(inner, self.limits, self.left)?),
                            mode: StoredBorrowMode::SharedAddress,
                            class: lifetime.clone(),
                        }],
                        retained: Vec::new(),
                        nominal: Vec::new(),
                    }),
                }
            }
            IntrinsicKind::MutexValue => {
                let SemanticTypeRecipe::Address {
                    mutable: true,
                    lifetime,
                    inner,
                } = &parameters[0]
                else {
                    return Err(bad(
                        "original mutex value requires an exclusive guard borrow",
                    ));
                };
                let SemanticTypeRecipe::MutexGuard { payload, .. } = inner.as_ref() else {
                    return Err(bad("original mutex value changes its sealed guard kind"));
                };
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                SemanticTypeRecipe::Address {
                    mutable: true,
                    lifetime: lifetime.clone(),
                    inner: Box::new(copy_recipe(payload, self.limits, self.left)?),
                }
            }
            IntrinsicKind::MutexUnlock => {
                if !matches!(&parameters[0], SemanticTypeRecipe::MutexGuard { .. }) {
                    return Err(bad("original mutex unlock must consume a real guard"));
                }
                SemanticTypeRecipe::Atom(Type::Unit)
            }
            IntrinsicKind::BytesNew => {
                SemanticTypeRecipe::Managed(Box::new(SemanticTypeRecipe::Atom(Type::ByteBuffer)))
            }
            IntrinsicKind::BytesReserve
            | IntrinsicKind::BytesTruncate
            | IntrinsicKind::BytesPush
            | IntrinsicKind::BytesLen
            | IntrinsicKind::BytesCapacity
            | IntrinsicKind::BytesBorrow { .. } => {
                let SemanticTypeRecipe::Desc { lifetime, inner } = &parameters[0] else {
                    return Err(bad("original bytes operation requires a scoped descriptor"));
                };
                self.same(inner, &SemanticTypeRecipe::Atom(Type::ByteBuffer))?;
                if parameters.len() == 2 {
                    self.same(
                        &parameters[1],
                        &SemanticTypeRecipe::Atom(if kind == IntrinsicKind::BytesPush {
                            Type::U8
                        } else {
                            Type::I64
                        }),
                    )?;
                }
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                match &kind {
                    IntrinsicKind::BytesLen | IntrinsicKind::BytesCapacity => {
                        SemanticTypeRecipe::Atom(Type::I64)
                    }
                    IntrinsicKind::BytesBorrow { mutable } => SemanticTypeRecipe::Address {
                        mutable: *mutable,
                        lifetime: lifetime.clone(),
                        inner: Box::new(SemanticTypeRecipe::Slice(Box::new(
                            SemanticTypeRecipe::Atom(Type::U8),
                        ))),
                    },
                    _ => SemanticTypeRecipe::Atom(Type::Bool),
                }
            }
            IntrinsicKind::Length => {
                let mut value = &parameters[0];
                while let SemanticTypeRecipe::Address { inner, .. } = value {
                    self.pay(1, depth)?;
                    value = inner;
                }
                if !matches!(
                    value,
                    SemanticTypeRecipe::Array { .. } | SemanticTypeRecipe::Slice(_)
                ) {
                    return Err(bad(
                        "original len receiver is not an array or borrowed slice",
                    ));
                }
                SemanticTypeRecipe::Atom(Type::I64)
            }
            IntrinsicKind::Numeric { target } => {
                if !matches!(&parameters[0], SemanticTypeRecipe::Atom(ty) if ty.is_integer() || ty.is_float())
                {
                    return Err(bad(
                        "original numeric intrinsic requires a primitive numeric input",
                    ));
                }
                SemanticTypeRecipe::Atom(target.clone())
            }
            IntrinsicKind::ThreadScope { .. }
            | IntrinsicKind::TaskCreate { .. }
            | IntrinsicKind::DynCall { .. }
            | IntrinsicKind::TaskYield
            | IntrinsicKind::TaskOperation(_)
            | IntrinsicKind::RegistrationStart { .. }
            | IntrinsicKind::RegistrationOperation(_)
            | IntrinsicKind::OwnedDyn
            | IntrinsicKind::NominalConstructor { .. }
            | IntrinsicKind::StoreNew
            | IntrinsicKind::StoreReserve
            | IntrinsicKind::StorePush
            | IntrinsicKind::StorePop
            | IntrinsicKind::StoreLen
            | IntrinsicKind::StoreCapacity
            | IntrinsicKind::StoreBorrow { .. } => unreachable!(),
        };
        self.record_intrinsic(owner, site, kind, parameters, &result, wanted, depth + 1)?;
        Ok(Some(result))
    }
    pub(super) fn record_intrinsic(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        kind: IntrinsicKind,
        parameters: Vec<SemanticTypeRecipe>,
        result: &SemanticTypeRecipe,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<()> {
        self.pay(2, depth)?;
        if let Some(wanted) = wanted {
            self.same(result, wanted)?;
        }
        self.pay(2, depth)?;
        let mut actuals = Vec::new();
        match &kind {
            IntrinsicKind::MutexCreate => actuals.push(SemanticActualRecipe::Type(copy_recipe(
                &parameters[0],
                self.limits,
                self.left,
            )?)),
            IntrinsicKind::MutexLock => {
                let SemanticTypeRecipe::Desc { inner, .. } = &parameters[0] else {
                    unreachable!()
                };
                let SemanticTypeRecipe::Mutex(payload) = inner.as_ref() else {
                    unreachable!()
                };
                actuals.push(SemanticActualRecipe::Type(copy_recipe(
                    payload,
                    self.limits,
                    self.left,
                )?));
            }
            IntrinsicKind::MutexValue => {
                let SemanticTypeRecipe::Address { inner, .. } = &parameters[0] else {
                    unreachable!()
                };
                let SemanticTypeRecipe::MutexGuard { payload, .. } = inner.as_ref() else {
                    unreachable!()
                };
                actuals.push(SemanticActualRecipe::Type(copy_recipe(
                    payload,
                    self.limits,
                    self.left,
                )?));
            }
            IntrinsicKind::MutexUnlock => {
                let SemanticTypeRecipe::MutexGuard { payload, .. } = &parameters[0] else {
                    unreachable!()
                };
                actuals.push(SemanticActualRecipe::Type(copy_recipe(
                    payload,
                    self.limits,
                    self.left,
                )?));
            }
            IntrinsicKind::ThreadScope { .. } => {
                for parameter in &parameters {
                    actuals.push(SemanticActualRecipe::Type(copy_recipe(
                        parameter,
                        self.limits,
                        self.left,
                    )?));
                }
            }
            _ => {}
        }
        self.proof.calls.insert((owner, site), actuals);
        let result = copy_recipe(result, self.limits, self.left)?;
        let effect = match &kind {
            IntrinsicKind::DynCall { call_kind, .. } => *call_kind,
            IntrinsicKind::TaskYield
            | IntrinsicKind::TaskCreate {
                await_result: true, ..
            } => CallKind::Suspend,
            _ => CallKind::Direct,
        };
        self.proof.intrinsics.insert(
            (owner, site),
            IntrinsicCall {
                kind,
                parameters,
                result,
                effect,
                worker_scheme: None,
            },
        );
        Ok(())
    }
}

impl Check<'_, '_> {
    /// Authenticate the entire compiler-distributed original declaration,
    /// including unused parameters and arms, before it can carry a scheme.
    pub(super) fn canonical_carrier(
        &mut self,
        definition: &SemanticDefinition,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        let SemanticDefinitionBody::Nominal {
            declaration,
            kind,
            body,
        } = &definition.body
        else {
            return Ok(());
        };
        use zil_ir::builtin_life as builtin;
        let (module, family, name, arity) = if *declaration == builtin::OPTION {
            ("option", "enum", "Option", 1)
        } else if *declaration == builtin::POLL {
            ("task", "enum", "Poll", 1)
        } else if *declaration == builtin::TASK_CREATE_ERROR {
            ("task", "enum", "TaskCreateError", 0)
        } else if *declaration == builtin::TASK_POLL_STATE {
            ("task", "enum", "TaskPollState", 0)
        } else if *declaration == builtin::TASK_CREATE_FAILURE_RECORD {
            ("task", "record", "TaskCreateFailure", 2)
        } else if *declaration == builtin::TASK_CREATE_FAILURE_PACKAGE {
            ("task", "package", "TaskCreateFailure", 2)
        } else {
            return Ok(());
        };
        crate::dyn_metadata::semantic_operand_snapshot(definition, self.limits, self.left, 1)?;
        if definition.kind != SemanticDefinitionKind::Nominal
            || definition.parent.is_some()
            || !definition.environment.is_empty()
            || !definition.implicit_lifetimes.is_empty()
            || !definition.requirements.is_empty()
            || definition.parameters.len() != arity
            || definition
                .parameters
                .iter()
                .any(|p| !matches!(p,SemanticFormal::Type{requirements}if requirements.is_empty()))
            || definition.identity.package != "@zil.builtin"
            || definition.identity.module_path.as_slice() != [module]
            || definition.identity.declaration_path.as_slice() != [family, name]
            || !definition.identity.generation_path.is_empty()
        {
            return Err(bad(
                "compiler carrier changes its complete original identity or mixed scheme",
            ));
        }
        let formal = SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0,
        });
        if family == "enum" {
            let SemanticNominalBody::Enum { variants } = body else {
                return Err(bad("compiler carrier changes its original enum body"));
            };
            if *kind != LifeNominalKind::Enum {
                return Err(bad("compiler carrier changes its original nominal kind"));
            }
            let names: &[&str] = if *declaration == builtin::OPTION {
                &["None", "Some"]
            } else if *declaration == builtin::POLL {
                &["Pending", "Ready"]
            } else if *declaration == builtin::TASK_CREATE_ERROR {
                &["Allocation"]
            } else {
                &["Complete", "Cancelled", "Invalid"]
            };
            if variants.len() != names.len() {
                return Err(bad(
                    "compiler carrier changes its complete original variants",
                ));
            }
            for (ordinal, (variant, name)) in variants.iter().zip(names).enumerate() {
                let payload = arity == 1 && ordinal == 1;
                if variant.name != *name
                    || (payload && variant.fields.as_slice() != [formal.clone()])
                    || (!payload && !variant.fields.is_empty())
                {
                    return Err(bad(
                        "compiler carrier changes its original variant payload scheme",
                    ));
                }
            }
        } else {
            let expected_kind = if family == "package" {
                LifeNominalKind::Package
            } else {
                LifeNominalKind::Record
            };
            let SemanticNominalBody::Product { fields } = body else {
                return Err(bad(
                    "Task failure carrier changes its original product body",
                ));
            };
            let [reason, inputs] = fields.as_slice() else {
                return Err(bad(
                    "Task failure carrier changes its complete input fields",
                ));
            };
            if *kind != expected_kind
                || reason.name != "reason"
                || inputs.name != "inputs"
                || inputs.ty != formal
            {
                return Err(bad(
                    "Task failure carrier changes its original input tuple or resource mode",
                ));
            }
            let SemanticTypeRecipe::Nominal {
                definition: error,
                arguments,
            } = &reason.ty
            else {
                return Err(bad(
                    "Task failure reason is not the canonical original error",
                ));
            };
            if !arguments.is_empty() {
                return Err(bad(
                    "Task failure reason changes its original mixed arguments",
                ));
            }
            let error = self.definition(*error)?;
            if !matches!(error.body,SemanticDefinitionBody::Nominal{declaration,..}if declaration==builtin::TASK_CREATE_ERROR)
            {
                return Err(bad(
                    "Task failure carrier changes its original reason declaration",
                ));
            }
            self.canonical_carrier(error, depth + 1)?;
        }
        self.pay(1, depth)
    }
    fn compiler_carrier(
        &mut self,
        declaration: LifeDeclarationId,
        arguments: Vec<SemanticActualRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(
            self.program
                .definitions
                .len()
                .saturating_add(arguments.len()),
            depth,
        )?;
        let mut found = None;
        for definition in &self.program.definitions {
            if matches!(definition.body,SemanticDefinitionBody::Nominal{declaration:id,..}if id==declaration)
            {
                if found.replace(definition).is_some() {
                    return Err(bad(
                        "sealed carrier has ambiguous real original declarations",
                    ));
                }
            }
        }
        let definition =
            found.ok_or_else(|| bad("sealed carrier omits its real compiler declaration"))?;
        self.canonical_carrier(definition, depth + 1)?;
        if definition.parameters.len() != arguments.len() {
            return Err(bad(
                "sealed carrier changes its full original generic arity",
            ));
        }
        Ok(SemanticTypeRecipe::Nominal {
            definition: definition.id,
            arguments,
        })
    }
    fn poll_carrier(
        &mut self,
        payload: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let payload = copy_recipe(payload, self.limits, self.left)?;
        self.compiler_carrier(
            zil_ir::builtin_life::POLL,
            vec![SemanticActualRecipe::Type(payload)],
            depth + 1,
        )
    }
    fn empty_task_shape(
        &mut self,
        parameters: &[SemanticParameterRecipe],
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticRetainedShapeRecipe> {
        let mut shape = SemanticRetainedShapeRecipe {
            domain: SemanticClassSequenceRecipe { parts: Vec::new() },
            ports: Vec::new(),
            retained: Vec::new(),
            nominal: Vec::new(),
        };
        self.pay(parameters.len().saturating_add(4), depth)?;
        for parameter in parameters {
            if !matches!(parameter.ty, SemanticTypeRecipe::Address { .. })
                && self.sealed_capability(
                    self.contexts
                        .last()
                        .copied()
                        .ok_or_else(|| bad("Task scheme omits original caller"))?,
                    &parameter.ty,
                    SemanticBuiltinTrait::ClosedEnvironment,
                    depth + 1,
                )? != Some(true)
            {
                return Err(pending(
                    "Task input requires its complete original retained input class graph",
                ));
            }
            if let SemanticTypeRecipe::Address { inner, .. } = &parameter.ty {
                if self.sealed_capability(
                    self.contexts
                        .last()
                        .copied()
                        .ok_or_else(|| bad("Task scheme omits original caller"))?,
                    inner,
                    SemanticBuiltinTrait::ClosedEnvironment,
                    depth + 1,
                )? != Some(true)
                {
                    return Err(pending(
                        "Task borrowed input requires its complete nested original class graph",
                    ));
                }
            }
            if let SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(lifetime),
                ..
            } = &parameter.ty
            {
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                if !shape
                    .domain
                    .parts
                    .iter()
                    .any(|part| matches!(part,SemanticClassSequencePart::One(old)if old==lifetime))
                {
                    shape
                        .domain
                        .parts
                        .push(SemanticClassSequencePart::One(lifetime.clone()));
                }
                let leaf = copy_recipe(&parameter.ty, self.limits, self.left)?;
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                shape.ports.push(SemanticPortPart::One {
                    leaf: Box::new(leaf),
                    mode: if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    class: lifetime.clone(),
                });
            }
        }
        if let SemanticTypeRecipe::Address {
            mutable: false,
            lifetime: SemanticOuterLifeRecipe::Class(lifetime),
            ..
        } = result
        {
            if !shape
                .domain
                .parts
                .iter()
                .any(|part| matches!(part,SemanticClassSequencePart::One(old)if old==lifetime))
            {
                return Err(bad(
                    "original Task borrowed result has no exact real input lifetime role",
                ));
            }
            crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
            shape.retained.push(SemanticRetainedPart::One {
                path: vec![LifeTypeProjection::Result],
                child_class: 0,
                parent: lifetime.clone(),
            });
        } else if self.sealed_capability(
            self.contexts
                .last()
                .copied()
                .ok_or_else(|| bad("Task scheme has no original caller context"))?,
            result,
            SemanticBuiltinTrait::ClosedEnvironment,
            depth + 1,
        )? != Some(true)
        {
            return Err(pending(
                "Task result requires its complete symbolic retained Result class graph",
            ));
        }
        Ok(shape)
    }
    fn task_failure(
        &mut self,
        owner: SemanticDefinitionId,
        parameters: &[SemanticParameterRecipe],
        shape: &SemanticRetainedShapeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(parameters.len().saturating_add(4), depth)?;
        let mut inputs = Vec::new();
        let mut owns = false;
        for parameter in parameters {
            owns |= self.task_input_owns(owner, &parameter.ty, depth + 1)?;
            inputs.push(copy_recipe(&parameter.ty, self.limits, self.left)?);
        }
        crate::dyn_metadata::semantic_operand_snapshot(shape, self.limits, self.left, 1)?;
        let mut context = shape.clone();
        context.retained.clear();
        self.compiler_carrier(
            if owns {
                zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
            } else {
                zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
            },
            vec![
                SemanticActualRecipe::Type(SemanticTypeRecipe::Tuple(inputs)),
                SemanticActualRecipe::Type(SemanticTypeRecipe::Task {
                    result: Box::new(SemanticTypeRecipe::Atom(Type::Unit)),
                    retained: Box::new(context),
                }),
            ],
            depth + 1,
        )
    }
    fn task_input_owns(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Managed(_)
            | SemanticTypeRecipe::Closure { .. }
            | SemanticTypeRecipe::Dyn { owned: true, .. } => true,
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => false,
            SemanticTypeRecipe::Address { .. }
            | SemanticTypeRecipe::Desc { .. }
            | SemanticTypeRecipe::Dyn { owned: false, .. } => false,
            SemanticTypeRecipe::Tuple(fields) => {
                self.pay(fields.len(), depth)?;
                let mut owns = false;
                for field in fields {
                    owns |= self.task_input_owns(owner, field, depth + 1)?;
                }
                owns
            }
            SemanticTypeRecipe::Array { element, .. } => {
                self.task_input_owns(owner, element, depth + 1)?
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.task_input_owns(owner, error, depth + 1)?
                    | self.task_input_owns(owner, result, depth + 1)?
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let definition = self.definition(*definition)?;
                match &definition.body {
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Package,
                        ..
                    } => true,
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } => {
                        self.pay(fields.len(), depth)?;
                        let mut owns = false;
                        for field in fields {
                            let field = super::super::instantiate::ty(
                                self.program,
                                &self.proof,
                                &field.ty,
                                super::super::instantiate::DomainMapping {
                                    source: definition.id,
                                    use_definition: owner,
                                    arguments,
                                    parent_call: None,
                                },
                                self.limits,
                                self.left,
                            )?;
                            owns |= self.task_input_owns(owner, &field, depth + 1)?;
                        }
                        owns
                    }
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } => {
                        self.pay(variants.len(), depth)?;
                        let mut owns = false;
                        for variant in variants {
                            self.pay(variant.fields.len(), depth)?;
                            for field in &variant.fields {
                                let field = super::super::instantiate::ty(
                                    self.program,
                                    &self.proof,
                                    field,
                                    super::super::instantiate::DomainMapping {
                                        source: definition.id,
                                        use_definition: owner,
                                        arguments,
                                        parent_call: None,
                                    },
                                    self.limits,
                                    self.left,
                                )?;
                                owns |= self.task_input_owns(owner, &field, depth + 1)?;
                            }
                        }
                        owns
                    }
                    _ => return Err(bad("Task failure input lacks its original nominal body")),
                }
            }
            _ => {
                return Err(pending(
                    "Task failure input ownership requires a complete original structural premise",
                ))
            }
        })
    }
    fn static_provider(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticDefinitionId> {
        let path = self
            .declaration_path(expression, variables, depth + 1)?
            .ok_or_else(|| bad("sealed operation requires a lexical static provider"))?;
        self.declaration_target(owner, &path, depth + 1)?
            .ok_or_else(|| bad("sealed static provider has no real original declaration"))
    }
    fn execution_intrinsic(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            return Ok(None);
        };
        let Some(path) = self.declaration_path(callee, variables, depth + 1)? else {
            return Ok(None);
        };
        if path.len() != 1 || self.declaration_target(owner, &path, depth + 1)?.is_some() {
            return Ok(None);
        }
        let name = path[0].as_str();
        self.pay(arguments.len().saturating_add(8), depth)?;
        if name == "task_yield" {
            if !arguments.is_empty() {
                return Err(bad("original task_yield has nonzero value arity"));
            }
            let result = SemanticTypeRecipe::Atom(Type::Unit);
            self.record_intrinsic(
                owner,
                site,
                IntrinsicKind::TaskYield,
                Vec::new(),
                &result,
                wanted,
                depth + 1,
            )?;
            return Ok(Some(result));
        }
        if matches!(name, "task_create" | "task_create_owned" | "task_await") {
            let worker_expression = arguments
                .first()
                .ok_or_else(|| bad("original Task creation omits its static worker"))?;
            let worker = self.static_provider(owner, worker_expression, variables, depth + 1)?;
            let definition = self.definition(worker)?;
            if definition.kind != SemanticDefinitionKind::Function {
                return Err(bad("original Task worker is not a real static function"));
            }
            if !self.proof.complete.contains(&worker) && !self.active.contains(&worker) {
                self.function(definition, None)?;
            }
            let SemanticDefinitionBody::FunctionWithBody {
                signature,
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            } = &definition.body
            else {
                return Err(bad("original Task worker omits its actual language body"));
            };
            if signature.call_kind != CallKind::Suspend {
                return Err(bad(
                    "original Task worker does not declare a checked Suspend ABI",
                ));
            }
            let operands = arguments[1..].iter().collect::<Vec<_>>();
            let written = self.written(
                owner,
                worker_expression,
                &definition.parameters,
                variables,
                depth + 1,
            )?;
            self.pay(written.len(), depth)?;
            let holes = written
                .into_iter()
                .map(|actual| actual.unwrap_or(SemanticActualRecipe::Infer))
                .collect::<Vec<_>>();
            let (actuals, signature) = self.solve_signature(
                owner,
                worker_expression,
                &operands,
                signature,
                &definition.parameters,
                &holes,
                variables,
                None,
                false,
                depth + 1,
            )?;
            self.call_requirements(owner, definition, &actuals, depth + 1)?;
            let payload = copy_recipe(&signature.result, self.limits, self.left)?;
            let result = if name == "task_await" {
                copy_recipe(&payload, self.limits, self.left)?
            } else {
                let shape = self.empty_task_shape(&signature.parameters, &payload, depth + 1)?;
                let error = if name == "task_create_owned" {
                    self.task_failure(owner, &signature.parameters, &shape, depth + 1)?
                } else {
                    self.compiler_carrier(
                        zil_ir::builtin_life::TASK_CREATE_ERROR,
                        Vec::new(),
                        depth + 1,
                    )?
                };
                let task = SemanticTypeRecipe::Managed(Box::new(SemanticTypeRecipe::Task {
                    result: Box::new(payload),
                    retained: Box::new(shape),
                }));
                SemanticTypeRecipe::ErrorUnion {
                    error: Box::new(error),
                    result: Box::new(task),
                }
            };
            self.pay(signature.parameters.len(), depth)?;
            let parameters = signature
                .parameters
                .iter()
                .map(|parameter| copy_recipe(&parameter.ty, self.limits, self.left))
                .collect::<R<Vec<_>>>()?;
            self.record_intrinsic(
                owner,
                site,
                IntrinsicKind::TaskCreate {
                    worker,
                    owned: name == "task_create_owned",
                    await_result: name == "task_await",
                },
                parameters,
                &result,
                wanted,
                depth + 1,
            )?;
            self.pay(actuals.len(), depth)?;
            for actual in &actuals {
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
            }
            self.proof
                .intrinsics
                .get_mut(&(owner, site))
                .ok_or_else(|| bad("Task scheme receipt disappeared"))?
                .worker_scheme = Some((signature, actuals.clone()));
            self.proof.calls.insert((owner, site), actuals);
            return Ok(Some(result));
        }
        if let Some(op) = match name {
            "task_valid" => Some(TaskOp::Valid),
            "task_poll" => Some(TaskOp::Poll),
            "task_request_cancel" => Some(TaskOp::RequestCancel),
            "task_poll_close" => Some(TaskOp::PollClose),
            "task_finish_destroy" => Some(TaskOp::FinishDestroy),
            _ => None,
        } {
            let [argument] = arguments.as_slice() else {
                return Err(bad(
                    "original Task operation changes its one-operand scheme",
                ));
            };
            let parameter = self.expr(owner, argument, variables, None, depth + 1)?;
            let task = match (&parameter, op) {
                (SemanticTypeRecipe::Desc { inner, .. }, TaskOp::Valid) => inner.as_ref(),
                (SemanticTypeRecipe::Managed(inner), TaskOp::FinishDestroy) => inner.as_ref(),
                (
                    SemanticTypeRecipe::Address {
                        mutable: true,
                        inner,
                        ..
                    },
                    TaskOp::Poll | TaskOp::RequestCancel | TaskOp::PollClose,
                ) => {
                    let SemanticTypeRecipe::Managed(task) = inner.as_ref() else {
                        return Err(bad(
                            "Task mutation requires an exclusive owning head address",
                        ));
                    };
                    task.as_ref()
                }
                _ => {
                    return Err(bad(
                        "original Task operation changes its carrier access/ownership mode",
                    ))
                }
            };
            let SemanticTypeRecipe::Task {
                result: payload, ..
            } = task
            else {
                return Err(bad(
                    "original Task operation changes its real sealed head kind",
                ));
            };
            let result = match op {
                TaskOp::Valid => SemanticTypeRecipe::Atom(Type::Bool),
                TaskOp::RequestCancel | TaskOp::FinishDestroy => {
                    SemanticTypeRecipe::Atom(Type::Unit)
                }
                TaskOp::PollClose => {
                    self.poll_carrier(&SemanticTypeRecipe::Atom(Type::Unit), depth + 1)?
                }
                TaskOp::Poll => SemanticTypeRecipe::ErrorUnion {
                    error: Box::new(self.compiler_carrier(
                        zil_ir::builtin_life::TASK_POLL_STATE,
                        Vec::new(),
                        depth + 1,
                    )?),
                    result: Box::new(self.poll_carrier(payload, depth + 1)?),
                },
            };
            let payload = copy_recipe(payload, self.limits, self.left)?;
            self.record_intrinsic(
                owner,
                site,
                IntrinsicKind::TaskOperation(op),
                vec![parameter],
                &result,
                wanted,
                depth + 1,
            )?;
            self.proof
                .calls
                .insert((owner, site), vec![SemanticActualRecipe::Type(payload)]);
            return Ok(Some(result));
        }
        self.registration_or_dyn_intrinsic(owner, site, expression, variables, wanted, depth + 1)
    }
    fn registration_or_dyn_intrinsic(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            return Ok(None);
        };
        let Some(path) = self.declaration_path(callee, variables, depth + 1)? else {
            return Ok(None);
        };
        if path.len() != 1 {
            return Ok(None);
        }
        let name = path[0].as_str();
        if name == "owned_dyn" {
            let interface = match &callee.kind {
                CoreExprKind::TypeApply { type_arguments, .. } if type_arguments.len() == 1 => {
                    self.resolve(owner, &type_arguments[0], depth + 1)?
                }
                CoreExprKind::GenericApply { arguments, .. } if arguments.len() == 1 => {
                    let CoreGenericArgument::Type(ty) = &arguments[0] else {
                        return Err(bad("owned dyn changes its sole original Type actual kind"));
                    };
                    self.resolve(owner, ty, depth + 1)?
                }
                _ => {
                    return Err(bad(
                        "owned dyn requires its full written original dyn interface",
                    ))
                }
            };
            let SemanticTypeRecipe::Dyn {
                owned: false,
                interface: descriptor,
            } = &interface
            else {
                return Err(bad(
                    "owned dyn Type argument is not its real borrowed interface recipe",
                ));
            };
            let [source] = arguments.as_slice() else {
                return Err(bad("owned dyn changes its one-source construction scheme"));
            };
            let source = self.expr(owner, source, variables, None, depth + 1)?;
            if !matches!(&source,SemanticTypeRecipe::Nominal{definition,..}if matches!(self.definition(*definition)?.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Record|LifeNominalKind::Package,..}))
            {
                return Err(bad(
                    "owned dyn source lacks its complete original product declaration",
                ));
            }
            self.prove_trait(owner, &source, &descriptor.application, depth + 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(
                descriptor.as_ref(),
                self.limits,
                self.left,
                1,
            )?;
            let result = SemanticTypeRecipe::ErrorUnion {
                error: Box::new(SemanticTypeRecipe::Exact(Box::new(
                    SemanticExactTypeRecipe {
                        ty: crate::dyn_dispatch::create_error(),
                        life: StorageLifeShape {
                            binder_count: 0,
                            slots: Vec::new(),
                            retained: Vec::new(),
                            nominal_arguments: Vec::new(),
                        },
                        classes: Vec::new(),
                    },
                ))),
                result: Box::new(SemanticTypeRecipe::Dyn {
                    owned: true,
                    interface: descriptor.clone(),
                }),
            };
            let source_actual = copy_recipe(&source, self.limits, self.left)?;
            self.record_intrinsic(
                owner,
                site,
                IntrinsicKind::OwnedDyn,
                vec![source],
                &result,
                wanted,
                depth + 1,
            )?;
            self.proof.calls.insert(
                (owner, site),
                vec![
                    SemanticActualRecipe::Type(interface),
                    SemanticActualRecipe::Type(source_actual),
                ],
            );
            return Ok(Some(result));
        }
        if matches!(name, "c_register" | "c_register_async") {
            if !self.original_unsafe(owner, expression.id, depth + 1)? {
                return Err(bad(
                    "registration construction lacks its real original unsafe region",
                ));
            }
            let asynchronous = name == "c_register_async";
            let provider_count = if asynchronous { 3 } else { 2 };
            if arguments.len() != provider_count + 1 {
                return Err(bad(
                    "registration constructor changes its ordered actual provider/callback arity",
                ));
            }
            let mut providers = Vec::new();
            self.pay(provider_count, depth)?;
            for (ordinal, argument) in arguments[..provider_count].iter().enumerate() {
                let id = self.static_provider(owner, argument, variables, depth + 1)?;
                let definition = self.definition(id)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    signature,
                    body: SemanticOriginalFunctionBody::External,
                    ..
                } = &definition.body
                else {
                    return Err(bad(
                        "registration provider is not its real original external declaration",
                    ));
                };
                if !definition.parameters.is_empty() || signature.call_kind != CallKind::Direct {
                    return Err(bad(
                        "registration provider changes its concrete C role signature",
                    ));
                }
                let wanted_parameters = if ordinal == 0 {
                    vec![
                        SemanticTypeRecipe::Code {
                            kind: SemanticCodeKind::ForeignCallback,
                            signature: Box::new(SemanticSignatureRecipe {
                                bound_lifetimes: 0,
                                invocation_lifetimes: 0,
                                parameters: vec![
                                    SemanticParameterRecipe {
                                        ty: SemanticTypeRecipe::Atom(Type::ForeignHandle),
                                        mode: PassingMode::Infer,
                                    },
                                    SemanticParameterRecipe {
                                        ty: SemanticTypeRecipe::Atom(Type::I32),
                                        mode: PassingMode::Infer,
                                    },
                                ],
                                result: SemanticTypeRecipe::Atom(Type::I64),
                                result_mode: PassingMode::Infer,
                                call_kind: CallKind::Direct,
                            }),
                        },
                        SemanticTypeRecipe::Atom(Type::ForeignHandle),
                    ]
                } else {
                    vec![SemanticTypeRecipe::Atom(Type::ForeignHandle)]
                };
                if signature.parameters.len() != wanted_parameters.len() {
                    return Err(bad(
                        "registration provider changes its complete original C parameter roles",
                    ));
                }
                for (parameter, wanted) in signature.parameters.iter().zip(&wanted_parameters) {
                    let actual = self.resolve(id, &parameter.ty, depth + 1)?;
                    self.same(&actual, wanted)?;
                }
                let expected_result = SemanticTypeRecipe::Atom(if ordinal == 0 {
                    Type::ForeignHandle
                } else if ordinal == 2 {
                    Type::I32
                } else {
                    Type::Unit
                });
                let result = self.resolve(id, &signature.result, depth + 1)?;
                self.same(&result, &expected_result)?;
                providers.push(id);
            }
            let callback = self.expr(
                owner,
                &arguments[provider_count],
                variables,
                None,
                depth + 1,
            )?;
            let signature = SemanticSignatureRecipe {
                bound_lifetimes: 0,
                invocation_lifetimes: 0,
                parameters: vec![SemanticParameterRecipe {
                    ty: SemanticTypeRecipe::Atom(Type::I32),
                    mode: PassingMode::Infer,
                }],
                result: SemanticTypeRecipe::Atom(Type::I32),
                result_mode: PassingMode::Infer,
                call_kind: CallKind::Direct,
            };
            self.callable_requirement(
                owner,
                &callback,
                CallableKind::FnMut,
                &signature,
                depth + 1,
            )?;
            if self.sealed_capability(
                owner,
                &callback,
                SemanticBuiltinTrait::ClosedEnvironment,
                depth + 1,
            )? != Some(true)
            {
                return Err(bad("registration callback lacks its independently proved complete ClosedEnvironment premise"));
            }
            let threaded_capable = self.sealed_capability(
                owner,
                &callback,
                SemanticBuiltinTrait::Transfer,
                depth + 1,
            )? == Some(true)
                && self.sealed_capability(
                    owner,
                    &callback,
                    SemanticBuiltinTrait::ThreadCallable,
                    depth + 1,
                )? == Some(true);
            let result = SemanticTypeRecipe::Managed(Box::new(if asynchronous {
                SemanticTypeRecipe::AsyncRegistration(Box::new(copy_recipe(
                    &callback,
                    self.limits,
                    self.left,
                )?))
            } else {
                SemanticTypeRecipe::Registration(Box::new(copy_recipe(
                    &callback,
                    self.limits,
                    self.left,
                )?))
            }));
            let actual = copy_recipe(&callback, self.limits, self.left)?;
            self.record_intrinsic(
                owner,
                site,
                IntrinsicKind::RegistrationStart {
                    providers,
                    asynchronous,
                    threaded_capable,
                },
                vec![callback],
                &result,
                wanted,
                depth + 1,
            )?;
            self.proof
                .calls
                .insert((owner, site), vec![SemanticActualRecipe::Type(actual)]);
            return Ok(Some(result));
        }
        if let Some(op) = match name {
            "registration_valid" => Some(RegistrationOp::Valid),
            "registration_close" => Some(RegistrationOp::Close),
            "async_registration_valid" => Some(RegistrationOp::AsyncValid),
            "async_registration_request_close" => Some(RegistrationOp::AsyncRequest),
            "async_registration_poll_close" => Some(RegistrationOp::AsyncPollClose),
            "async_registration_finish_destroy" => Some(RegistrationOp::AsyncFinish),
            "async_registration_finish_destroy_checked" => Some(RegistrationOp::AsyncFinishChecked),
            _ => None,
        } {
            let [argument] = arguments.as_slice() else {
                return Err(bad(
                    "registration operation changes its single actual carrier arity",
                ));
            };
            let parameter = self.expr(owner, argument, variables, None, depth + 1)?;
            let inner = match (&parameter, op) {
                (
                    SemanticTypeRecipe::Desc { inner, .. },
                    RegistrationOp::Valid | RegistrationOp::AsyncValid,
                ) => inner.as_ref(),
                (
                    SemanticTypeRecipe::Managed(inner),
                    RegistrationOp::Close
                    | RegistrationOp::AsyncFinish
                    | RegistrationOp::AsyncFinishChecked,
                ) => inner.as_ref(),
                (
                    SemanticTypeRecipe::Address {
                        mutable: true,
                        inner,
                        ..
                    },
                    RegistrationOp::AsyncRequest | RegistrationOp::AsyncPollClose,
                ) => {
                    let SemanticTypeRecipe::Managed(payload) = inner.as_ref() else {
                        return Err(bad(
                            "registration mutation requires an exclusive owning head address",
                        ));
                    };
                    payload.as_ref()
                }
                _ => {
                    return Err(bad(
                        "registration operation changes its ownership or receiver access mode",
                    ))
                }
            };
            let environment =
                match (inner, op) {
                    (
                        SemanticTypeRecipe::Registration(environment),
                        RegistrationOp::Valid | RegistrationOp::Close,
                    ) => environment.as_ref(),
                    (
                        SemanticTypeRecipe::AsyncRegistration(environment),
                        RegistrationOp::AsyncValid
                        | RegistrationOp::AsyncRequest
                        | RegistrationOp::AsyncPollClose
                        | RegistrationOp::AsyncFinish
                        | RegistrationOp::AsyncFinishChecked,
                    ) => environment.as_ref(),
                    _ => return Err(bad(
                        "registration operation changes its synchronous/asynchronous head family",
                    )),
                };
            let actual = copy_recipe(environment, self.limits, self.left)?;
            let result = if matches!(op, RegistrationOp::Valid | RegistrationOp::AsyncValid) {
                SemanticTypeRecipe::Atom(Type::Bool)
            } else if op == RegistrationOp::AsyncPollClose {
                self.poll_carrier(&SemanticTypeRecipe::Atom(Type::Unit), depth + 1)?
            } else {
                SemanticTypeRecipe::Atom(Type::Unit)
            };
            self.record_intrinsic(
                owner,
                site,
                IntrinsicKind::RegistrationOperation(op),
                vec![parameter],
                &result,
                wanted,
                depth + 1,
            )?;
            self.proof
                .calls
                .insert((owner, site), vec![SemanticActualRecipe::Type(actual)]);
            return Ok(Some(result));
        }
        Ok(None)
    }
}

struct UnsafeObservation<'a> {
    target: zil_base::NodeId,
    limits: &'a Limits,
    left: &'a mut usize,
    found: bool,
    scan: bool,
}
impl super::super::original_walk::Context for UnsafeObservation<'_> {
    fn node(&mut self, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original unsafe occurrence work exceeded",
                None,
            )
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original unsafe occurrence depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn ty(&mut self, _: &mut SemanticTypeRecipe, _: usize) -> R<()> {
        Ok(())
    }
    fn descend_closure(&self) -> bool {
        false
    }
    fn expression(
        &mut self,
        id: zil_base::NodeId,
        _: &SemanticOriginalExprKind,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        if !self.scan && id == self.target {
            self.found = true;
        }
        Ok(())
    }
    fn statement(
        &mut self,
        _: zil_base::NodeId,
        kind: &SemanticOriginalStatementKind,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        if self.scan {
            if let CoreStatementKind::Unsafe(body) = kind {
                crate::dyn_metadata::semantic_operand_snapshot(body, self.limits, self.left, 1)?;
                let mut body = body.clone();
                let mut observation = UnsafeObservation {
                    target: self.target,
                    limits: self.limits,
                    left: self.left,
                    found: false,
                    scan: false,
                };
                super::super::original_walk::block(&mut body, &mut observation, depth + 1)?;
                self.found |= observation.found;
            }
        }
        Ok(())
    }
}
impl Check<'_, '_> {
    fn original_unsafe(
        &mut self,
        owner: SemanticDefinitionId,
        expression: zil_base::NodeId,
        depth: usize,
    ) -> R<bool> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            body: SemanticOriginalFunctionBody::Defined(body),
            ..
        } = &definition.body
        else {
            return Ok(false);
        };
        crate::dyn_metadata::semantic_operand_snapshot(body, self.limits, self.left, 1)?;
        let mut body = body.clone();
        let mut observation = UnsafeObservation {
            target: expression,
            limits: self.limits,
            left: self.left,
            found: false,
            scan: true,
        };
        super::super::original_walk::block(&mut body, &mut observation, depth + 1)?;
        Ok(observation.found)
    }
}

//! Original mutex and structured-thread schemes. A guard carries the lifetime
//! of its actual stable mutex; it is neither Copy nor a transferable handle.
use super::*;

impl Solver<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn mutex_call(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        intrinsic: sealed::Intrinsic,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        if !written.is_empty() {
            return Err(fail("synchronization intrinsic invents generic actuals"));
        }
        let mut inputs = Vec::new();
        let result = if intrinsic == sealed::Intrinsic::ThreadScope {
            let [worker, argument, body] = arguments else {
                return Err(fail(
                    "structured thread requires its complete worker, input and parent body",
                ));
            };
            let (target, parameters, output, modes, result_mode, call_kind) =
                self.task_worker(frame, worker, b, depth + 1)?;
            if parameters.len() != 1
                || modes.len() != 1
                || call_kind != CallKind::Direct
                || !matches!(
                    self.state.arena[self.type_root(output, b)?],
                    Kind::Atom(Type::I32)
                )
            {
                return Err(fail(
                    "structured thread worker changes its original Direct scalar signature",
                ));
            }
            let actual = self.expression(frame, argument, b, depth + 1)?;
            self.unify(actual, parameters[0], depth + 1, b)?;
            if !self.thread_boundary(parameters[0], b, depth + 1)? {
                return Err(fail(
                    "structured thread input lacks its scoped synchronization or transfer scheme",
                ));
            }
            let body_term = self.expression(frame, body, b, depth + 1)?;
            let signature = SemanticSignatureRecipe {
                bound_lifetimes: 0,
                invocation_lifetimes: 0,
                parameters: Vec::new(),
                result: SemanticTypeRecipe::Atom(Type::I32),
                result_mode: PassingMode::Infer,
                call_kind: CallKind::Direct,
            };
            self.callable_need(
                frame,
                body_term,
                CallableKind::FnOnce,
                &signature,
                b,
                depth + 1,
            )?;
            let reference = self.term(
                Kind::Code {
                    kind: SemanticCodeKind::FunctionPointer,
                    parameters,
                    result: output,
                    modes,
                    result_mode,
                    call_kind,
                },
                b,
            )?;
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(worker.id, reference).is_some() {
                return Err(fail(
                    "structured thread worker reference reuses another source node",
                ));
            }
            b.lookup(self.state.native_targets.len(), WORK)?;
            if self
                .state
                .native_targets
                .insert((frame.definition, node), vec![target])
                .is_some()
            {
                return Err(fail(
                    "structured thread has competing original worker targets",
                ));
            }
            Self::push(&mut inputs, actual, b)?;
            Self::push(&mut inputs, body_term, b)?;
            let anchor = self.registration_anchor(frame, argument, b, depth + 1)?;
            b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
            if self
                .state
                .intrinsic_anchors
                .insert((frame.definition, node), anchor)
                .is_some()
            {
                return Err(fail("thread input has competing original value anchors"));
            }
            let flag = self.atom(Type::Bool, b)?;
            let scalar = self.atom(Type::I32, b)?;
            self.term(Kind::Tuple(vec![flag, scalar, scalar]), b)?
        } else {
            let [argument] = arguments else {
                return Err(fail("mutex operation changes its complete ordered arity"));
            };
            let input = self.expression(frame, argument, b, depth + 1)?;
            Self::push(&mut inputs, input, b)?;
            match intrinsic {
                sealed::Intrinsic::MutexCreate => {
                    let payload = self.atom(Type::I32, b)?;
                    self.unify(input, payload, depth + 1, b)?;
                    let payload = self.term(Kind::Wrapped(Wrapper::Mutex, payload), b)?;
                    let owner = self.term(Kind::Wrapped(Wrapper::Managed, payload), b)?;
                    let error = self.atom(crate::generic_mutex::error_type(), b)?;
                    self.term(
                        Kind::ErrorUnion {
                            error,
                            result: owner,
                        },
                        b,
                    )?
                }
                sealed::Intrinsic::MutexLock => {
                    let Kind::Desc { life, inner } = self.state.arena[self.type_root(input, b)?]
                    else {
                        return Err(unknown(
                            "mutex lock needs its exact original weak description and head lifetime",
                        ));
                    };
                    let Kind::Wrapped(Wrapper::Mutex, payload) =
                        self.state.arena[self.type_root(inner, b)?]
                    else {
                        return Err(fail("mutex lock selects another original resource family"));
                    };
                    let scalar = self.atom(Type::I32, b)?;
                    self.unify(payload, scalar, depth + 1, b)?;
                    let anchor = self.registration_anchor(frame, argument, b, depth + 1)?;
                    b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
                    if self
                        .state
                        .intrinsic_anchors
                        .insert((frame.definition, node), anchor)
                        .is_some()
                    {
                        return Err(fail("mutex lock has competing original source heads"));
                    }
                    self.term(Kind::MutexGuard { payload, life }, b)?
                }
                sealed::Intrinsic::MutexValue => {
                    let Kind::Address {
                        mutable: true,
                        life,
                        inner,
                    } = self.state.arena[self.type_root(input, b)?]
                    else {
                        return Err(fail(
                            "mutex value requires an exclusive short borrow of its guard",
                        ));
                    };
                    let Kind::MutexGuard { payload, .. } =
                        self.state.arena[self.type_root(inner, b)?]
                    else {
                        return Err(fail("mutex value selects no actual guard obligation"));
                    };
                    let anchor = self.registration_anchor(frame, argument, b, depth + 1)?;
                    b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
                    if self
                        .state
                        .intrinsic_anchors
                        .insert((frame.definition, node), anchor)
                        .is_some()
                    {
                        return Err(fail("mutex projection has competing original guard heads"));
                    }
                    self.term(
                        Kind::Address {
                            mutable: true,
                            life,
                            inner: payload,
                        },
                        b,
                    )?
                }
                _ => return Err(fail("another intrinsic entered the original mutex scheme")),
            }
        };
        b.lookup(self.state.intrinsics.len(), WORK)?;
        if self
            .state
            .intrinsics
            .insert((frame.definition, node), intrinsic)
            .is_some()
        {
            return Err(fail("synchronization call has competing original schemes"));
        }
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        if self
            .state
            .intrinsic_inputs
            .insert((frame.definition, node), inputs)
            .is_some()
        {
            return Err(fail(
                "synchronization call has competing complete input roles",
            ));
        }
        Ok(result)
    }

    fn thread_boundary(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        self.step(depth, b)?;
        if let Kind::Desc { inner, .. } = self.state.arena[self.type_root(term, b)?]
            && let Kind::Wrapped(Wrapper::Mutex, payload) =
                self.state.arena[self.type_root(inner, b)?]
        {
            return Ok(matches!(
                self.state.arena[self.type_root(payload, b)?],
                Kind::Atom(Type::I32)
            ));
        }
        self.capability(term, SemanticBuiltinTrait::Transfer, b, depth + 1)
    }

    pub(super) fn check_scoped_threads(&self, b: &mut Budget) -> CompileResult<()> {
        let mut pending = Vec::new();
        for (key, intrinsic) in &self.state.intrinsics {
            b.spend(1, WORK)?;
            if *intrinsic != sealed::Intrinsic::ThreadScope {
                continue;
            }
            b.lookup(self.state.native_targets.len(), WORK)?;
            let targets = self
                .state
                .native_targets
                .get(key)
                .ok_or_else(|| fail("thread operation loses its original worker"))?;
            for target in targets {
                Self::push(&mut pending, *target, b)?;
            }
        }
        let mut done = BTreeSet::new();
        while let Some(target) = pending.pop() {
            b.lookup(done.len(), WORK)?;
            if !done.insert(target) {
                continue;
            }
            let declaration = self.inventory.definition(target, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                signature,
                foreign,
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            } = &declaration.body
            else {
                return Err(unknown(
                    "thread worker execution needs the complete original defined or explicitly thread-safe external scheme",
                ));
            };
            if foreign.is_some() || signature.call_kind != CallKind::Direct {
                return Err(fail(
                    "structured thread executes an unproved external or Suspend original edge",
                ));
            }
            let frame = self
                .state
                .frames
                .get(&target)
                .ok_or_else(|| fail("thread worker has no universal original body proof"))?;
            for ty in frame
                .parameters
                .iter()
                .chain(std::iter::once(&frame.result))
                .chain(&frame.environment)
            {
                if !self.thread_boundary(*ty, b, 0)? {
                    return Err(fail(
                        "thread worker boundary carries unproved data or borrowed heads",
                    ));
                }
            }
            // Internal guard/borrow temporaries stay on this worker. Their
            // lifetime, initialization and cleanup were checked in original Γ;
            // they are never declared Transfer simply because they are local.
            for ((owner, _), call) in &self.state.calls {
                b.spend(1, WORK)?;
                if *owner == target {
                    Self::push(&mut pending, call.target, b)?;
                }
            }
            for ((owner, _), call) in &self.state.abstract_calls {
                b.spend(1, WORK)?;
                if *owner == target
                    && !self.capability(call.subject, SemanticBuiltinTrait::ThreadCallable, b, 0)?
                {
                    return Err(fail(
                        "original abstract thread call lacks ThreadCallable in its own context",
                    ));
                }
            }
            for ((owner, _), intrinsic) in &self.state.intrinsics {
                b.spend(1, WORK)?;
                if *owner == target
                    && !matches!(
                        intrinsic,
                        sealed::Intrinsic::NumericConvert(_)
                            | sealed::Intrinsic::CCallback
                            | sealed::Intrinsic::SliceLength
                            | sealed::Intrinsic::MutexLock
                            | sealed::Intrinsic::MutexValue
                    )
                {
                    return Err(unknown(
                        "thread worker intrinsic needs its complete concurrent execution and cleanup scheme",
                    ));
                }
            }
        }
        Ok(())
    }
}

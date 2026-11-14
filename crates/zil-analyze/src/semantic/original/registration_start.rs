//! Installation consumes a callback under its original declaration premises.
//! In particular the failure path's synchronous cleanup is not implied by
//! FnMut, ClosedEnvironment or the later owner's pollable close protocol.
use super::*;
impl Solver<'_, '_> {
    fn provider(
        &mut self,
        frame: &Frame,
        expression: &SemanticOriginalExpr,
        role: ScopedCallbackRole,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<SemanticDefinitionId> {
        self.step(depth, b)?;
        let path = sealed::path(expression, b, depth + 1)?.ok_or_else(|| {
            unknown("original registration provider needs its complete static declaration identity")
        })?;
        let mut locals = BTreeSet::new();
        for local in &frame.locals {
            b.spend(local.name.len().saturating_add(locals.len() + 1), WORK)?;
            locals.insert(local.name.clone());
        }
        let sealed::Resolution::Definition(target) =
            sealed::resolve(self.inventory, frame.definition, &path, &locals, b)?
        else {
            return Err(fail(
                "original registration provider is shadowed or has no exact declared identity",
            ));
        };
        let declaration = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            foreign: Some(foreign),
            body: SemanticOriginalFunctionBody::External,
            ..
        } = &declaration.body
        else {
            return Err(fail(
                "original registration provider has no original external ABI promise",
            ));
        };
        crate::dyn_metadata::recipe(signature, b)?;
        if declaration.parent.is_some()
            || !declaration.parameters.is_empty()
            || !declaration.environment.is_empty()
            || signature.bound_lifetimes != 0
            || signature.invocation_lifetimes != 0
            || signature.call_kind != CallKind::Direct
            || foreign.abi != "C"
            || !foreign.static_callbacks
            || foreign.scoped_callback != Some(role)
            || !foreign.noescape_parameters.is_empty()
            || signature.result_mode != PassingMode::Infer
            || signature
                .parameters
                .iter()
                .any(|parameter| parameter.mode != PassingMode::Infer)
        {
            return Err(fail(
                "original registration provider changes its complete fixed C role",
            ));
        }
        let callback = SemanticTypeRecipe::Code {
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
        };
        let register = matches!(
            role,
            ScopedCallbackRole::Register
                | ScopedCallbackRole::AsyncRegister
                | ScopedCallbackRole::ThreadAsyncRegister
        );
        let drain = matches!(
            role,
            ScopedCallbackRole::AsyncPollDrain | ScopedCallbackRole::ThreadAsyncPollDrain
        );
        let expected = if register {
            vec![callback, SemanticTypeRecipe::Atom(Type::ForeignHandle)]
        } else {
            vec![SemanticTypeRecipe::Atom(Type::ForeignHandle)]
        };
        let output = if register {
            Type::ForeignHandle
        } else if drain {
            Type::I32
        } else {
            Type::Unit
        };
        if signature.parameters.len() != expected.len() {
            return Err(fail(
                "original registration provider has another complete value arity",
            ));
        }
        for (actual, expected) in signature.parameters.iter().zip(expected) {
            crate::dyn_metadata::recipe(&actual.ty, b)?;
            crate::dyn_metadata::recipe(&expected, b)?;
            if actual.ty != expected {
                return Err(fail(
                    "original registration provider changes a complete C parameter type",
                ));
            }
        }
        if signature.result != SemanticTypeRecipe::Atom(output) {
            return Err(fail(
                "original registration provider changes its fixed C result carrier",
            ));
        }
        Ok(target)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn registration_start(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        id: sealed::Intrinsic,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let asynchronous = id == sealed::Intrinsic::RegisterAsync;
        let count = if asynchronous { 4 } else { 3 };
        if !written.is_empty() || arguments.len() != count {
            return Err(fail(
                "original registration installation changes its complete source scheme arity",
            ));
        }
        let first_path = sealed::path(&arguments[0], b, depth + 1)?.ok_or_else(|| {
            unknown("original installation needs its exact foreign provider declaration")
        })?;
        let mut locals = BTreeSet::new();
        for local in &frame.locals {
            b.spend(local.name.len().saturating_add(locals.len() + 1), WORK)?;
            locals.insert(local.name.clone());
        }
        let sealed::Resolution::Definition(first) =
            sealed::resolve(self.inventory, frame.definition, &first_path, &locals, b)?
        else {
            return Err(fail(
                "original installation provider is shadowed or undeclared",
            ));
        };
        let declaration = self.inventory.definition(first, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            foreign: Some(foreign),
            ..
        } = &declaration.body
        else {
            return Err(fail(
                "original installation has no complete original foreign promise",
            ));
        };
        let threaded = foreign.scoped_callback == Some(ScopedCallbackRole::ThreadAsyncRegister);
        let roles: &[ScopedCallbackRole] = if !asynchronous {
            &[ScopedCallbackRole::Register, ScopedCallbackRole::Close]
        } else if threaded {
            &[
                ScopedCallbackRole::ThreadAsyncRegister,
                ScopedCallbackRole::ThreadAsyncRequestClose,
                ScopedCallbackRole::ThreadAsyncPollDrain,
            ]
        } else {
            &[
                ScopedCallbackRole::AsyncRegister,
                ScopedCallbackRole::AsyncRequestClose,
                ScopedCallbackRole::AsyncPollDrain,
            ]
        };
        let mut targets = Vec::new();
        for (expression, role) in arguments.iter().zip(roles) {
            let target = self.provider(frame, expression, *role, b, depth + 1)?;
            Self::push(&mut targets, target, b)?;
        }
        let callback_expression = arguments
            .last()
            .ok_or_else(|| fail("original installation loses its callback"))?;
        let callback = self.expression(frame, callback_expression, b, depth + 1)?;
        let anchor = self.registration_anchor(frame, callback_expression, b, depth + 1)?;
        for predicate in [
            Predicate::SyncDrop,
            Predicate::Capability(SemanticBuiltinTrait::ClosedEnvironment),
        ] {
            Self::push(
                &mut self.needs,
                Need {
                    term: callback,
                    predicate,
                },
                b,
            )?;
        }
        if threaded {
            for capability in [
                SemanticBuiltinTrait::Transfer,
                SemanticBuiltinTrait::ThreadCallable,
            ] {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: callback,
                        predicate: Predicate::Capability(capability),
                    },
                    b,
                )?;
            }
        }
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
        if let Kind::Rigid(owner, ordinal) = self.state.arena[self.type_root(callback, b)?] {
            let (kind, available) = self.callable_premise(owner, ordinal, b)?;
            crate::dyn_metadata::recipe(&available, b)?;
            crate::dyn_metadata::recipe(&signature, b)?;
            if !matches!(kind, CallableKind::Fn | CallableKind::FnMut) || available != signature {
                return Err(fail(
                    "original installation lacks its complete scalar FnMut callback premise",
                ));
            }
        } else {
            self.callable_need(
                frame,
                callback,
                CallableKind::FnMut,
                &signature,
                b,
                depth + 1,
            )?;
        }
        b.lookup(self.state.native_targets.len(), WORK)?;
        if self
            .state
            .native_targets
            .insert((frame.definition, node), targets)
            .is_some()
        {
            return Err(fail(
                "original installation has competing foreign provider sets",
            ));
        }
        b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
        self.state
            .intrinsic_anchors
            .insert((frame.definition, node), anchor);
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        self.state
            .intrinsic_inputs
            .insert((frame.definition, node), vec![callback]);
        b.lookup(self.state.intrinsics.len(), WORK)?;
        self.state.intrinsics.insert((frame.definition, node), id);
        let payload = self.term(
            Kind::Wrapped(
                if asynchronous {
                    Wrapper::AsyncRegistration
                } else {
                    Wrapper::Registration
                },
                callback,
            ),
            b,
        )?;
        self.term(Kind::Wrapped(Wrapper::Managed, payload), b)
    }
}

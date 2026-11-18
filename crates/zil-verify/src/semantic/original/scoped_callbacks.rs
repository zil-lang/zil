//! The scoped C registration borrows one concrete callback and consumes its
//! body on both outcomes. Its two declaration selectors are never arguments.
use super::*;
use intrinsics::BufferSource;

#[derive(Clone, Copy)]
pub(in crate::semantic) struct ScopedCallbackSources {
    pub(in crate::semantic) providers: [SemanticDefinitionId; 2],
    pub(in crate::semantic) callback: BufferSource,
    pub(in crate::semantic) body_node: NodeId,
    pub(in crate::semantic) body_place: Option<BufferSource>,
}

impl CheckedOriginal {
    pub(super) fn scoped_callback(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        site: SemanticSiteId,
        arguments: &[SemanticOriginalExpr],
        env: &Env,
        bindings: &BTreeMap<String, Binding>,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Ty> {
        let owner = env.current()?;
        if self.unsafe_depth.get(&owner).copied().unwrap_or(0) == 0 {
            return Err(bad(
                "scoped foreign callback lacks its original unsafe scope",
            ));
        }
        let operation = CallbackRegistration::Scoped;
        let operands = operation
            .operands(arguments)
            .ok_or_else(|| bad("scoped registration has another original operand list"))?;
        let [register, close] = operands.providers else {
            return Err(bad("scoped registration needs its two static providers"));
        };
        let mut resolve = |expr: &SemanticOriginalExpr, role| -> Check<SemanticDefinitionId> {
            let mut selector = expr;
            while let CoreExprKind::Group(inner) = &selector.kind {
                b.spend(1)?;
                selector = inner;
            }
            let direct = match &selector.kind {
                CoreExprKind::Name(name) => !bindings.contains_key(name),
                CoreExprKind::Member { .. } => self
                    .static_call_type(module, context, selector, env, bindings, b)?
                    .is_some(),
                _ => false,
            };
            if !direct {
                return Err(bad(
                    "scoped registration provider must select a static declaration in its original scope",
                ));
            }
            let value = self.expression(module, context, expr, env, bindings, None, active, b)?;
            let Ty::Function(id) = self.resolved(&value, b)? else {
                return Err(bad(
                    "scoped registration provider is not an original function",
                ));
            };
            let definition = context.definition(module, id, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                foreign: Some(foreign),
                body: SemanticOriginalFunctionBody::External,
                ..
            } = &definition.body
            else {
                return Err(bad(
                    "scoped registration provider is not a declared external function",
                ));
            };
            if !foreign.static_callbacks || foreign.scoped_callback != Some(role) {
                return Err(bad(
                    "scoped registration changes its original provider role",
                ));
            }
            self.function_head(definition, b)?;
            Ok(id)
        };
        let providers = [
            resolve(register, ScopedCallbackRole::Register)?,
            resolve(close, ScopedCallbackRole::Close)?,
        ];
        let callback = self.expression(
            module,
            context,
            operands.callback,
            env,
            bindings,
            None,
            active,
            b,
        )?;
        let Ty::Address {
            mutable: true,
            inner: callback,
            ..
        } = self.resolved(&callback, b)?
        else {
            return Err(bad(
                "scoped registration requires an exclusive original callback address",
            ));
        };
        let entry = self.operation_callable(module, context, &callback, env, active, b)?;
        let [input] = entry.parameters.as_slice() else {
            return Err(bad("scoped callback needs its single original i32 input"));
        };
        self.unify(input, &Ty::Atom(Type::I32), b)?;
        self.unify(&entry.result, &Ty::Atom(Type::I32), b)?;
        if entry.call_kind != CallKind::Direct
            || !matches!(
                entry.callable_kind,
                Some(CallableKind::Fn | CallableKind::FnMut)
            )
        {
            return Err(bad(
                "scoped callback cannot be invoked repeatedly and synchronously",
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
        let callback = self.operation_source(module, operands.callback, env, bindings, b)?;
        let body = operands
            .body
            .ok_or_else(|| bad("scoped registration omits its body"))?;
        let body_ty = self.expression(module, context, body, env, bindings, None, active, b)?;
        let entry = self.operation_callable(module, context, &body_ty, env, active, b)?;
        if entry.call_kind != CallKind::Direct
            || !entry.parameters.is_empty()
            || !matches!(
                entry.callable_kind,
                Some(CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce)
            )
        {
            return Err(bad(
                "scoped registration body cannot be consumed synchronously with zero arguments",
            ));
        }
        self.unify(&entry.result, &Ty::Atom(Type::I32), b)?;
        self.structural_obligation(
            module,
            context,
            &body_ty,
            DynStructuralBound::SyncDrop,
            env,
            active,
            b,
        )?;
        let mut observed = body;
        while let CoreExprKind::Group(inner)
        | CoreExprKind::Ascribed { value: inner, .. }
        | CoreExprKind::Unary {
            op: UnaryOp::Give,
            operand: inner,
        } = &observed.kind
        {
            b.spend(1)?;
            observed = inner;
        }
        let body_place = if matches!(observed.kind, CoreExprKind::Name(_))
            || self.value_places.contains_key(&(owner, observed.id))
        {
            Some(self.operation_source(module, observed, env, bindings, b)?)
        } else {
            None
        };
        self.scoped_callbacks.insert(
            (owner, site),
            ScopedCallbackSources {
                providers,
                callback,
                body_node: observed.id,
                body_place,
            },
        );
        Ok(Ty::Tuple(vec![Ty::Atom(Type::Bool), Ty::Atom(Type::I32)]))
    }
    pub(in crate::semantic) fn scoped_callback_sources(
        &self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        b: &mut Budget,
    ) -> Check<ScopedCallbackSources> {
        b.spend(1)?;
        self.scoped_callbacks
            .get(&(owner, site))
            .copied()
            .ok_or_else(|| bad("scoped callback lacks its original source directory"))
    }
}

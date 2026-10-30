//! Definition-time checking for the one-owned-F factory. These symbolic
//! positions restrict source admission; concrete IR remains independently
//! checked by Types, Analyze and Verify.
use super::*;
use zil_ir::CallableKind;

#[derive(Default)]
struct Use {
    written: bool,
    consumed: bool,
}

pub(super) struct Scope {
    outer: BTreeSet<usize>,
    captures: BTreeMap<usize, Use>,
    result: Option<TypeRef>,
}

impl Check<'_, '_, '_> {
    fn factory_copy(&mut self, ty: &TypeRef) -> R<()> {
        copy::require(ty, self.nominals, &self.bounds, &self.variables, self.budget)
    }

    fn factory_signature(&mut self, parameters: &[TypeRef], result: &TypeRef) -> R<()> {
        for parameter in parameters {
            self.tick(parameter.span)?;
            let value = match &group(parameter).kind {
                TypeKind::Reference { inner, lifetime: None, .. } => match &group(inner).kind {
                    TypeKind::Array { element, length: None } => element.as_ref(),
                    _ => inner.as_ref(),
                },
                _ => parameter,
            };
            self.factory_copy(value)?;
        }
        self.factory_copy(result)
    }

    pub(super) fn factory_header(&mut self, function: &Function) -> R<()> {
        let bounds = self.callable.as_ref().expect("factory bound");
        let [bound] = bounds.roles.as_slice() else {
            return Err(unsupported(function.span, "owning factory requires one direct F input"));
        };
        for parameter in &bound.parameters {
            self.budget.ty(parameter)?;
        }
        self.budget.ty(&bound.result)?;
        let bound = bound.clone();
        self.factory_signature(&bound.parameters, &bound.result)?;
        let (_, parameters, result) = callable::opaque_signature(
            function.return_type.as_ref().expect("factory result"),
        ).expect("factory callable signature");
        self.factory_signature(parameters, result)?;
        for (index, parameter) in function.params.iter().enumerate() {
            self.tick(parameter.span)?;
            if index != bound.index && dependent(&parameter.ty, &self.variables) {
                return Err(unsupported(parameter.span, "ordinary factory parameters cannot hide callable type inputs"));
            }
        }
        Ok(())
    }

    pub(super) fn factory_use(&mut self, id: usize, written: bool, consumed: bool, span: Span) -> R<()> {
        let Some(scope) = self.factory_scopes.last_mut() else { return Ok(()) };
        self.budget.node(span, 0)?;
        if scope.outer.contains(&id) {
            self.budget.node(span, 0)?;
            let usage = scope.captures.entry(id).or_default();
            usage.written |= written;
            usage.consumed |= consumed;
        }
        Ok(())
    }

    pub(super) fn factory_place(&mut self, expression: &Expr, state: &State, written: bool) -> R<()> {
        if self.factory_scopes.is_empty() { return Ok(()) }
        self.tick(expression.span)?;
        match &expression.kind {
            ExprKind::Name(name) => {
                if let Some(local) = state.locals.get(name) {
                    self.factory_use(local.id, written, false, expression.span)?;
                }
            }
            ExprKind::Group(inner) | ExprKind::Unary { operand: inner, .. } =>
                self.factory_place(inner, state, written)?,
            ExprKind::Member { object, .. } | ExprKind::Index { object, .. }
            | ExprKind::Slice { object, .. } => self.factory_place(object, state, written)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn factory_body_return(&mut self, expression: Option<&Expr>, state: &mut State, span: Span) -> R<()> {
        let expected = self.factory_scopes.last().expect("closure scope").result.clone();
        if let Some(ty) = &expected { self.budget.ty(ty)?; }
        let actual = if let Some(expression) = expression {
            self.expr(expression, state, expected.as_ref(), false)?
        } else { unit(span) };
        if let Some(expected) = expected {
            same_return(&actual, &expected, span, self.budget)?;
        } else {
            self.budget.ty(&actual)?;
            self.result = actual.clone();
            self.factory_scopes.last_mut().expect("closure scope").result = Some(actual);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn factory_closure(
        &mut self, move_capture: bool, parameters: &[ClosureParam], annotation: Option<&TypeRef>,
        body: &ArrowBody, expected: Option<&TypeRef>, state: &mut State, span: Span,
    ) -> R<TypeRef> {
        let context = expected.and_then(callable::opaque_signature);
        if context.is_some_and(|(_, parameters_, _)| parameters_.len() != parameters.len()) {
            return Err(error(span, "factory closure argument count differs from its opaque result"));
        }
        let mut child = self.clone_state(state)?;
        let mut outer = BTreeSet::new();
        for local in state.locals.values() {
            self.tick(span)?;
            outer.insert(local.id);
        }
        let mut types = Vec::new();
        let mut names = BTreeSet::new();
        for (index, parameter) in parameters.iter().enumerate() {
            self.tick(parameter.span)?;
            if !names.insert(&parameter.name) { return Err(error(parameter.span, "duplicate closure parameter")); }
            let hint = context.and_then(|(_, values, _)| values.get(index));
            let ty = parameter.ty.as_ref().or(hint).ok_or_else(|| unsupported(
                parameter.span, "generic factory closure parameter requires a declared or opaque-context type",
            ))?;
            if let (Some(annotation), Some(hint)) = (&parameter.ty, hint) {
                same(annotation, hint, parameter.span, self.budget)?;
            }
            self.budget.ty(ty)?;
            types.push(ty.clone());
            self.bind_local(&mut child, parameter.name.clone(), ty.clone())?;
        }
        let result = annotation.or_else(|| context.map(|(_, _, result)| result));
        if let (Some(annotation), Some((_, _, expected))) = (annotation, context) {
            same(annotation, expected, span, self.budget)?;
        }
        if let Some(result) = result { self.budget.ty(result)?; }
        let saved_result = std::mem::replace(&mut self.result, result.cloned().unwrap_or_else(|| unit(span)));
        let saved_loops = std::mem::take(&mut self.loops);
        self.factory_scopes.push(Scope { outer, captures: BTreeMap::new(), result: result.cloned() });
        match body {
            ArrowBody::Expression(expression) => self.factory_body_return(Some(expression), &mut child, span)?,
            ArrowBody::Block(block) => {
                if self.block(block, &mut child)?.fallthrough {
                    self.factory_body_return(None, &mut child, span)?;
                }
            }
        }
        let scope = self.factory_scopes.pop().expect("factory closure scope");
        self.result = saved_result;
        self.loops = saved_loops;
        let result = scope.result.unwrap_or_else(|| unit(span));
        self.factory_signature(&types, &result)?;
        let mut kind = CallableKind::Fn;
        for (id, usage) in scope.captures {
            self.tick(span)?;
            let local = state.locals.values().find(|local| local.id == id)
                .ok_or_else(|| error(span, "factory capture lost its lexical position"))?;
            self.budget.ty(&local.ty)?;
            let local = local.clone();
            if !move_capture {
                return Err(unsupported(span, "retained generic factory captures require owned move values"));
            }
            if matches!(group(&local.ty).kind, TypeKind::Reference { .. }) {
                return Err(unsupported(span, "owning factory cannot retain an external address"));
            }
            if state.moved.contains(&id) {
                return Err(error(span, "factory capture uses an already consumed position"));
            }
            if usage.consumed { kind = CallableKind::FnOnce; }
            else if usage.written && kind != CallableKind::FnOnce { kind = CallableKind::FnMut; }
            let consumes = local.owned_resource || !self.known_copy(&local.ty)?;
            self.factory_use(id, false, consumes, span)?;
            if consumes {
                self.alias_invalidate(state, id, None, span)?;
                state.moved.insert(id);
            }
        }
        let name = match kind {
            CallableKind::Fn => "Fn", CallableKind::FnMut => "FnMut", CallableKind::FnOnce => "FnOnce",
            CallableKind::FunctionPointer => unreachable!(),
        };
        Ok(TypeRef { span, kind: TypeKind::Impl(vec![TypeRef {
            span, kind: TypeKind::Named { path: vec![name.into()], args: vec![
                GenericArg::Type(TypeRef { span, kind: TypeKind::Tuple(types) }),
                GenericArg::Type(result),
            ] },
        }]) })
    }

    fn factory_origin(&mut self, expression: &Expr, state: &State) -> R<Option<Span>> {
        self.tick(expression.span)?;
        Ok(match &expression.kind {
            ExprKind::Closure { .. } => Some(expression.span),
            ExprKind::Name(name) => state.locals.get(name).and_then(|local| self.factory_values.get(&local.id)).copied(),
            ExprKind::Group(inner) => self.factory_origin(inner, state)?,
            ExprKind::Call { callee, .. } => stores::call_name(callee)
                .and_then(|name| self.signatures.get(name))
                .filter(|function| function.return_type.as_ref().is_some_and(|ty| callable::opaque_signature(ty).is_some()))
                .map(|function| function.span),
            ExprKind::If { then_value, else_value, .. } => {
                let a = self.factory_origin(then_value, state)?;
                let b = self.factory_origin(else_value, state)?;
                if a != b { return Err(error(expression.span, "opaque factory returns distinct closure definitions")); }
                a
            }
            _ => None,
        })
    }

    pub(super) fn factory_bind_value(&mut self, id: usize, expression: &Expr, state: &State) -> R<()> {
        if !self.callable.as_ref().is_some_and(|bounds| bounds.owned_factory) { return Ok(()) }
        if let Some(origin) = self.factory_origin(expression, state)? {
            self.tick(expression.span)?;
            self.factory_values.insert(id, origin);
        }
        Ok(())
    }

    pub(super) fn factory_result_identity(&mut self, expression: &Expr, state: &State) -> R<()> {
        let origin = self.factory_origin(expression, state)?.ok_or_else(|| unsupported(
            expression.span, "owning callable factory must return a fresh grounded environment, not its input F",
        ))?;
        if self.factory_return_identity.is_some_and(|prior| prior != origin) {
            return Err(error(expression.span, "opaque factory returns distinct closure definitions"));
        }
        self.factory_return_identity = Some(origin);
        Ok(())
    }
}

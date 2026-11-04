//! Definition-time checking for the one-owned-F factory. These symbolic
//! positions restrict source admission; concrete IR remains independently
//! checked by Types, Analyze and Verify.
use super::*;
use zil_ir::CallableKind;

#[derive(Default)]
struct Use {
    written: bool,
    consumed: bool,
    partial: BTreeSet<usize>,
}

pub(super) struct Scope {
    outer: BTreeSet<usize>,
    captures: BTreeMap<usize, Use>,
    result: Option<TypeRef>,
    exits: BTreeSet<usize>,
}

impl Check<'_, '_, '_> {
    pub(super) fn factory_same_result(
        &mut self,
        actual: &TypeRef,
        expected: &TypeRef,
        span: Span,
    ) -> R<()> {
        let (Some((ak, ap, ar)), Some((ek, ep, er))) = (
            callable::opaque_signature(actual),
            callable::opaque_signature(expected),
        ) else {
            return Err(error(
                span,
                "owning factory result requires a fresh opaque callable",
            ));
        };
        if ak != ek || ap.len() != ep.len() {
            return Err(error(
                span,
                "factory closure kind and signature must exactly match its opaque declaration",
            ));
        }
        for (a, e) in ap.iter().zip(ep) {
            same(a, e, span, self.budget)?;
        }
        same(ar, er, span, self.budget)
    }
    fn factory_copy(&mut self, ty: &TypeRef) -> R<()> {
        copy::require(
            ty,
            self.nominals,
            &self.bounds,
            &self.variables,
            self.budget,
        )
    }

    fn factory_signature(&mut self, parameters: &[TypeRef], result: &TypeRef) -> R<()> {
        for parameter in parameters {
            self.tick(parameter.span)?;
            let value = match &group(parameter).kind {
                TypeKind::Reference {
                    inner,
                    lifetime: None,
                    ..
                } => match &group(inner).kind {
                    TypeKind::Array {
                        element,
                        length: None,
                    } => element.as_ref(),
                    _ => inner.as_ref(),
                },
                _ => parameter,
            };
            self.factory_copy(value)?;
        }
        self.factory_copy(result)
    }

    fn local_signature(&mut self, parameters: &[TypeRef], result: &TypeRef) -> R<()> {
        let mut owned = false;
        for (index, ty) in parameters.iter().chain(std::iter::once(result)).enumerate() {
            if dependent(ty, &self.variables) {
                return Err(unsupported(
                    ty.span,
                    "local concrete callable signatures cannot contain a helper type parameter",
                ));
            }
            // Retain the old Copy/address signature and its exact work. Only
            // a failed old signature enters the concrete owned-data check.
            let value = match &group(ty).kind {
                TypeKind::Reference {
                    inner,
                    lifetime: None,
                    ..
                } if index < parameters.len() => match &group(inner).kind {
                    TypeKind::Array {
                        element,
                        length: None,
                    } => element.as_ref(),
                    _ => inner.as_ref(),
                },
                _ => ty,
            };
            match self.factory_copy(value) {
                Ok(()) => {}
                Err(diagnostic)
                    if diagnostic.category == zil_base::ResultCategory::ResourceLimit =>
                {
                    return Err(diagnostic);
                }
                Err(diagnostic) => {
                    self.budget.ty(ty)?;
                    let admitted = self.local_owned_signature_type(ty, &mut BTreeSet::new(), 0)?;
                    if !admitted {
                        return Err(diagnostic);
                    }
                    owned = true;
                }
            }
        }
        if owned
            && parameters
                .iter()
                .chain(std::iter::once(result))
                .any(|ty| matches!(group(ty).kind, TypeKind::Reference { .. }))
        {
            return Err(unsupported(
                result.span,
                "owned local callable signatures cannot mix address inputs or results",
            ));
        }
        Ok(())
    }

    fn local_owned_signature_type(
        &mut self,
        ty: &TypeRef,
        active: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<bool> {
        self.budget.node(ty.span, depth)?;
        if dependent(ty, &self.variables) {
            return Ok(false);
        }
        if let TypeKind::Error { error, result } = &group(ty).kind {
            return Ok(self.local_owned_signature_type(error, active, depth + 1)?
                && self.local_owned_signature_type(result, active, depth + 1)?);
        }
        match self.factory_copy(ty) {
            Ok(()) => return Ok(true),
            Err(diagnostic) if diagnostic.category == zil_base::ResultCategory::ResourceLimit => {
                return Err(diagnostic);
            }
            Err(_) => {}
        }
        let TypeKind::Named { path, args } = &group(ty).kind else {
            return Ok(false);
        };
        let key = type_key(ty, self.budget)?;
        self.budget.text(&key, ty.span)?;
        if !active.insert(key.clone()) {
            return Err(error(
                ty.span,
                "local owned callable signature must have finite layout",
            ));
        }
        let fields = match self.nominals.get(&path.join(".")).copied() {
            Some(Item {
                kind: ItemKind::Aggregate(aggregate),
                ..
            }) if aggregate.kind == AggregateKind::Package => {
                if aggregate.generics.len() != args.len() {
                    return Err(error(ty.span, "local owned signature type arity differs"));
                }
                let mut substitutions = BTreeMap::new();
                for (parameter, argument) in aggregate.generics.iter().zip(args) {
                    let GenericArg::Type(actual) = argument else {
                        return Ok(false);
                    };
                    self.budget.ty(actual)?;
                    self.budget.text(&parameter.name, parameter.span)?;
                    substitutions.insert(parameter.name.clone(), actual.clone());
                }
                for field in &aggregate.fields {
                    self.budget.node(field.span, depth + 1)?;
                    let actual = substituted_type(&field.ty, &substitutions, self.budget)?;
                    let valid = if field.manages {
                        self.local_managed_signature_type(&actual, depth + 1)?
                    } else {
                        self.local_owned_signature_type(&actual, active, depth + 1)?
                    };
                    if !valid {
                        return Ok(false);
                    }
                }
                true
            }
            Some(Item {
                kind: ItemKind::Enum(enumeration),
                ..
            }) if enumeration.generics.is_empty() && args.is_empty() => {
                for variant in &enumeration.variants {
                    self.budget.node(variant.span, depth + 1)?;
                    match &variant.fields {
                        VariantFields::Unit => {}
                        VariantFields::Tuple(fields) => {
                            for field in fields {
                                let valid = if field.manages {
                                    self.local_managed_signature_type(field, depth + 1)?
                                } else {
                                    self.local_owned_signature_type(field, active, depth + 1)?
                                };
                                if !valid {
                                    return Ok(false);
                                }
                            }
                        }
                        VariantFields::Record(_) => return Ok(false),
                    }
                }
                true
            }
            _ => false,
        };
        active.remove(&key);
        Ok(fields)
    }
    fn local_managed_signature_type(&mut self, ty: &TypeRef, depth: usize) -> R<bool> {
        self.budget.node(ty.span, depth)?;
        if dependent(ty, &self.variables) {
            return Ok(false);
        }
        if let Some(element) = stores::element(ty) {
            stores::concrete_element(
                element,
                self.nominals,
                self.budget,
                (
                    &self.bounds,
                    &self.variables,
                    callable::StoreCallbacks::None,
                ),
            )?;
            return Ok(true);
        }
        let TypeKind::Named { path, args } = &group(ty).kind else {
            return Ok(false);
        };
        if matches!(path.as_slice(), [name] if name == "ByteBuffer" || name == "TaskI32")
            && args.is_empty()
        {
            return Ok(true);
        }
        let Some(Item {
            kind: ItemKind::Aggregate(resource),
            ..
        }) = self.nominals.get(&path.join(".")).copied()
        else {
            return Ok(false);
        };
        if resource.kind != AggregateKind::Resource || resource.generics.len() != args.len() {
            return Ok(false);
        }
        let mut substitutions = BTreeMap::new();
        for (parameter, argument) in resource.generics.iter().zip(args) {
            let GenericArg::Type(actual) = argument else {
                return Ok(false);
            };
            self.budget.ty(actual)?;
            self.budget.text(&parameter.name, parameter.span)?;
            substitutions.insert(parameter.name.clone(), actual.clone());
        }
        for field in &resource.fields {
            self.budget.node(field.span, depth + 1)?;
            if field.manages {
                return Ok(false);
            }
            let actual = substituted_type(&field.ty, &substitutions, self.budget)?;
            self.factory_copy(&actual)?;
        }
        Ok(true)
    }

    pub(super) fn factory_header(&mut self, function: &Function) -> R<()> {
        let bounds = self.callable.as_ref().expect("factory bound");
        let [bound] = bounds.roles.as_slice() else {
            return Err(unsupported(
                function.span,
                "owning factory requires one direct F input",
            ));
        };
        for parameter in &bound.parameters {
            self.budget.ty(parameter)?;
        }
        self.budget.ty(&bound.result)?;
        let bound = bound.clone();
        self.factory_signature(&bound.parameters, &bound.result)?;
        let (_, parameters, result) =
            callable::opaque_signature(function.return_type.as_ref().expect("factory result"))
                .expect("factory callable signature");
        self.factory_signature(parameters, result)?;
        for (index, parameter) in function.params.iter().enumerate() {
            self.tick(parameter.span)?;
            if index != bound.index && dependent(&parameter.ty, &self.variables) {
                return Err(unsupported(
                    parameter.span,
                    "ordinary factory parameters cannot hide callable type inputs",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn factory_use(
        &mut self,
        id: usize,
        written: bool,
        consumed: bool,
        span: Span,
    ) -> R<()> {
        let Some(scope) = self.factory_scopes.last_mut() else {
            return Ok(());
        };
        self.budget.node(span, 0)?;
        if scope.outer.contains(&id) {
            self.budget.node(span, 0)?;
            let usage = scope.captures.entry(id).or_default();
            usage.written |= written;
            usage.consumed |= consumed;
        }
        Ok(())
    }

    pub(super) fn factory_place(
        &mut self,
        expression: &Expr,
        state: &State,
        written: bool,
    ) -> R<()> {
        if self.factory_scopes.is_empty() {
            return Ok(());
        }
        self.tick(expression.span)?;
        match &expression.kind {
            ExprKind::Name(name) => {
                if let Some(local) = state.locals.get(name) {
                    self.factory_use(local.id, written, false, expression.span)?;
                }
            }
            ExprKind::Group(inner) | ExprKind::Unary { operand: inner, .. } => {
                self.factory_place(inner, state, written)?
            }
            ExprKind::Member { object, .. }
            | ExprKind::Index { object, .. }
            | ExprKind::Slice { object, .. } => self.factory_place(object, state, written)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn factory_field_use(&mut self, owner: usize, slot: usize, span: Span) -> R<()> {
        let Some(scope) = self.factory_scopes.last_mut() else {
            return Ok(());
        };
        self.budget.node(span, 0)?;
        if scope.outer.contains(&owner) {
            self.budget.node(span, 0)?;
            let usage = scope.captures.entry(owner).or_default();
            usage.written = true;
            usage.partial.insert(slot);
        }
        Ok(())
    }

    pub(super) fn factory_body_return(
        &mut self,
        expression: Option<&Expr>,
        state: &mut State,
        span: Span,
    ) -> R<()> {
        if let Some(expected) = &self.factory_scopes.last().expect("closure scope").result {
            self.budget.ty(expected)?;
        }
        let expected = self
            .factory_scopes
            .last()
            .expect("closure scope")
            .result
            .clone();
        let actual = if let Some(expression) = expression {
            self.expr(expression, state, expected.as_ref(), false)?
        } else {
            unit(span)
        };
        if let Some(expected) = expected {
            same_return(&actual, &expected, span, self.budget)?;
        } else {
            self.budget.ty(&actual)?;
            self.result = actual.clone();
            self.factory_scopes
                .last_mut()
                .expect("closure scope")
                .result = Some(actual);
        }
        for _ in &state.moved {
            self.tick(span)?;
        }
        self.factory_scopes
            .last_mut()
            .expect("closure scope")
            .exits
            .extend(state.moved.iter().copied());
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn factory_closure(
        &mut self,
        move_capture: bool,
        parameters: &[ClosureParam],
        annotation: Option<&TypeRef>,
        body: &ArrowBody,
        expected: Option<&TypeRef>,
        state: &mut State,
        span: Span,
        concrete_local: bool,
    ) -> R<TypeRef> {
        let context = expected.and_then(callable::opaque_signature);
        if context.is_some_and(|(_, parameters_, _)| parameters_.len() != parameters.len()) {
            return Err(error(
                span,
                "factory closure argument count differs from its opaque result",
            ));
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
            if !names.insert(&parameter.name) {
                return Err(error(parameter.span, "duplicate closure parameter"));
            }
            let hint = context.and_then(|(_, values, _)| values.get(index));
            let ty = parameter.ty.as_ref().or(hint).ok_or_else(|| {
                unsupported(
                    parameter.span,
                    "generic factory closure parameter requires a declared or opaque-context type",
                )
            })?;
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
        if let Some(result) = result {
            self.budget.ty(result)?;
        }
        let saved_result = std::mem::replace(
            &mut self.result,
            result.cloned().unwrap_or_else(|| unit(span)),
        );
        let saved_loops = std::mem::take(&mut self.loops);
        self.factory_scopes.push(Scope {
            outer,
            captures: BTreeMap::new(),
            result: result.cloned(),
            exits: BTreeSet::new(),
        });
        match body {
            ArrowBody::Expression(expression) => {
                self.factory_body_return(Some(expression), &mut child, span)?
            }
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
        if concrete_local {
            self.local_signature(&types, &result)?;
        } else {
            self.factory_signature(&types, &result)?;
        }
        let mut kind = CallableKind::Fn;
        let mut scoped_copy = concrete_local;
        for (id, usage) in scope.captures {
            self.tick(span)?;
            for _ in 0..state.locals.len() {
                self.tick(span)?;
            }
            let local = state
                .locals
                .values()
                .find(|local| local.id == id)
                .ok_or_else(|| error(span, "factory capture lost its lexical position"))?;
            self.budget.ty(&local.ty)?;
            let local = local.clone();
            if concrete_local && (!move_capture || usage.consumed || !self.known_copy(&local.ty)?) {
                scoped_copy = false;
            }
            if concrete_local && dependent(&local.ty, &self.variables) {
                return Err(error(
                    span,
                    "a scoped local body cannot capture a callable parameter",
                ));
            }
            if !concrete_local && !move_capture && !usage.consumed && usage.partial.is_empty() {
                return Err(unsupported(
                    span,
                    "retained generic factory captures require explicit move or an actual owning use",
                ));
            }
            if !concrete_local && matches!(group(&local.ty).kind, TypeKind::Reference { .. }) {
                return Err(unsupported(
                    span,
                    "owning factory cannot retain an external address",
                ));
            }
            if state.moved.contains(&id) {
                return Err(error(
                    span,
                    "factory capture uses an already consumed position",
                ));
            }
            if self.concrete_helper_resource(&local.ty) && !local.owned_resource {
                return Err(unsupported(
                    span,
                    "retained factory resource capture requires an actual owner, not a description",
                ));
            }
            if stores::element(&local.ty).is_some() && !state.store_owners.contains(&id) {
                return Err(unsupported(
                    span,
                    "retained factory Store capture requires an actual owner, not a description",
                ));
            }
            for _ in &usage.partial {
                self.tick(span)?;
            }
            if usage.consumed || usage.partial.iter().any(|slot| scope.exits.contains(slot)) {
                kind = CallableKind::FnOnce;
            } else if usage.written && kind != CallableKind::FnOnce {
                kind = CallableKind::FnMut;
            }
            let consumes = local.owned_resource
                || state.store_owners.contains(&id)
                || !self.known_copy(&local.ty)?;
            self.factory_use(id, false, consumes, span)?;
            if consumes {
                self.alias_invalidate(state, id, None, span)?;
                state.moved.insert(id);
                if let Some(origins) = state.store_origins.get(&id) {
                    for _ in origins {
                        self.tick(span)?;
                    }
                    state.invalidated_stores.extend(origins);
                }
            }
        }
        if scoped_copy {
            self.tick(span)?;
            self.scoped_copy_closures.insert(span);
        }
        let name = match kind {
            CallableKind::Fn => "Fn",
            CallableKind::FnMut => "FnMut",
            CallableKind::FnOnce => "FnOnce",
            CallableKind::FunctionPointer => unreachable!(),
        };
        Ok(TypeRef {
            span,
            kind: TypeKind::Impl(vec![TypeRef {
                span,
                kind: TypeKind::Named {
                    path: vec![name.into()],
                    args: vec![
                        GenericArg::Type(TypeRef {
                            span,
                            kind: TypeKind::Tuple(types),
                        }),
                        GenericArg::Type(result),
                    ],
                },
            }]),
        })
    }

    fn factory_origin(&mut self, expression: &Expr, state: &State) -> R<Option<Span>> {
        self.tick(expression.span)?;
        Ok(match &expression.kind {
            ExprKind::Closure { .. } => Some(expression.span),
            ExprKind::Name(name) => state
                .locals
                .get(name)
                .and_then(|local| self.factory_values.get(&local.id))
                .copied(),
            ExprKind::Ascribed { ty, value } => {
                self.budget.ty(ty)?;
                self.factory_origin(value, state)?
            }
            ExprKind::Group(inner) => self.factory_origin(inner, state)?,
            ExprKind::Call { callee, .. } => stores::call_name(callee)
                .and_then(|name| self.signatures.get(name))
                .filter(|function| {
                    function
                        .return_type
                        .as_ref()
                        .is_some_and(|ty| callable::opaque_signature(ty).is_some())
                })
                .map(|function| function.span),
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                let a = self.factory_origin(then_value, state)?;
                let b = self.factory_origin(else_value, state)?;
                if a != b {
                    return Err(error(
                        expression.span,
                        "opaque factory returns distinct closure definitions",
                    ));
                }
                a
            }
            _ => None,
        })
    }

    pub(super) fn factory_bind_value(
        &mut self,
        id: usize,
        expression: &Expr,
        state: &State,
    ) -> R<()> {
        if self.scoped_body_closures.is_none()
            && !self
                .callable
                .as_ref()
                .is_some_and(|bounds| bounds.owned_factory)
        {
            return Ok(());
        }
        if let Some(origin) = self.factory_origin(expression, state)? {
            self.tick(expression.span)?;
            self.factory_values.insert(id, origin);
        }
        Ok(())
    }

    pub(super) fn factory_result_identity(&mut self, expression: &Expr, state: &State) -> R<()> {
        let origin = self.factory_origin(expression, state)?.ok_or_else(|| {
            unsupported(
                expression.span,
                "owning callable factory must return a fresh grounded environment, not its input F",
            )
        })?;
        if self
            .factory_return_identity
            .is_some_and(|prior| prior != origin)
        {
            return Err(error(
                expression.span,
                "opaque factory returns distinct closure definitions",
            ));
        }
        self.factory_return_identity = Some(origin);
        Ok(())
    }
}

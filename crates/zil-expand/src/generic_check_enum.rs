//! Definition-time enum constructor and match checking, before instantiation.
use super::super::enums as syntax_enum;
use super::*;

#[derive(Clone, PartialEq, Eq)]
enum Ctor {
    Tuple,
    Bool(bool),
    Integer(i128),
    Variant(String),
    Sum(String),
    Error(bool),
    Opaque,
}
#[derive(Clone)]
enum Pat {
    Any,
    Ctor(Ctor, Vec<Pat>),
}

type ScopedEnumActuals = Option<(Vec<GenericArg>, BTreeMap<String, TypeRef>)>;

// This read is covered by the explicit Type actual traversal paid in
// mixed_enum_actuals. It observes original source syntax before any nominal
// type erasure; a generated name never supplies a caller class.
pub(super) fn scoped_nominal_actual(ty: &TypeRef) -> bool {
    match &ty.kind {
        TypeKind::Named { args, .. } => args.iter().any(|arg| match arg {
            GenericArg::Lifetime(_) => true,
            GenericArg::Type(inner) => scoped_nominal_actual(inner),
            GenericArg::Const(_) => false,
        }),
        TypeKind::Dyn { args, bindings, .. } => {
            !bindings.is_empty()
                || args.iter().any(|arg| match arg {
                    GenericArg::Lifetime(_) => true,
                    GenericArg::Type(inner) => scoped_nominal_actual(inner),
                    GenericArg::Const(_) => false,
                })
        }
        TypeKind::Reference {
            lifetime, inner, ..
        } => lifetime.is_some() || scoped_nominal_actual(inner),
        TypeKind::Group(inner) | TypeKind::Optional(inner) => scoped_nominal_actual(inner),
        TypeKind::Tuple(fields) | TypeKind::Union(fields) | TypeKind::Impl(fields) => {
            fields.iter().any(scoped_nominal_actual)
        }
        TypeKind::Array { element, .. } => scoped_nominal_actual(element),
        TypeKind::Error { error, result } => {
            scoped_nominal_actual(error) || scoped_nominal_actual(result)
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            params.iter().any(scoped_nominal_actual) || scoped_nominal_actual(result)
        }
        TypeKind::ForLifetime { .. } => true,
        _ => false,
    }
}

impl Check<'_, '_, '_> {
    /// Mixed nominal ordinals come from the actual constructor or its full
    /// source context. Only Type actuals enter the substitution/bound map.
    fn mixed_enum_actuals(
        &mut self,
        decl: &EnumDecl,
        explicit: Option<&[GenericArg]>,
        expected: Option<&TypeRef>,
        span: Span,
        prepaid_explicit: &mut bool,
        prepaid_context: &mut Option<BTreeMap<String, TypeRef>>,
    ) -> R<ScopedEnumActuals> {
        let all_type = decl
            .generics
            .iter()
            .all(|p| matches!(p.kind, GenericParamKind::Type(_)));
        if all_type {
            if let Some(explicit) = explicit {
                let mut scoped = false;
                for argument in explicit {
                    let GenericArg::Type(ty) = argument else {
                        return Ok(None);
                    };
                    // Reuse the payment in the legacy explicit branch.
                    self.budget.ty(ty)?;
                    scoped |= scoped_nominal_actual(ty);
                }
                *prepaid_explicit = true;
                if !scoped {
                    return Ok(None);
                }
            } else {
                let Some(source) = expected else {
                    return Ok(None);
                };
                let TypeKind::Named { path, .. } = &group(source).kind else {
                    return Ok(None);
                };
                if path.as_slice() != [decl.name.as_str()] {
                    return Ok(None);
                }
                // Context's original unify traverses and pays every actual.
                // Keep its result so a closed all-Type request pays it once.
                let mut found = BTreeMap::new();
                syntax_enum::context(decl, expected, &mut found, self.budget)?;
                let scoped = found.values().any(scoped_nominal_actual);
                if !scoped {
                    *prepaid_context = Some(found);
                    return Ok(None);
                }
            }
        }
        let context = expected.and_then(|expected| match &group(expected).kind {
            TypeKind::Named { path, args } if path.as_slice() == [decl.name.as_str()] => {
                Some(args.as_slice())
            }
            _ => None,
        });
        let args = explicit.or(context).ok_or_else(|| {
            error(
                span,
                "qualified enum needs its complete source actual vector",
            )
        })?;
        if args.len() != decl.generics.len() {
            return Err(error(
                span,
                "qualified enum actual arity differs from declaration",
            ));
        }
        let args = args.to_vec();
        let found = nominal_type_actuals(
            &decl.generics,
            &args,
            self.source_definition,
            self.budget,
            span,
        )?;
        if let Some(context) = context {
            let actual = TypeRef {
                span,
                kind: TypeKind::Named {
                    path: vec![decl.name.clone()],
                    args: args.clone(),
                },
            };
            let expected = TypeRef {
                span,
                kind: TypeKind::Named {
                    path: vec![decl.name.clone()],
                    args: context.to_vec(),
                },
            };
            role_check::same(&actual, &expected, span, self.class_scope, self.budget)?;
        } else if expected.is_some() {
            return Err(error(
                span,
                "qualified enum constructor has a different complete context",
            ));
        }
        Ok(Some((args, found)))
    }

    pub(super) fn enum_expression(
        &mut self,
        e: &Expr,
        state: &mut State,
        expected: Option<&TypeRef>,
    ) -> R<Option<TypeRef>> {
        if matches!(e.kind, ExprKind::Record { .. }) {
            return self.record_enum_expression(e, state, expected);
        }
        let (callee, args) = match &e.kind {
            ExprKind::Call { callee, arguments } => (callee.as_ref(), arguments.as_slice()),
            _ => (e, &[][..]),
        };
        let Some((name, variant, explicit)) = syntax_enum::constructor(callee) else {
            return Ok(None);
        };
        if state.locals.contains_key(name) {
            return Ok(None);
        }
        let Some(decl) = self.enums.get(name).copied() else {
            return Ok(None);
        };
        let fields = syntax_enum::fields(decl, variant, e.span)?;
        if args.len() != fields.len() {
            return Err(error(
                e.span,
                "parametric enum constructor payload arity mismatch",
            ));
        }
        let mut prepaid_explicit = false;
        let mut prepaid_context = None;
        if let Some((full, found)) = self.mixed_enum_actuals(
            decl,
            explicit,
            expected,
            e.span,
            &mut prepaid_explicit,
            &mut prepaid_context,
        )? {
            self.traits.check_arguments_scoped(
                &decl.generics,
                &found,
                (
                    &self.bounds,
                    &self.variables,
                    callable::StoreCallbacks::None,
                ),
                self.budget,
                e.span,
                None,
                Some(self.source_definition),
            )?;
            for (arg, field) in args.iter().zip(fields) {
                if arg.spread {
                    return Err(unsupported(arg.span, "enum payload tuple spread"));
                }
                let want = nominal_field_type(
                    &decl.generics,
                    &full,
                    field,
                    self.source_definition,
                    self.budget,
                )?;
                let actual = self.expr(&arg.value, state, Some(&want), false)?;
                role_check::same(&actual, &want, arg.span, self.class_scope, self.budget)?;
            }
            let actual = TypeRef {
                span: e.span,
                kind: TypeKind::Named {
                    path: vec![decl.name.clone()],
                    args: full,
                },
            };
            syntax_enum::check_injection(&actual, expected, self.budget)?;
            return Ok(Some(actual));
        }
        let vars = decl
            .generics
            .iter()
            .map(|p| p.name.clone())
            .collect::<BTreeSet<_>>();
        let already_context = prepaid_context.is_some();
        let mut found = prepaid_context.take().unwrap_or_default();
        if let Some(explicit) = explicit {
            if explicit.len() != decl.generics.len() {
                return Err(error(
                    e.span,
                    "parametric enum type argument arity mismatch",
                ));
            }
            for (p, arg) in decl.generics.iter().zip(explicit) {
                let GenericArg::Type(t) = arg else {
                    return Err(unsupported(e.span, "const enum type argument"));
                };
                if !prepaid_explicit {
                    self.budget.ty(t)?;
                }
                found.insert(p.name.clone(), t.clone());
            }
        }
        if !already_context {
            syntax_enum::context(decl, expected, &mut found, self.budget)?;
        }
        for (arg, field) in args.iter().zip(fields) {
            if arg.spread {
                return Err(unsupported(arg.span, "enum payload tuple spread"));
            }
            let hint = syntax_enum::substituted(field, &found, self.budget)?;
            let unresolved = vars
                .iter()
                .any(|v| !found.contains_key(v) && syntax_enum::contains(&hint, v));
            let actual = self.expr(&arg.value, state, (!unresolved).then_some(&hint), false)?;
            unify(field, &actual, &vars, &mut found, arg.span, self.budget)?;
        }
        let actual = syntax_enum::application(decl, &found, e.span, self.budget)?;
        if self.budget.has_enum_profiles() {
            self.traits.check_arguments(
                &decl.generics,
                &found,
                (
                    &self.bounds,
                    &self.variables,
                    callable::StoreCallbacks::None,
                ),
                self.budget,
                e.span,
                None,
            )?;
        }
        syntax_enum::check_injection(&actual, expected, self.budget)?;
        Ok(Some(actual))
    }

    fn record_enum_expression(
        &mut self,
        e: &Expr,
        state: &mut State,
        expected: Option<&TypeRef>,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Record {
            ty: Some(callee),
            base,
            fields: supplied,
        } = &e.kind
        else {
            return Ok(None);
        };
        let Some((name, variant, explicit)) = syntax_enum::constructor(callee) else {
            return Ok(None);
        };
        let Some(declaration) = self.enums.get(name).copied() else {
            return Ok(None);
        };
        if state.locals.contains_key(name) {
            return Err(error(
                e.span,
                "record enum namespace is shadowed by a value",
            ));
        }
        let declared = record_enums::fields(declaration, variant, e.span, self.budget)?;
        if base.is_some() {
            return Err(unsupported(e.span, "record enum spread is not implemented"));
        }
        let order = record_enums::order(
            declared,
            supplied.iter().map(|f| (f.name.as_str(), f.span)),
            e.span,
            self.budget,
        )?;
        let mut prepaid_explicit = false;
        let mut prepaid_context = None;
        if let Some((full, found)) = self.mixed_enum_actuals(
            declaration,
            explicit,
            expected,
            e.span,
            &mut prepaid_explicit,
            &mut prepaid_context,
        )? {
            self.traits.check_arguments_scoped(
                &declaration.generics,
                &found,
                (
                    &self.bounds,
                    &self.variables,
                    callable::StoreCallbacks::None,
                ),
                self.budget,
                e.span,
                None,
                Some(self.source_definition),
            )?;
            for (supplied, index) in supplied.iter().zip(&order) {
                let shorthand;
                let value = if let Some(value) = &supplied.value {
                    value
                } else {
                    self.budget.node(supplied.span, 0)?;
                    self.budget.text(&supplied.name, supplied.span)?;
                    shorthand = Expr {
                        kind: ExprKind::Name(supplied.name.clone()),
                        span: supplied.span,
                    };
                    &shorthand
                };
                let want = nominal_field_type(
                    &declaration.generics,
                    &full,
                    &declared[*index].ty,
                    self.source_definition,
                    self.budget,
                )?;
                let actual = self.expr(value, state, Some(&want), false)?;
                role_check::same(&actual, &want, supplied.span, self.class_scope, self.budget)?;
            }
            let actual = TypeRef {
                span: e.span,
                kind: TypeKind::Named {
                    path: vec![declaration.name.clone()],
                    args: full,
                },
            };
            syntax_enum::check_injection(&actual, expected, self.budget)?;
            return Ok(Some(actual));
        }
        let variables = record_enums::variables(declaration, self.budget)?;
        let already_context = prepaid_context.is_some();
        let mut found = prepaid_context.take().unwrap_or_default();
        if let Some(explicit) = explicit {
            if explicit.len() != declaration.generics.len() {
                return Err(error(e.span, "record enum type argument arity mismatch"));
            }
            for (parameter, argument) in declaration.generics.iter().zip(explicit) {
                let GenericArg::Type(ty) = argument else {
                    return Err(unsupported(e.span, "const enum type argument"));
                };
                self.budget.text(&parameter.name, parameter.span)?;
                if !prepaid_explicit {
                    self.budget.ty(ty)?;
                }
                found.insert(parameter.name.clone(), ty.clone());
            }
        }
        if !already_context {
            syntax_enum::context(declaration, expected, &mut found, self.budget)?;
        }
        for (supplied, index) in supplied.iter().zip(order) {
            let shorthand;
            let value = if let Some(value) = &supplied.value {
                value
            } else {
                self.budget.node(supplied.span, 0)?;
                self.budget.text(&supplied.name, supplied.span)?;
                shorthand = Expr {
                    kind: ExprKind::Name(supplied.name.clone()),
                    span: supplied.span,
                };
                &shorthand
            };
            let field = &declared[index];
            let hint = syntax_enum::substituted(&field.ty, &found, self.budget)?;
            let unresolved = record_enums::unresolved(&variables, &found, &hint, self.budget)?;
            let actual = self.expr(value, state, (!unresolved).then_some(&hint), false)?;
            unify(
                &field.ty,
                &actual,
                &variables,
                &mut found,
                supplied.span,
                self.budget,
            )?;
        }
        self.traits.check_arguments(
            &declaration.generics,
            &found,
            (
                &self.bounds,
                &self.variables,
                callable::StoreCallbacks::None,
            ),
            self.budget,
            e.span,
            None,
        )?;
        let actual = syntax_enum::application(declaration, &found, e.span, self.budget)?;
        syntax_enum::check_injection(&actual, expected, self.budget)?;
        Ok(Some(actual))
    }
    fn enum_fields(
        &mut self,
        ty: &TypeRef,
        variant: &str,
        mut managed: Option<&mut Vec<usize>>,
    ) -> R<Vec<TypeRef>> {
        let TypeKind::Named { path, args } = &group(ty).kind else {
            return Err(error(
                ty.span,
                "constructor pattern requires a declared enum",
            ));
        };
        let decl = self
            .enums
            .get(&path.join("."))
            .copied()
            .ok_or_else(|| error(ty.span, "unknown enum in generic pattern"))?;
        if decl.generics.len() != args.len() {
            return Err(error(ty.span, "generic enum pattern type arity mismatch"));
        }
        if decl
            .generics
            .iter()
            .any(|p| !matches!(p.kind, GenericParamKind::Type(_)))
        {
            nominal_type_actuals(
                &decl.generics,
                args,
                self.source_definition,
                self.budget,
                ty.span,
            )?;
            if self.budget.has_enum_profiles()
                && let VariantFields::Record(fields) =
                    &record_enums::variant(decl, variant, ty.span, self.budget)?.fields
            {
                return fields
                    .iter()
                    .map(|field| {
                        nominal_field_type(
                            &decl.generics,
                            args,
                            &field.ty,
                            self.source_definition,
                            self.budget,
                        )
                    })
                    .collect();
            }
            return syntax_enum::fields(decl, variant, ty.span)?
                .iter()
                .map(|field| {
                    if field.manages {
                        return Err(unsupported(field.span, "owned generic enum payload"));
                    }
                    nominal_field_type(
                        &decl.generics,
                        args,
                        &field.ty,
                        self.source_definition,
                        self.budget,
                    )
                })
                .collect();
        }
        let mut found = BTreeMap::new();
        let mut scoped_type_actual = false;
        for (p, arg) in decl.generics.iter().zip(args) {
            let GenericArg::Type(t) = arg else {
                return Err(unsupported(ty.span, "const enum argument"));
            };
            self.budget.ty(t)?;
            scoped_type_actual |= scoped_nominal_actual(t);
            found.insert(p.name.clone(), t.clone());
        }
        if scoped_type_actual {
            nominal_type_actuals(
                &decl.generics,
                args,
                self.source_definition,
                self.budget,
                ty.span,
            )?;
            if self.budget.has_enum_profiles()
                && let VariantFields::Record(fields) =
                    &record_enums::variant(decl, variant, ty.span, self.budget)?.fields
            {
                return fields
                    .iter()
                    .map(|field| {
                        nominal_field_type(
                            &decl.generics,
                            args,
                            &field.ty,
                            self.source_definition,
                            self.budget,
                        )
                    })
                    .collect();
            }
            return syntax_enum::fields(decl, variant, ty.span)?
                .iter()
                .map(|field| {
                    if field.manages {
                        return Err(unsupported(field.span, "owned generic enum payload"));
                    }
                    nominal_field_type(
                        &decl.generics,
                        args,
                        &field.ty,
                        self.source_definition,
                        self.budget,
                    )
                })
                .collect();
        }
        if self.budget.has_enum_profiles()
            && let VariantFields::Record(fields) =
                &record_enums::variant(decl, variant, ty.span, self.budget)?.fields
        {
            return fields
                .iter()
                .map(|field| syntax_enum::substituted(&field.ty, &found, self.budget))
                .collect();
        }
        syntax_enum::fields(decl, variant, ty.span)?
            .iter()
            .enumerate()
            .map(|(index, t)| {
                if t.manages {
                    if !decl.generics.is_empty() {
                        return Err(unsupported(t.span, "owned generic enum payload"));
                    }
                    if let Some(managed) = &mut managed {
                        self.budget.node(t.span, 0)?;
                        managed.push(index);
                    }
                }
                syntax_enum::substituted(t, &found, self.budget)
            })
            .collect()
    }
    fn pattern_has_ref(&mut self, p: &Pattern, depth: usize) -> R<bool> {
        self.budget.node(p.span, depth)?;
        match &p.kind {
            PatternKind::Reference { .. } => Ok(true),
            PatternKind::ErrorArm { pattern, .. } => self.pattern_has_ref(pattern, depth + 1),
            PatternKind::Tuple(fields)
            | PatternKind::Constructor {
                arguments: fields, ..
            } => {
                for p in fields {
                    if self.pattern_has_ref(p, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            PatternKind::RecordConstructor { fields, .. } => {
                for f in fields {
                    if self.pattern_has_ref(&f.pattern, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    fn pattern_value(&mut self, value: &Expr, state: &mut State, keep_place: bool) -> R<TypeRef> {
        let previous = self.pattern_read;
        if keep_place {
            let mut base = value;
            while let ExprKind::Group(inner) | ExprKind::Ascribed { value: inner, .. } = &base.kind
            {
                self.budget.node(base.span, 0)?;
                base = inner;
            }
            if matches!(
                base.kind,
                ExprKind::Unary {
                    op: UnaryOp::Dereference,
                    ..
                } | ExprKind::Member { .. }
                    | ExprKind::Index { .. }
            ) {
                self.pattern_read = Some(base as *const Expr as usize);
            }
        }
        let result = self.expr(value, state, None, false);
        self.pattern_read = previous;
        result
    }
    fn prepare_pattern_place(&mut self, p: &Pattern, value: &Expr, state: &State) -> R<()> {
        debug_assert!(self.pattern_place.is_none());
        self.pattern_route.clear();
        if self.budget.has_reference_patterns() && self.pattern_has_ref(p, 0)? {
            self.pattern_place = Some(self.alias_pattern_prepare(value, state)?);
        }
        Ok(())
    }
    fn pattern_child(
        &mut self,
        p: &Pattern,
        ty: &TypeRef,
        state: &mut State,
        seen: &mut BTreeSet<String>,
        step: zil_ir::PlaceProjection,
    ) -> R<Pat> {
        let positioned = self.pattern_place.is_some();
        if positioned {
            self.budget.node(p.span, self.pattern_route.len() + 1)?;
            for _ in 0..self.pattern_route.len() + 1 {
                self.budget.node(p.span, 0)?;
            }
            self.pattern_route.push(step);
        }
        let result = self.pattern(p, ty, state, seen);
        if positioned {
            self.pattern_route.pop();
        }
        result
    }
    fn pattern_variant_slot(&mut self, decl: &EnumDecl, name: &str, span: Span) -> R<u32> {
        for (i, variant) in decl.variants.iter().enumerate() {
            self.budget.node(span, 0)?;
            self.budget.text(&variant.name, span)?;
            self.budget.text(name, span)?;
            if variant.name == name {
                return Ok(i as u32);
            }
        }
        Err(error(
            span,
            "pattern variant has no actual declaration position",
        ))
    }
    fn pattern(
        &mut self,
        p: &Pattern,
        ty: &TypeRef,
        state: &mut State,
        seen: &mut BTreeSet<String>,
    ) -> R<Pat> {
        self.tick(p.span)?;
        Ok(match &p.kind {
            PatternKind::Name(name) => {
                if !seen.insert(name.clone()) {
                    return Err(error(p.span, "duplicate generic pattern binding"));
                }
                self.budget.ty(ty)?;
                self.bind_local(state, name.clone(), ty.clone())?;
                Pat::Any
            }
            PatternKind::Wildcard => Pat::Any,
            PatternKind::Reference { mutable, pattern } => {
                self.budget.ty(ty)?;
                let address = TypeRef {
                    kind: TypeKind::Reference {
                        lifetime: None,
                        mutable: *mutable,
                        inner: Box::new(ty.clone()),
                    },
                    span: p.span,
                };
                if !matches!(pattern.kind, PatternKind::Name(_) | PatternKind::Wildcard) {
                    return Err(error(p.span, "reference binding needs a name or discard"));
                }
                let local = self.next_local;
                let checked = self.pattern(pattern, &address, state, seen)?;
                if let Some(place) = self.pattern_place.take() {
                    let route = std::mem::take(&mut self.pattern_route);
                    let binding = matches!(pattern.kind, PatternKind::Name(_)).then_some(local);
                    let result =
                        self.alias_pattern_borrow(&place, &route, binding, *mutable, state, p.span);
                    self.pattern_route = route;
                    self.pattern_place = Some(place);
                    result?;
                }
                checked
            }
            PatternKind::ErrorArm { is_error, pattern } => {
                let TypeKind::Error {
                    error: error_ty,
                    result,
                } = &group(ty).kind
                else {
                    return Err(error(p.span, "error arm requires an ErrorUnion"));
                };
                Pat::Ctor(
                    Ctor::Error(*is_error),
                    vec![self.pattern_child(
                        pattern,
                        if *is_error { error_ty } else { result },
                        state,
                        seen,
                        zil_ir::PlaceProjection::ErrorArm(*is_error),
                    )?],
                )
            }
            PatternKind::Literal(Literal::Bool(v))
                if boolean(ty) && !dependent(ty, &self.variables) =>
            {
                Pat::Ctor(Ctor::Bool(*v), vec![])
            }
            PatternKind::Literal(Literal::Integer(v))
                if integer(ty) && !dependent(ty, &self.variables) =>
            {
                Pat::Ctor(
                    Ctor::Integer(integer_value(v, ty, p.span, self.budget)?),
                    vec![],
                )
            }
            PatternKind::Tuple(ps)
                if ps.is_empty()
                    && matches!(&group(ty).kind, TypeKind::Named{path,args} if path.as_slice()==["void"] && args.is_empty()) =>
            {
                Pat::Ctor(Ctor::Tuple, vec![])
            }
            PatternKind::Tuple(ps) => {
                let TypeKind::Tuple(ts) = &group(ty).kind else {
                    return Err(error(
                        p.span,
                        "tuple pattern requires tuple type before instantiation",
                    ));
                };
                if ps.len() != ts.len() {
                    return Err(error(p.span, "generic tuple pattern arity mismatch"));
                }
                Pat::Ctor(
                    Ctor::Tuple,
                    ps.iter()
                        .zip(ts)
                        .enumerate()
                        .map(|(index, (p, t))| {
                            self.pattern_child(
                                p,
                                t,
                                state,
                                seen,
                                zil_ir::PlaceProjection::Field(index as u32),
                            )
                        })
                        .collect::<R<_>>()?,
                )
            }
            PatternKind::RecordConstructor {
                path,
                fields: supplied,
            } => {
                let alias = path.first().and_then(|name| self.pattern_types.get(name));
                if let Some(alias) = alias {
                    same(ty, alias, p.span, self.budget)?;
                }
                let TypeKind::Named { path: base, args } = &group(ty).kind else {
                    return Err(error(p.span, "record pattern requires declared enum type"));
                };
                if path.len() != base.len() + 1
                    || (alias.is_none() && path[..path.len() - 1] != base[..])
                {
                    return Err(error(
                        p.span,
                        "generic record pattern nominal type mismatch",
                    ));
                }
                let declaration = self
                    .enums
                    .get(&base.join("."))
                    .copied()
                    .ok_or_else(|| error(p.span, "unknown enum in record pattern"))?;
                let variant = path.last().expect("qualified variant");
                let declared = record_enums::fields(declaration, variant, p.span, self.budget)?;
                let order = record_enums::order(
                    declared,
                    supplied.iter().map(|f| (f.name.as_str(), f.span)),
                    p.span,
                    self.budget,
                )?;
                if declaration.generics.len() != args.len() {
                    return Err(error(p.span, "record enum pattern type arity mismatch"));
                }
                let mixed = declaration
                    .generics
                    .iter()
                    .any(|p| !matches!(p.kind, GenericParamKind::Type(_)));
                let found = nominal_type_actuals(
                    &declaration.generics,
                    args,
                    self.source_definition,
                    self.budget,
                    p.span,
                )?;
                self.budget.text(variant, p.span)?;
                for _ in declared {
                    self.budget.node(p.span, 0)?;
                }
                let variant_slot = if self.pattern_place.is_some() {
                    self.pattern_variant_slot(declaration, variant, p.span)?
                } else {
                    0
                };
                let mut patterns = vec![Pat::Any; declared.len()];
                for (supplied, index) in supplied.iter().zip(order) {
                    let field_type = if mixed {
                        nominal_field_type(
                            &declaration.generics,
                            args,
                            &declared[index].ty,
                            self.source_definition,
                            self.budget,
                        )?
                    } else {
                        syntax_enum::substituted(&declared[index].ty, &found, self.budget)?
                    };
                    patterns[index] = self.pattern_child(
                        &supplied.pattern,
                        &field_type,
                        state,
                        seen,
                        zil_ir::PlaceProjection::Variant(zil_ir::VariantProjection {
                            variant: variant_slot,
                            index: index as u32,
                        }),
                    )?;
                }
                Pat::Ctor(Ctor::Variant(variant.clone()), patterns)
            }
            PatternKind::Constructor { path, arguments } => {
                if let TypeKind::Error {
                    error: error_ty,
                    result,
                } = &group(ty).kind
                {
                    if path.len() != 1 || arguments.len() != 1 {
                        return Err(error(
                            p.span,
                            "ErrorUnion member pattern requires one type and one payload",
                        ));
                    }
                    if error_members_may_equal(error_ty, result, &self.variables, self.budget, 0)? {
                        return Err(error(
                            p.span,
                            "ErrorUnion member identity may coincide; use .ok or .err",
                        ));
                    }
                    let member = if let Some(alias) = self.pattern_types.get(&path[0]) {
                        self.budget.ty(alias)?;
                        alias.clone()
                    } else {
                        named(&path[0], p.span)
                    };
                    let key = type_key(&member, self.budget)?;
                    let is_error = if key == type_key(error_ty, self.budget)? {
                        true
                    } else if key == type_key(result, self.budget)? {
                        false
                    } else {
                        return Err(error(
                            p.span,
                            "pattern type is not an exact ErrorUnion member",
                        ));
                    };
                    return Ok(Pat::Ctor(
                        Ctor::Error(is_error),
                        vec![self.pattern_child(
                            &arguments[0],
                            if is_error { error_ty } else { result },
                            state,
                            seen,
                            zil_ir::PlaceProjection::ErrorArm(is_error),
                        )?],
                    ));
                }
                if matches!(&group(ty).kind, TypeKind::Union(_)) {
                    if path.len() != 1 || arguments.len() != 1 {
                        return Err(error(
                            p.span,
                            "anonymous member pattern requires one type and one payload",
                        ));
                    }
                    let member = if let Some(alias) = self.pattern_types.get(&path[0]) {
                        self.budget.ty(alias)?;
                        alias.clone()
                    } else {
                        named(&path[0], p.span)
                    };
                    let key = type_key(&member, self.budget)?;
                    let members = self.sum_members(ty)?;
                    let mut found = None;
                    for member in &members {
                        if type_key(member, self.budget)? == key {
                            if found.is_some() {
                                return Err(error(p.span, "anonymous member pattern is ambiguous"));
                            }
                            self.budget.ty(member)?;
                            found = Some(member.clone());
                        }
                    }
                    let member = found.ok_or_else(|| {
                        error(p.span, "pattern type is not an exact anonymous member")
                    })?;
                    let original = self.pattern_place.take();
                    if let Some(place) = &original {
                        self.pattern_place = Some(self.alias_pattern_coarse(place, p.span)?);
                    }
                    let child = self.pattern(&arguments[0], &member, state, seen);
                    self.pattern_place = original;
                    return Ok(Pat::Ctor(Ctor::Sum(key), vec![child?]));
                }
                let alias = path.first().and_then(|name| self.pattern_types.get(name));
                if let Some(alias) = alias {
                    same(ty, alias, p.span, self.budget)?;
                }
                let TypeKind::Named { path: base, .. } = &group(ty).kind else {
                    return Err(error(p.span, "enum pattern requires declared enum type"));
                };
                if path.len() != base.len() + 1
                    || (alias.is_none() && path[..path.len() - 1] != base[..])
                {
                    return Err(error(p.span, "generic pattern nominal type mismatch"));
                }
                let variant = path.last().expect("variant");
                if let Some(declaration) = self.enums.get(&base.join("."))
                    && self.budget.has_enum_profiles()
                    && matches!(
                        record_enums::variant(declaration, variant, p.span, self.budget)?.fields,
                        VariantFields::Record(_)
                    )
                {
                    return Err(error(
                        p.span,
                        "record enum variant requires a named pattern",
                    ));
                }
                let variant_slot = if self.pattern_place.is_some() {
                    let decl = self.enums[&base.join(".")];
                    self.pattern_variant_slot(decl, variant, p.span)?
                } else {
                    0
                };
                let mut managed = Vec::new();
                let fields = self.enum_fields(ty, variant, Some(&mut managed))?;
                if fields.len() != arguments.len() {
                    return Err(error(p.span, "generic enum pattern payload arity mismatch"));
                }
                self.budget.text(variant, p.span)?;
                Pat::Ctor(
                    Ctor::Variant(variant.clone()),
                    arguments
                        .iter()
                        .zip(&fields)
                        .enumerate()
                        .map(|(index, (p, t))| {
                            let pattern = self.pattern_child(
                                p,
                                t,
                                state,
                                seen,
                                zil_ir::PlaceProjection::Variant(zil_ir::VariantProjection {
                                    variant: variant_slot,
                                    index: index as u32,
                                }),
                            )?;
                            if !managed.is_empty()
                                && managed.binary_search(&index).is_ok()
                                && let PatternKind::Name(name) = &p.kind
                            {
                                self.budget.node(p.span, 0)?;
                                for _ in
                                    0..(state.locals.len().saturating_add(1).ilog2() as usize + 2)
                                {
                                    self.budget.text(name, p.span)?;
                                }
                                // Resolve this real declaration field in its own scope,
                                // not against the helper's possibly same-spelled binders.
                                self.budget.ty(t)?;
                                let declaration_resource = self.declared_resource(t);
                                let roots =
                                    if declaration_resource {
                                        Some(self.resource_description_root(
                                            state.locals[name].id,
                                            p.span,
                                        )?)
                                    } else {
                                        None
                                    };
                                let local =
                                    state.locals.get_mut(name).expect("actual pattern binding");
                                local.owned_resource = true;
                                local.declaration_resource = declaration_resource;
                                local.description_roots = roots;
                                local.description_reference_depth =
                                    declaration_resource.then_some(0);
                                self.enum_resource_flow = true;
                                if stores::element(&local.ty).is_some() {
                                    // Extraction is incoming ownership, never a create producer.
                                    self.budget.node(p.span, 0)?;
                                    state.store_owners.insert(local.id);
                                    state
                                        .store_origins
                                        .insert(local.id, BTreeSet::from([local.id]));
                                }
                            }
                            Ok(pattern)
                        })
                        .collect::<R<_>>()?,
                )
            }
            _ => {
                return Err(unsupported(
                    p.span,
                    "parametric pattern requires a known type constructor",
                ));
            }
        })
    }
    pub(super) fn enum_if_let(
        &mut self,
        pattern: &Pattern,
        value: &Expr,
        then_body: &Block,
        else_body: Option<&ElseBranch>,
        state: &mut State,
        span: Span,
    ) -> R<BlockFlow> {
        let has_ref = self.budget.has_reference_patterns() && self.pattern_has_ref(pattern, 0)?;
        let ty = self.pattern_value(value, state, has_ref)?;
        let mut yes = self.clone_state(state)?;
        let first = self.next_local;
        self.prepare_pattern_place(pattern, value, &yes)?;
        let pattern = self.pattern(pattern, &ty, &mut yes, &mut BTreeSet::new())?;
        let irrefutable = !self.useful(
            &[vec![pattern]],
            &[Pat::Any],
            std::slice::from_ref(&ty),
            0,
            span,
        )?;
        let place = self.pattern_place.take();
        let a = self.block(then_body, &mut yes)?;
        if let Some(place) = place {
            self.alias_pattern_end(&place, &mut yes, span)?;
        }
        self.resource_description_scope_exit(&mut yes, first, span)?;
        let mut no = self.clone_state(state)?;
        let b = match else_body {
            Some(ElseBranch::Block(block)) => self.block(block, &mut no)?,
            Some(ElseBranch::If(statement)) => {
                self.budget.statement_snapshot(statement)?;
                self.block(
                    &Block {
                        span: statement.span,
                        statements: vec![*statement.clone()],
                    },
                    &mut no,
                )?
            }
            None => BlockFlow::NEXT,
        };
        match (a.fallthrough, !irrefutable && b.fallthrough) {
            (true, true) => self.join_states(state, &[&yes, &no], span)?,
            (true, false) => self.join_states(state, &[&yes], span)?,
            (false, true) => self.join_states(state, &[&no], span)?,
            _ => {}
        }
        Ok(BlockFlow {
            fallthrough: a.fallthrough || !irrefutable && b.fallthrough,
            returns: a.returns || !irrefutable && b.returns,
        })
    }
    pub(super) fn enum_while_let(
        &mut self,
        pattern: &Pattern,
        value: &Expr,
        body: &Block,
        state: &mut State,
        span: Span,
    ) -> R<BlockFlow> {
        let aliases = self.alias_mark_loop();
        let entry = self.clone_loop_state(state, value.span)?;
        let has_ref = self.budget.has_reference_patterns() && self.pattern_has_ref(pattern, 0)?;
        let ty = self.pattern_value(value, state, has_ref)?;
        self.loop_invariant(&entry, state, value.span)?;
        let mut iteration = self.clone_loop_state(state, body.span)?;
        self.prepare_pattern_place(pattern, value, &iteration)?;
        self.pattern(pattern, &ty, &mut iteration, &mut BTreeSet::new())?;
        let place = self.pattern_place.take();
        self.loop_body_pattern(body, state, iteration, span, aliases, place.as_ref())
    }
    pub(super) fn enum_match(
        &mut self,
        value: &Expr,
        arms: &[MatchArm],
        state: &mut State,
    ) -> R<BlockFlow> {
        let mut has_ref = false;
        if self.budget.has_reference_patterns() {
            for arm in arms {
                if self.pattern_has_ref(&arm.pattern, 0)? {
                    has_ref = true;
                    break;
                }
            }
        }
        let ty = self.pattern_value(value, state, has_ref)?;
        let prepared = if has_ref {
            Some(self.alias_pattern_prepare(value, state)?)
        } else {
            None
        };
        let mut rows = Vec::new();
        let mut continuing = Vec::new();
        let mut flow = BlockFlow {
            fallthrough: false,
            returns: false,
        };
        for arm in arms {
            let mut branch = self.clone_state(state)?;
            let first_resource_local = self.next_local;
            self.pattern_route.clear();
            self.pattern_place = prepared
                .as_ref()
                .map(|place| self.alias_pattern_clone(place, arm.span))
                .transpose()?;
            let p = self.pattern(&arm.pattern, &ty, &mut branch, &mut BTreeSet::new())?;
            if !self.useful(
                &rows,
                std::slice::from_ref(&p),
                std::slice::from_ref(&ty),
                0,
                arm.span,
            )? {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-MATCH-UNREACHABLE",
                    Some(arm.span),
                    "generic match arm is already covered",
                ));
            }
            rows.push(vec![p]);
            let place = self.pattern_place.take();
            let branch_flow = match &arm.body {
                ArrowBody::Block(b) => self.block(b, &mut branch)?,
                ArrowBody::Expression(e) => {
                    self.expr(e, &mut branch, None, false)?;
                    BlockFlow::NEXT
                }
            };
            if let Some(place) = place {
                self.alias_pattern_end(&place, &mut branch, arm.span)?;
            }
            self.resource_description_scope_exit(&mut branch, first_resource_local, arm.span)?;
            flow.returns |= branch_flow.returns;
            if branch_flow.fallthrough {
                flow.fallthrough = true;
                continuing.push(branch);
            }
        }
        if self.useful(&rows, &[Pat::Any], std::slice::from_ref(&ty), 0, value.span)? {
            return Err(Diagnostic::error(
                Phase::Expand,
                "E-MATCH-NONEXHAUSTIVE",
                Some(value.span),
                "generic match does not cover every possible value",
            ));
        }
        if flow.fallthrough {
            self.join_states(state, &continuing.iter().collect::<Vec<_>>(), value.span)?;
        }
        Ok(flow)
    }
    fn charge_pattern(&mut self, p: &Pat, depth: usize, span: Span) -> R<()> {
        self.budget.node(span, depth)?;
        if let Pat::Ctor(constructor, fields) = p {
            if let Ctor::Variant(name) | Ctor::Sum(name) = constructor {
                self.budget.text(name, span)?;
            }
            for f in fields {
                self.charge_pattern(f, depth + 1, span)?;
            }
        }
        Ok(())
    }
    fn sum_members(&mut self, ty: &TypeRef) -> R<Vec<TypeRef>> {
        self.budget.ty(ty)?;
        let mut todo = vec![ty];
        let mut members = Vec::new();
        while let Some(member) = todo.pop() {
            if let TypeKind::Union(parts) = &group(member).kind {
                todo.extend(parts);
            } else {
                self.budget.ty(member)?;
                members.push(member.clone());
            }
        }
        Ok(members)
    }
    fn constructors(&mut self, ty: &TypeRef) -> R<Vec<Ctor>> {
        Ok(match &group(ty).kind {
            TypeKind::Error { .. } => vec![Ctor::Error(false), Ctor::Error(true)],
            TypeKind::Union(_) => {
                let members = self.sum_members(ty)?;
                let mut constructors = Vec::new();
                for member in members {
                    constructors.push(Ctor::Sum(type_key(&member, self.budget)?));
                }
                constructors
            }
            TypeKind::Tuple(_) => vec![Ctor::Tuple],
            TypeKind::Named { path, args } if path.as_slice() == ["void"] && args.is_empty() => {
                vec![Ctor::Tuple]
            }
            TypeKind::Named { path, .. } if path.as_slice() == ["bool"] => {
                vec![Ctor::Bool(false), Ctor::Bool(true)]
            }
            TypeKind::Named { path, .. } if self.enums.contains_key(&path.join(".")) => {
                let decl = self.enums[&path.join(".")];
                let mut variants = Vec::new();
                for v in &decl.variants {
                    self.budget.node(v.span, 0)?;
                    self.budget.text(&v.name, v.span)?;
                    variants.push(Ctor::Variant(v.name.clone()));
                }
                variants
            }
            _ => vec![Ctor::Opaque],
        })
    }
    fn constructor_fields(&mut self, c: &Ctor, ty: &TypeRef) -> R<Vec<TypeRef>> {
        match (c, &group(ty).kind) {
            (Ctor::Error(is_error), TypeKind::Error { error, result }) => {
                let payload = if *is_error { error } else { result };
                self.budget.ty(payload)?;
                Ok(vec![payload.as_ref().clone()])
            }
            (Ctor::Tuple, TypeKind::Tuple(fields)) => {
                for f in fields {
                    self.budget.ty(f)?;
                }
                Ok(fields.clone())
            }
            (Ctor::Sum(key), TypeKind::Union(_)) => {
                for member in self.sum_members(ty)? {
                    if &type_key(&member, self.budget)? == key {
                        return Ok(vec![member]);
                    }
                }
                Err(error(ty.span, "anonymous coverage constructor is absent"))
            }
            (Ctor::Variant(v), _) => self.enum_fields(ty, v, None),
            _ => Ok(vec![]),
        }
    }
    fn specialize(
        &mut self,
        rows: &[Vec<Pat>],
        c: &Ctor,
        arity: usize,
        span: Span,
    ) -> R<Vec<Vec<Pat>>> {
        for row in rows {
            match &row[0] {
                Pat::Any => {
                    for _ in 0..arity {
                        self.budget.node(span, 0)?;
                    }
                }
                Pat::Ctor(actual, fields) if actual == c => {
                    for p in fields {
                        self.charge_pattern(p, 0, span)?;
                    }
                }
                _ => continue,
            }
            for p in &row[1..] {
                self.charge_pattern(p, 0, span)?;
            }
        }
        Ok(specialize(rows, c, arity))
    }
    fn useful(
        &mut self,
        rows: &[Vec<Pat>],
        query: &[Pat],
        types: &[TypeRef],
        depth: usize,
        span: Span,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        for p in rows.iter().flatten().chain(query) {
            self.charge_pattern(p, 0, span)?;
        }
        for ty in types {
            self.budget.ty(ty)?;
        }
        if query.is_empty() {
            return Ok(rows.is_empty());
        }
        if rows.iter().any(|r| r.iter().all(|p| matches!(p, Pat::Any))) {
            return Ok(false);
        }
        if let Pat::Ctor(c, fields) = &query[0] {
            let mut next = fields.clone();
            next.extend_from_slice(&query[1..]);
            let mut ts = self.constructor_fields(c, &types[0])?;
            ts.extend_from_slice(&types[1..]);
            let rows = self.specialize(rows, c, fields.len(), span)?;
            return self.useful(&rows, &next, &ts, depth + 1, span);
        }
        let ctors = self.constructors(&types[0])?;
        for c in ctors {
            if c == Ctor::Opaque {
                let defaults = rows
                    .iter()
                    .filter(|r| matches!(r[0], Pat::Any))
                    .map(|r| r[1..].to_vec())
                    .collect::<Vec<_>>();
                return self.useful(&defaults, &query[1..], &types[1..], depth + 1, span);
            }
            let mut ts = self.constructor_fields(&c, &types[0])?;
            let n = ts.len();
            ts.extend_from_slice(&types[1..]);
            let mut next = vec![Pat::Any; n];
            next.extend_from_slice(&query[1..]);
            let specialized = self.specialize(rows, &c, n, span)?;
            if self.useful(&specialized, &next, &ts, depth + 1, span)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn specialize(rows: &[Vec<Pat>], c: &Ctor, arity: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|row| {
            let fields = match &row[0] {
                Pat::Any => vec![Pat::Any; arity],
                Pat::Ctor(actual, fields) if actual == c => fields.clone(),
                _ => return None,
            };
            Some(fields.into_iter().chain(row[1..].iter().cloned()).collect())
        })
        .collect()
}
fn integer_value(text: &str, ty: &TypeRef, span: Span, budget: &mut Budget<'_>) -> R<i128> {
    let (actual, value) =
        checked_integer(text, Some(ty), false, span, budget.target_pointer_width)?;
    if type_key_bounded(&actual, budget)? != type_key_bounded(ty, budget)? {
        return Err(error(
            span,
            "generic integer pattern suffix disagrees with its type",
        ));
    }
    Ok(value)
}

// Substitution may identify a bare binder with any type. Rigid different
// constructors/nominal families are disjoint independently of their fields.
fn error_members_may_equal(
    a: &TypeRef,
    b: &TypeRef,
    vars: &BTreeSet<String>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<bool> {
    // Reserve this entire borrowed traversal before stripping wrappers or
    // scanning associated-name/argument structure. Non-Type arguments count.
    budget.ty(a)?;
    budget.ty(b)?;
    budget.node(a.span, depth)?;
    let (a, b) = (group(a), group(b));
    fn unknown(t: &TypeRef, vars: &BTreeSet<String>, budget: &mut Budget<'_>) -> R<bool> {
        let TypeKind::Named { path, .. } = &t.kind else {
            return Ok(false);
        };
        let Some(name) = path.first() else {
            return Ok(false);
        };
        let factor = (vars.len().checked_ilog2().unwrap_or(0) as usize + 2) * 8;
        for _ in 0..factor {
            budget.text(name, t.span)?;
        }
        Ok(vars.contains(name))
    }
    let unit = |t: &TypeRef| {
        matches!(&t.kind, TypeKind::Tuple(fields) if fields.is_empty())
            || matches!(&t.kind, TypeKind::Named { path, args } if path.as_slice()==["void"] && args.is_empty())
    };
    if unit(a) && unit(b) {
        return Ok(true);
    }
    if unknown(a, vars, budget)?
        || unknown(b, vars, budget)?
        || trait_bounds::associated::parts(a).is_some()
        || trait_bounds::associated::parts(b).is_some()
    {
        return Ok(true);
    }
    match (&a.kind, &b.kind) {
        (TypeKind::Named { path: ap, args: aa }, TypeKind::Named { path: bp, args: ba }) => {
            for name in ap.iter().chain(bp) {
                budget.text(name, a.span)?;
            }
            if ap != bp || aa.len() != ba.len() {
                return Ok(false);
            }
            for (left, right) in aa.iter().zip(ba) {
                budget.node(a.span, depth + 1)?;
                if let (GenericArg::Type(a), GenericArg::Type(b)) = (left, right)
                    && !error_members_may_equal(a, b, vars, budget, depth + 1)?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                if !error_members_may_equal(a, b, vars, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (
            TypeKind::Reference {
                mutable: am,
                inner: a,
                ..
            },
            TypeKind::Reference {
                mutable: bm,
                inner: b,
                ..
            },
        ) => Ok(am == bm && error_members_may_equal(a, b, vars, budget, depth + 1)?),
        (TypeKind::Optional(a), TypeKind::Optional(b)) => {
            error_members_may_equal(a, b, vars, budget, depth + 1)
        }
        (
            TypeKind::Error {
                error: ae,
                result: ar,
            },
            TypeKind::Error {
                error: be,
                result: br,
            },
        ) => Ok(error_members_may_equal(ae, be, vars, budget, depth + 1)?
            && error_members_may_equal(ar, br, vars, budget, depth + 1)?),
        (TypeKind::Array { element: a, .. }, TypeKind::Array { element: b, .. }) => {
            error_members_may_equal(a, b, vars, budget, depth + 1)
        }
        // These forms need their complete normalization/equations before a
        // disjointness proof; absence of proof never grants unique selection.
        (
            TypeKind::MetaEval(_)
            | TypeKind::MetaSplice(_)
            | TypeKind::MetaBlock(_)
            | TypeKind::Impl(_)
            | TypeKind::Union(_),
            _,
        )
        | (
            _,
            TypeKind::MetaEval(_)
            | TypeKind::MetaSplice(_)
            | TypeKind::MetaBlock(_)
            | TypeKind::Impl(_)
            | TypeKind::Union(_),
        ) => Ok(true),
        (a, b) => Ok(std::mem::discriminant(a) == std::mem::discriminant(b)),
    }
}

#[cfg(test)]
mod terminal340_error_member_budget {
    use super::*;
    #[test]
    fn wide_const_arguments_long_binder_prefix_and_groups_are_prepaid() {
        let span = Span::default();
        let prefix = format!("binding_{}", "x".repeat(256));
        let vars = (0..64)
            .map(|i| format!("{prefix}_{i:03}"))
            .collect::<BTreeSet<_>>();
        let args = (0..96)
            .map(|_| {
                GenericArg::Const(Expr {
                    kind: ExprKind::Literal(Literal::Integer("123456789".into())),
                    span,
                })
            })
            .collect::<Vec<_>>();
        let mut member = TypeRef {
            kind: TypeKind::Named {
                path: vec![format!("{prefix}_family")],
                args,
            },
            span,
        };
        for _ in 0..24 {
            member = TypeRef {
                kind: TypeKind::Group(Box::new(member)),
                span,
            };
        }
        let run = |work| {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            let mut budget = Budget::new(&limits);
            let result = error_members_may_equal(&member, &member, &vars, &mut budget, 0);
            (result, budget.remaining_limits().max_analysis_steps)
        };
        let (result, left) = run(Limits::default().max_analysis_steps);
        assert!(result.unwrap());
        let n = Limits::default().max_analysis_steps - left;
        assert!(run(n).0.unwrap());
        assert!(run(n - 1).0.unwrap_err().category == zil_base::ResultCategory::ResourceLimit);
        assert!(run(n).0.unwrap());
        let (short, left) = run(3);
        assert_eq!(
            short.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(left, 0);
        println!("340 ErrorUnion member-disjointness wide/grouped/prefix N={n}");
    }
}

#[cfg(test)]
mod terminal340_pattern_origin_tests {
    use zil_base::{FileId, ModuleId, SourceUnit};
    fn check(text: &str) -> zil_base::CompileResult<zil_ir::CoreModule> {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "pattern-origin.zil", text);
        let syntax = zil_syntax::parse(&source).into_result()?;
        crate::expand(&source, &syntax)
    }
    #[test]
    fn unused_payload_reference_keeps_parent_and_last_use_in_all_condition_forms() {
        for (index, body) in [
            "match value{.ok(ref n)=>{value=x;let observed=*n;return 0},.err(_)=>{return 0}}",
            "if let .ok(ref n)=value{value=x;let observed=*n;return 0}return 0",
            "while let .ok(ref n)=value{value=x;let observed=*n;return 0}return 0",
        ]
        .iter()
        .enumerate()
        {
            let bad = format!(
                "enum F{{Bad}}fn unused<T:Copy>(x:F!T)->i32{{var value=x;{body}}}fn result()->i32{{return 42}}"
            );
            let good = bad.replace("value=x;let observed=*n;", "let observed=*n;value=x;");
            let rejected = check(&bad);
            let admitted = check(&good);
            assert!(admitted.is_ok(), "case{index} fresh: {admitted:?}");
            assert!(rejected.is_err(), "case{index} stale accepted");
        }
    }
    #[test]
    fn copy_snapshots_are_independent_and_fresh_parent_loans_expire() {
        let fresh = "enum F{Bad}fn unused<T:Copy>(x:F!T,fallback:T)->T{var value=x;match value{.ok(ref n)=>{let observed=*n;value=x;return observed},.err(_)=>{return fallback}}}fn result()->i32{return 42}";
        assert!(check(fresh).is_ok(), "{:?}", check(fresh));
        assert!(
            check(&fresh.replace("let observed=*n;value=x;", "value=x;let observed=*n;")).is_err()
        );
        for body in [
            "if let .ok(ref n)=(make()){saved=n}return *saved",
            "while let .ok(ref n)=(make()){saved=n;break}return *saved",
            "match (make()){.ok(ref n)=>{saved=n},.err(_)=>{}}return *saved",
        ] {
            let bad = format!(
                "enum F{{Bad}}fn make()->F!i32{{return 42}}fn unused<T:Copy>(backup:&i32,marker:T)->i32{{var saved=backup;{body}}}fn result()->i32{{return 42}}"
            );
            let good = bad.replace("saved=n", "saved=n;let observed=*saved;saved=backup");
            assert!(
                check(&good).is_ok(),
                "fresh temporary nearest: {:?}",
                check(&good)
            );
            assert!(check(&bad).is_err(), "fresh temporary escaped: {body}");
        }
    }
    #[test]
    fn nested_pattern_origin_budget_is_exact_and_fresh() {
        let mut ty = "(T,i32)".to_owned();
        let mut pattern = "(ref a,ref mut b)".to_owned();
        for _ in 0..12 {
            ty = format!("({ty},i32)");
            pattern = format!("({pattern},_)");
        }
        let text = format!(
            "enum F{{Bad}}fn unused<T:Copy>(x:F!{ty})->i32{{var value=x;match value{{.ok({pattern})=>{{*b=42;let observed=*a;return *b}},.err(_)=>{{return 0}}}}}}fn result()->i32{{return 42}}"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), "pattern-budget.zil", &text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let run = |work| {
            crate::expand_with_map_limits(
                &source,
                &syntax,
                &zil_base::Limits {
                    max_analysis_steps: work,
                    ..zil_base::Limits::default()
                },
            )
        };
        let (mut low, mut high) = (1, zil_base::Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(_) => high = mid,
                Err(errors) => {
                    assert_eq!(
                        errors[0].category,
                        zil_base::ResultCategory::ResourceLimit,
                        "{errors:?}"
                    );
                    low = mid + 1;
                }
            }
        }
        let first = run(low).unwrap().core;
        assert_eq!(
            run(low - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(first, run(low).unwrap().core);
        println!("340 nested generic pattern-origin Expand N={low}, N-1 rejects, fresh agree");
    }
    #[test]
    fn grouped_fresh_scrutinee_and_saved_call_result_root() {
        let fresh = "enum F{Bad}fn make()->F!i32{return 42}fn unused<T:Copy>(x:T)->i32{match (make()){.ok(ref n)=>{return *n},.err(_)=>{return 0}}}fn result()->i32{return 42}";
        assert!(check(fresh).is_ok(), "{:?}", check(fresh));
        let bad = "enum F{Bad}fn relay<life a,T:Copy>(p:&'a mut(F!T))->&'a mut(F!T){return p}fn unused<T:Copy>(p:&mut(F!T),q:&(F!T),next:F!T)->i32{match *relay(p){.ok(ref n)=>{let unrelated=q;let other=*unrelated;let observed=*n;return 0},.err(ref n)=>{*p=next;let observed=*n;return 0}}}fn result()->i32{return 42}";
        let good = bad.replace("*p=next;let observed=*n;", "let observed=*n;*p=next;");
        assert!(check(&good).is_ok(), "fresh relay: {:?}", check(&good));
        assert!(check(bad).is_err(), "stale relay accepted");
    }
    #[test]
    fn helper_mutation_shared_and_mutable_modes_keep_real_parent() {
        for mode in ["ref", "ref mut"] {
            let bad = format!(
                "enum F{{Bad}}fn replace<T:Copy>(p:&mut(F!T),next:F!T){{*p=next}}fn unused<T:Copy>(p:&mut(F!T),next:F!T)->i32{{match *p{{.ok({mode} n)=>{{replace(p,next);let observed=*n;return 0}},.err(_)=>{{return 0}}}}}}fn result()->i32{{return 42}}"
            );
            let good = bad.replace(
                "replace(p,next);let observed=*n;",
                "let observed=*n;replace(p,next);",
            );
            assert!(check(&good).is_ok(), "{mode} fresh: {:?}", check(&good));
            assert!(check(&bad).is_err(), "{mode} stale accepted");
        }
        let shared = "enum F{Bad}fn unused<T:Copy>(p:&(F!T))->i32{match *p{.ok(ref mut n)=>{let observed=*n;return 0},.err(_)=>{return 0}}}fn result()->i32{return 42}";
        assert!(check(shared).is_err());
        let immutable = "enum F{Bad}fn unused<T:Copy>(x:F!T)->i32{let value=x;match value{.ok(ref mut n)=>{let observed=*n;return 0},.err(_)=>{return 0}}}fn result()->i32{return 42}";
        assert!(check(immutable).is_err());
    }
    #[test]
    fn addressed_parent_mutation_and_disjoint_pattern_siblings() {
        let bad = "enum F{Bad}fn unused<T:Copy>(value:&mut(F!T),replacement:F!T)->i32{match *value{.ok(ref n)=>{*value=replacement;let observed=*n;return 0},.err(_)=>{return 0}}}fn result()->i32{return 42}";
        let good = bad.replace(
            "*value=replacement;let observed=*n;",
            "let observed=*n;*value=replacement;",
        );
        assert!(check(&good).is_ok(), "{:?}", check(&good));
        assert!(check(bad).is_err());
        let siblings = "enum F{Bad}fn unused<T:Copy>(x:F!(T,i32))->i32{var value=x;match value{.ok((ref a,ref mut b))=>{*b=42;let observed=*a;return *b},.err(_)=>{return 0}}}fn result()->i32{return 42}";
        assert!(check(siblings).is_ok(), "{:?}", check(siblings));
    }
}

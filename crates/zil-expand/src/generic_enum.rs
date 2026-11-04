//! Generic enum specialization uses the same bounded AST engine as functions.
use super::*;

pub(super) fn constructor(e: &Expr) -> Option<(&str, &str, Option<&[GenericArg]>)> {
    let ExprKind::Member {
        object,
        member: MemberName::Name(variant),
    } = &e.kind
    else {
        return None;
    };
    match &object.kind {
        ExprKind::Name(name) => Some((name, variant, None)),
        ExprKind::TypeApply { callee, arguments } => match &callee.kind {
            ExprKind::Name(name) => Some((name, variant, Some(arguments))),
            _ => None,
        },
        _ => None,
    }
}
pub(super) fn fields<'a>(
    decl: &'a EnumDecl,
    variant: &str,
    span: Span,
) -> R<&'a [VariantTupleField]> {
    let v = decl
        .variants
        .iter()
        .find(|v| v.name == variant)
        .ok_or_else(|| error(span, "unknown enum variant"))?;
    match &v.fields {
        VariantFields::Unit => Ok(&[]),
        VariantFields::Tuple(fields) => Ok(fields),
        VariantFields::Record(_) => Err(error(span, "record enum variant requires named fields")),
    }
}
pub(super) fn substituted(
    ty: &TypeRef,
    found: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<TypeRef> {
    budget.substituted_type(ty, found)?;
    let mut ty = ty.clone();
    Substitution {
        substitutions: found,
    }
    .ty(&mut ty);
    Ok(ty)
}
pub(super) fn context(
    decl: &EnumDecl,
    expected: Option<&TypeRef>,
    found: &mut BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let expected = grouped(expected);
    if let TypeKind::Error {
        error: failure,
        result,
    } = &expected.kind
    {
        budget.ty(expected)?;
        let nominal = |ty: &TypeRef| {
            matches!(&grouped(ty).kind, TypeKind::Named { path, args }
                if path.as_slice() == [decl.name.as_str()] && args.len() == decl.generics.len())
        };
        return match (nominal(failure), nominal(result)) {
            (true, false) => context(decl, Some(failure), found, budget),
            (false, true) => context(decl, Some(result), found, budget),
            // Never compete between branch hints. Explicit arguments and
            // actual payloads must first establish one complete enum type;
            // check_injection below still validates the entire expected union.
            (true, true) => Ok(()),
            (false, false) => Err(error(
                expected.span,
                "enum constructor matches neither error-union alternative",
            )),
        };
    }
    let TypeKind::Named { path, args } = &expected.kind else {
        return Err(error(
            expected.span,
            "enum constructor context must be the same nominal enum",
        ));
    };
    if path.as_slice() != [decl.name.as_str()] || args.len() != decl.generics.len() {
        return Err(error(
            expected.span,
            "enum constructor has a different nominal type or type arity",
        ));
    }
    let vars = decl.generics.iter().map(|p| p.name.clone()).collect();
    for (p, arg) in decl.generics.iter().zip(args) {
        let GenericArg::Type(ty) = arg else {
            return Err(unsupported(expected.span, "const enum type argument"));
        };
        unify(
            &named(&p.name, p.span),
            ty,
            &vars,
            found,
            expected.span,
            budget,
        )?;
    }
    Ok(())
}
pub(super) fn check_injection(
    actual: &TypeRef,
    expected: Option<&TypeRef>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let TypeKind::Error {
        error: failure,
        result,
    } = &grouped(expected).kind
    else {
        return Ok(());
    };
    let actual = type_key(actual, budget)?;
    match (
        actual == type_key(failure, budget)?,
        actual == type_key(result, budget)?,
    ) {
        (true, false) | (false, true) => Ok(()),
        (true, true) => Err(error(
            expected.span,
            "enum constructor matches both error-union alternatives",
        )),
        (false, false) => Err(error(
            expected.span,
            "enum constructor matches neither error-union alternative",
        )),
    }
}
pub(super) fn application(
    decl: &EnumDecl,
    found: &BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<TypeRef> {
    let mut args = Vec::new();
    for parameter in &decl.generics {
        let ty = found.get(&parameter.name).ok_or_else(|| {
            error(
                span,
                format!(
                    "type parameter {} has no unique constraint; supply context or ::<...>",
                    parameter.name
                ),
            )
        })?;
        budget.ty(ty)?;
        args.push(GenericArg::Type(ty.clone()));
    }
    Ok(TypeRef {
        span,
        kind: TypeKind::Named {
            path: vec![decl.name.clone()],
            args,
        },
    })
}
pub(super) fn validate_declaration(
    decl: &EnumDecl,
    span: Span,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let mut names = BTreeSet::new();
    for variant in &decl.variants {
        if !names.insert(&variant.name) {
            return Err(error(variant.span, "duplicate generic enum variant"));
        }
        if !variant.attributes.is_empty() {
            return Err(unsupported(
                variant.span,
                "unexecuted generic enum variant attribute",
            ));
        }
        if matches!(variant.fields, VariantFields::Record(_)) {
            return Err(unsupported(variant.span, "record enum variants"));
        }
    }
    let variables = decl
        .generics
        .iter()
        .map(|p| p.name.clone())
        .collect::<BTreeSet<_>>();
    let ty = TypeRef {
        span,
        kind: TypeKind::Named {
            path: vec![decl.name.clone()],
            args: decl
                .generics
                .iter()
                .map(|p| GenericArg::Type(named(&p.name, p.span)))
                .collect(),
        },
    };
    for variant in &decl.variants {
        for field in fields(decl, &variant.name, variant.span)? {
            if field.manages {
                return Err(unsupported(field.span, "owned generic enum payload"));
            }
            shape(field, &variables, nominals, budget, 0)?;
        }
    }
    layout(&ty, &variables, nominals, &mut BTreeSet::new(), budget, 0)
}
// Validate every written type argument, including phantom arguments which do
// not contribute fields to the layout walk.
fn shape(
    ty: &TypeRef,
    variables: &BTreeSet<String>,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    match &ty.kind {
        TypeKind::Named { path, args } if path.len() == 1 => {
            let name = &path[0];
            let arity = if variables.contains(name)
                || matches!(
                    name.as_str(),
                    "u8" | "u16"
                        | "u64"
                        | "usize"
                        | "i32"
                        | "i64"
                        | "f32"
                        | "f64"
                        | "bool"
                        | "void"
                        | "c_handle"
                        | "c_wstr"
                ) {
                0
            } else {
                params(
                    nominals
                        .get(name)
                        .ok_or_else(|| error(ty.span, "unknown type in generic enum definition"))?,
                )
                .len()
            };
            if arity != args.len() {
                return Err(error(ty.span, "generic enum field type arity mismatch"));
            }
            for arg in args {
                let GenericArg::Type(ty) = arg else {
                    return Err(unsupported(ty.span, "const generic enum argument"));
                };
                shape(ty, variables, nominals, budget, depth + 1)?;
            }
        }
        TypeKind::Tuple(ts) | TypeKind::Union(ts) => {
            for ty in ts {
                shape(ty, variables, nominals, budget, depth + 1)?;
            }
        }
        TypeKind::Array {
            element,
            length: Some(length),
        } => {
            array_length(length)?;
            shape(element, variables, nominals, budget, depth + 1)?;
        }
        TypeKind::Group(t) => shape(t, variables, nominals, budget, depth + 1)?,
        _ => {
            return Err(unsupported(
                ty.span,
                "generic enum payload requires finite Copy type syntax",
            ));
        }
    }
    Ok(())
}
fn layout(
    ty: &TypeRef,
    variables: &BTreeSet<String>,
    nominals: &BTreeMap<String, &Item>,
    active: &mut BTreeSet<String>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    match &ty.kind {
        TypeKind::Named { path, args } if path.len() == 1 => {
            let name = &path[0];
            if variables.contains(name) && args.is_empty() {
                return Ok(());
            }
            if matches!(
                name.as_str(),
                "u8" | "u16"
                    | "u64"
                    | "usize"
                    | "i32"
                    | "i64"
                    | "f32"
                    | "f64"
                    | "bool"
                    | "void"
                    | "c_handle"
                    | "c_wstr"
            ) && args.is_empty()
            {
                return Ok(());
            }
            let item = nominals
                .get(name)
                .ok_or_else(|| error(ty.span, "unknown type in generic enum definition"))?;
            if params(item).len() != args.len() {
                return Err(error(ty.span, "generic enum field type arity mismatch"));
            }
            let mut substitution = BTreeMap::new();
            for (param, arg) in params(item).iter().zip(args) {
                let GenericArg::Type(ty) = arg else {
                    return Err(unsupported(ty.span, "const enum type argument"));
                };
                budget.ty(ty)?;
                substitution.insert(param.name.clone(), ty.clone());
            }
            let key = type_key(ty, budget)?;
            if !active.insert(key.clone()) {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-INFINITE-LAYOUT",
                    Some(ty.span),
                    "generic by-value enum has infinite layout",
                ));
            }
            let mut fields = Vec::new();
            match &item.kind {
                ItemKind::Enum(e) => {
                    for v in &e.variants {
                        match &v.fields {
                            VariantFields::Tuple(ts) => {
                                if ts.iter().any(|f| f.manages) {
                                    return Err(unsupported(v.span, "owned generic enum payload"));
                                }
                                fields.extend(ts.iter().map(|f| &f.ty));
                            }
                            VariantFields::Unit => {}
                            VariantFields::Record(_) => {
                                return Err(unsupported(v.span, "record enum variants"));
                            }
                        }
                    }
                }
                ItemKind::Aggregate(a) if a.kind == AggregateKind::Struct => {
                    if a.fields.iter().any(|f| f.manages) {
                        return Err(unsupported(ty.span, "managed enum payload field"));
                    }
                    fields.extend(a.fields.iter().map(|f| &f.ty))
                }
                _ => {
                    return Err(unsupported(
                        ty.span,
                        "enum fields require finite Value layout",
                    ));
                }
            }
            for field in fields {
                let field = substituted(field, &substitution, budget)?;
                layout(&field, variables, nominals, active, budget, depth + 1)?;
            }
            active.remove(&key);
        }
        TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
            for field in fields {
                layout(field, variables, nominals, active, budget, depth + 1)?;
            }
        }
        TypeKind::Array {
            element,
            length: Some(length),
        } => {
            array_length(length)?;
            layout(element, variables, nominals, active, budget, depth + 1)?;
        }
        TypeKind::Group(element) => {
            layout(element, variables, nominals, active, budget, depth + 1)?
        }
        _ => {
            return Err(unsupported(
                ty.span,
                "generic enum payload requires a finite Copy value type",
            ));
        }
    }
    Ok(())
}

impl Engine<'_> {
    pub(super) fn enum_expression(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        expected: Option<&TypeRef>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        if matches!(e.kind, ExprKind::Record { .. }) {
            return self.record_enum_expression(e, env, expected, depth);
        }
        let span = e.span;
        let (callee, arguments) = match &mut e.kind {
            ExprKind::Call { callee, arguments } => {
                (callee.as_mut(), Some(arguments.as_mut_slice()))
            }
            _ => (e, None),
        };
        let Some((name, variant, explicit)) = constructor(callee) else {
            return Ok(None);
        };
        if env.contains_key(name) {
            return Ok(None);
        }
        let Some(decl) = self.enums.get(name).copied() else {
            return Ok(None);
        };
        let variant = variant.to_owned();
        let raw_fields = fields(decl, &variant, span)?;
        let mut found = BTreeMap::new();
        let vars = decl
            .generics
            .iter()
            .map(|p| p.name.clone())
            .collect::<BTreeSet<_>>();
        if let Some(explicit) = explicit {
            if explicit.len() != decl.generics.len() {
                return Err(error(span, "enum type argument arity mismatch"));
            }
            for (p, arg) in decl.generics.iter().zip(explicit) {
                let GenericArg::Type(ty) = arg else {
                    return Err(unsupported(span, "const enum type argument"));
                };
                self.budget.ty(ty)?;
                found.insert(p.name.clone(), self.original_type(ty)?);
            }
        }
        let expected = expected.map(|ty| self.original_type(ty)).transpose()?;
        context(decl, expected.as_ref(), &mut found, &mut self.budget)?;
        let supplied = arguments.as_ref().map_or(0, |a| a.len());
        if supplied != raw_fields.len() {
            return Err(error(span, "enum constructor payload arity mismatch"));
        }
        if let Some(arguments) = arguments {
            for (argument, field) in arguments.iter_mut().zip(raw_fields) {
                if argument.spread {
                    return Err(unsupported(argument.span, "enum payload tuple spread"));
                }
                let hint = substituted(field, &found, &mut self.budget)?;
                let unresolved = vars
                    .iter()
                    .any(|v| !found.contains_key(v) && contains(&hint, v));
                let actual = self.expr_expected(
                    &mut argument.value,
                    env,
                    (!unresolved).then_some(&hint),
                    depth + 1,
                )?;
                let actual = match actual {
                    Some(actual) => actual,
                    None => self
                        .value_hint(&argument.value, env, depth + 1, true)?
                        .ok_or_else(|| {
                            error(argument.span, "enum payload type cannot be inferred")
                        })?,
                };
                let actual = self.original_type(&actual)?;
                unify(field, &actual, &vars, &mut found, span, &mut self.budget)?;
            }
        }
        let mut result = application(decl, &found, span, &mut self.budget)?;
        check_injection(&result, expected.as_ref(), &mut self.budget)?;
        self.ty(&mut result, depth + 1)?;
        let TypeKind::Named { path, .. } = &result.kind else {
            unreachable!()
        };
        callee.kind = ExprKind::Member {
            object: Box::new(Expr {
                kind: ExprKind::Name(path[0].clone()),
                span,
            }),
            member: MemberName::Name(variant),
        };
        Ok(Some(result))
    }
    pub(super) fn enum_pattern(
        &mut self,
        pattern: &mut Pattern,
        ty: Option<&TypeRef>,
        env: &mut Env,
        depth: usize,
    ) -> R<()> {
        self.tick(pattern.span, depth)?;
        match &mut pattern.kind {
            PatternKind::Name(name) => {
                if let Some(ty) = ty {
                    self.budget.ty(ty)?;
                }
                env.insert(name.clone(), ty.cloned());
            }
            PatternKind::Reference { mutable, pattern } => {
                let address = ty.map(|inner| TypeRef {
                    kind: TypeKind::Reference {
                        lifetime: None,
                        mutable: *mutable,
                        inner: Box::new(inner.clone()),
                    },
                    span: pattern.span,
                });
                self.enum_pattern(pattern, address.as_ref(), env, depth + 1)?;
            }
            PatternKind::Tuple(patterns) => {
                let types = ty.and_then(|ty| match &grouped(ty).kind {
                    TypeKind::Tuple(fields) => Some(fields.as_slice()),
                    _ => None,
                });
                for (i, p) in patterns.iter_mut().enumerate() {
                    self.enum_pattern(p, types.and_then(|t| t.get(i)), env, depth + 1)?;
                }
            }
            PatternKind::Constructor { path, arguments } => {
                let Some(ty) = ty else {
                    return Err(error(pattern.span, "match enum type cannot be inferred"));
                };
                let written = self.original_type(ty)?;
                if matches!(&grouped(&written).kind, TypeKind::Union(_)) {
                    if path.len() != 1 || arguments.len() != 1 {
                        return Err(error(
                            pattern.span,
                            "anonymous member pattern needs one type name and one payload",
                        ));
                    }
                    let mut member = if let Some(alias) = self.pattern_types.get(&path[0]) {
                        self.budget.ty(alias)?;
                        alias.clone()
                    } else {
                        named(&path[0], pattern.span)
                    };
                    if let Some(substitutions) = self
                        .anonymous_function
                        .as_ref()
                        .and_then(|name| self.anonymous_substitutions.get(name))
                    {
                        self.budget.substituted_type(&member, substitutions)?;
                        Substitution { substitutions }.ty(&mut member);
                    }
                    self.ty(&mut member, depth + 1)?;
                    let marker =
                        format!("$anonymous_pattern_{}", self.products.member_patterns.len());
                    self.budget.text(&marker, pattern.span)?;
                    self.budget.ty(&member)?;
                    self.products
                        .member_patterns
                        .insert(marker.clone(), member.clone());
                    path[0] = marker;
                    self.enum_pattern(&mut arguments[0], Some(&member), env, depth + 1)?;
                    return Ok(());
                }
                let TypeKind::Named { path: root, args } = &grouped(&written).kind else {
                    return Err(error(pattern.span, "constructor pattern requires enum"));
                };
                let Some(decl) = self.enums.get(&root.join(".")).copied() else {
                    return Err(error(
                        pattern.span,
                        "constructor pattern requires a declared enum",
                    ));
                };
                if let Some(alias) = path.first().and_then(|name| self.pattern_types.get(name)) {
                    if type_key(&written, &mut self.budget)? != type_key(alias, &mut self.budget)? {
                        return Err(error(
                            pattern.span,
                            "alias pattern has a different concrete enum type",
                        ));
                    }
                    path[0] = decl.name.clone();
                }
                if path.len() == 2 && path[0] == "Option" && decl.name == options::TEMPLATE {
                    path[0] = options::TEMPLATE.into();
                }
                if path.len() != 2 || path[0] != decl.name {
                    return Err(error(
                        pattern.span,
                        "pattern has different nominal enum type",
                    ));
                }
                let variant = path[1].clone();
                let raw = fields(decl, &variant, pattern.span)?;
                if raw.len() != arguments.len() {
                    return Err(error(pattern.span, "enum pattern payload arity mismatch"));
                }
                let mut found = BTreeMap::new();
                for (p, arg) in decl.generics.iter().zip(args) {
                    let GenericArg::Type(t) = arg else {
                        unreachable!()
                    };
                    self.budget.ty(t)?;
                    found.insert(p.name.clone(), t.clone());
                }
                let TypeKind::Named { path: instance, .. } = &grouped(ty).kind else {
                    unreachable!()
                };
                path[0] = instance[0].clone();
                for (p, t) in arguments.iter_mut().zip(raw) {
                    let mut t = substituted(t, &found, &mut self.budget)?;
                    self.ty(&mut t, depth + 1)?;
                    self.enum_pattern(p, Some(&t), env, depth + 1)?;
                }
            }
            PatternKind::RecordConstructor { path, fields } => {
                self.record_enum_pattern(path, fields, ty, env, pattern.span, depth)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn record_field(&mut self, ty: &TypeRef, member: &str) -> R<Option<TypeRef>> {
        let original = self.original_type(ty)?;
        let TypeKind::Named { path, args } = &grouped(&original).kind else {
            return Ok(None);
        };
        let Some(Item {
            kind: ItemKind::Aggregate(decl),
            ..
        }) = self.nominals.get(&path.join("."))
        else {
            return Ok(None);
        };
        let Some(field) = decl.fields.iter().find(|field| field.name == member) else {
            return Ok(None);
        };
        if decl.generics.len() != args.len() {
            return Err(error(ty.span, "generic record argument arity mismatch"));
        }
        let mut found = BTreeMap::new();
        for (p, arg) in decl.generics.iter().zip(args) {
            let GenericArg::Type(t) = arg else {
                return Err(unsupported(ty.span, "const record argument"));
            };
            self.budget.ty(t)?;
            found.insert(p.name.clone(), t.clone());
        }
        let mut result = substituted(&field.ty, &found, &mut self.budget)?;
        self.ty(&mut result, 0)?;
        Ok(Some(result))
    }
    pub(super) fn call_hints(
        &mut self,
        callee: &Expr,
        env: &Env,
        expected: Option<&TypeRef>,
    ) -> R<Option<CallHints>> {
        let (name, explicit) = match &callee.kind {
            ExprKind::Name(n) => (n, None),
            ExprKind::TypeApply { callee, arguments } => match &callee.kind {
                ExprKind::Name(n) => (n, Some(arguments)),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        if env.contains_key(name) {
            return Ok(None);
        }
        let expected = expected.map(|ty| self.original_type(ty)).transpose()?;
        let Some(f) = self.signatures.get(name) else {
            return Ok(None);
        };
        let variables = type_parameters(f)
            .map(|p| p.name.clone())
            .collect::<BTreeSet<_>>();
        let mut found = BTreeMap::new();
        if let Some(args) = explicit {
            if args.len() != consts::parameters(f).count() {
                return Err(error(callee.span, "type argument arity mismatch"));
            }
            for (p, a) in consts::parameters(f).zip(args) {
                let actual = consts::argument(a, Some(p), &mut self.budget)?;
                let actual = consts::normalize(p, &actual, &mut self.budget)?;
                found.insert(p.name.clone(), actual);
            }
        }
        if let (Some(result), Some(expected)) = (&f.return_type, expected.as_ref())
            && callable::opaque_signature(result).is_none()
            && callable::opaque_signature(expected).is_none()
        {
            if self.traits.has_associated
                && trait_bounds::associated::contains(result, &mut self.budget)?
            {
                let result = substituted(result, &found, &mut self.budget)?;
                if !check::dependent(&result, &variables) {
                    let result = self.traits.normalize_associated(
                        &result,
                        &trait_bounds::Bounds::new(),
                        &BTreeSet::new(),
                        &mut self.budget,
                    )?;
                    unify(
                        &result,
                        expected,
                        &variables,
                        &mut found,
                        callee.span,
                        &mut self.budget,
                    )?;
                }
            } else {
                unify(
                    result,
                    expected,
                    &variables,
                    &mut found,
                    callee.span,
                    &mut self.budget,
                )?;
            }
        }
        let parameters = f
            .params
            .iter()
            .map(|p| {
                if self.traits.has_associated
                    && trait_bounds::associated::contains(&p.ty, &mut self.budget)?
                {
                    let substituted = substituted(&p.ty, &found, &mut self.budget)?;
                    if !check::dependent(&substituted, &variables) {
                        return self.traits.normalize_associated(
                            &substituted,
                            &trait_bounds::Bounds::new(),
                            &BTreeSet::new(),
                            &mut self.budget,
                        );
                    }
                }
                self.budget.ty(&p.ty)?;
                Ok(p.ty.clone())
            })
            .collect::<R<Vec<_>>>()?;
        Ok(Some(CallHints {
            callable: callable::template(f)?.map_or_else(Vec::new, |bounds| {
                bounds.roles.into_iter().map(|role| role.index).collect()
            }),
            parameters,
            variables,
            found,
        }))
    }
}
pub(super) fn contains(ty: &TypeRef, variable: &str) -> bool {
    match &ty.kind {
        TypeKind::Named { path, args } => {
            path.as_slice() == [variable]
                || args
                    .iter()
                    .any(|a| matches!(a,GenericArg::Type(t) if contains(t,variable)))
        }
        TypeKind::Tuple(ts) => ts.iter().any(|t| contains(t, variable)),
        TypeKind::Error { error, result } => {
            contains(error, variable) || contains(result, variable)
        }
        TypeKind::Array { element, .. }
        | TypeKind::Group(element)
        | TypeKind::Optional(element)
        | TypeKind::Reference { inner: element, .. } => contains(element, variable),
        _ => false,
    }
}

pub(super) struct CallHints {
    callable: Vec<usize>,
    parameters: Vec<TypeRef>,
    variables: BTreeSet<String>,
    found: BTreeMap<String, TypeRef>,
}
impl CallHints {
    pub(super) fn independent_input(
        &self,
        index: usize,
        union_pass: bool,
        budget: &mut Budget<'_>,
    ) -> R<bool> {
        match self.parameters.get(index) {
            Some(parameter) => {
                independent_input(parameter, &self.variables, &self.found, union_pass, budget)
            }
            None => Ok(false),
        }
    }
    pub(super) fn unresolved_context(&self, index: usize, budget: &mut Budget<'_>) -> R<bool> {
        let Some(parameter) = self.parameters.get(index) else {
            return Ok(false);
        };
        if !matches!(grouped(parameter).kind, TypeKind::Error { .. }) {
            return Ok(false);
        }
        context_unresolved(parameter, &self.variables, &self.found, budget)
    }
    pub(super) fn independent_constraint(
        &mut self,
        index: usize,
        actual: &TypeRef,
        span: Span,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        if let Some(parameter) = self.parameters.get(index) {
            budget.ty(parameter)?;
            budget.ty(actual)?;
            if matches!(grouped(parameter).kind, TypeKind::Error { .. }) {
                if !matches!(grouped(actual).kind, TypeKind::Error { .. }) {
                    return Ok(());
                }
                unify(
                    parameter,
                    actual,
                    &self.variables,
                    &mut self.found,
                    span,
                    budget,
                )?;
            } else {
                self.constrain(index, actual, span, budget)?;
            }
        }
        Ok(())
    }
    pub(super) fn context_constraints(
        &self,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<BTreeMap<String, TypeRef>> {
        for (name, ty) in &self.found {
            budget.text(name, span)?;
            budget.ty(ty)?;
        }
        Ok(self.found.clone())
    }
    pub(super) fn symbolic_result(
        &self,
        result: &TypeRef,
        bindings: &callable::CallableTemplates,
        budget: &mut Budget<'_>,
    ) -> R<Option<TypeRef>> {
        // The type engine owns concrete opaque identities. Only propagate a
        // lexical generic variable through State<F> here, never instantiate a
        // nominal from the visible Fn signature of an opaque factory.
        let symbolic = |binder: &str| {
            self.found.get(binder).is_some_and(|ty| {
                matches!(&grouped(ty).kind,
            TypeKind::Named { path, args } if path.len()==1 && args.is_empty())
            })
        };
        let ready = if let Some(binder) = bindings.single_generic() {
            symbolic(binder)
        } else if let Some(names) = bindings.dual_names() {
            budget.node(result.span, 0)?;
            symbolic(names[0]) && symbolic(names[1])
        } else {
            false
        };
        if ready {
            Ok(Some(substituted(result, &self.found, budget)?))
        } else {
            Ok(None)
        }
    }

    pub(super) fn hint(&self, index: usize, budget: &mut Budget<'_>) -> R<Option<TypeRef>> {
        let Some(parameter) = self.parameters.get(index) else {
            return Ok(None);
        };
        let hint = substituted(parameter, &self.found, budget)?;
        Ok((!self
            .variables
            .iter()
            .any(|v| !self.found.contains_key(v) && contains(&hint, v)))
        .then_some(hint))
    }
    pub(super) fn constrain(
        &mut self,
        index: usize,
        actual: &TypeRef,
        span: Span,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let environment = match &grouped(actual).kind {
            TypeKind::Reference { inner, .. } => inner.as_ref(),
            _ => actual,
        };
        if self.callable.contains(&index) && callable::opaque_signature(environment).is_some() {
            // A source factory identity is not an enumerable generic type key.
            // Types binds its opaque origin to the callback State independently.
            return Ok(());
        }
        if let Some(parameter) = self.parameters.get(index) {
            unify_argument(
                parameter,
                actual,
                &self.variables,
                &mut self.found,
                span,
                budget,
            )?;
        }
        Ok(())
    }
}

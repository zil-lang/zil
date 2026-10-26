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
pub(super) fn fields<'a>(decl: &'a EnumDecl, variant: &str, span: Span) -> R<&'a [TypeRef]> {
    let v = decl
        .variants
        .iter()
        .find(|v| v.name == variant)
        .ok_or_else(|| error(span, "unknown enum variant"))?;
    match &v.fields {
        VariantFields::Unit => Ok(&[]),
        VariantFields::Tuple(fields) => Ok(fields),
        VariantFields::Record(_) => Err(unsupported(span, "record enum variants")),
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
                    "u8" | "u16" | "i32" | "i64" | "bool" | "void" | "c_handle" | "c_wstr"
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
        TypeKind::Tuple(ts) => {
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
                "u8" | "u16" | "i32" | "i64" | "bool" | "void" | "c_handle" | "c_wstr"
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
                            VariantFields::Tuple(ts) => fields.extend(ts),
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
        TypeKind::Tuple(fields) => {
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
                let actual = self
                    .expr_expected(
                        &mut argument.value,
                        env,
                        (!unresolved).then_some(&hint),
                        depth + 1,
                    )?
                    .ok_or_else(|| error(argument.span, "enum payload type cannot be inferred"))?;
                let actual = self.original_type(&actual)?;
                unify(field, &actual, &vars, &mut found, span, &mut self.budget)?;
            }
        }
        let mut result = application(decl, &found, span, &mut self.budget)?;
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
        let variables = f
            .generics
            .iter()
            .map(|p| p.name.clone())
            .collect::<BTreeSet<_>>();
        let mut found = BTreeMap::new();
        if let Some(args) = explicit {
            if args.len() != f.generics.len() {
                return Err(error(callee.span, "type argument arity mismatch"));
            }
            for (p, a) in f.generics.iter().zip(args) {
                let GenericArg::Type(t) = a else {
                    return Err(unsupported(callee.span, "const generic argument"));
                };
                self.budget.ty(t)?;
                found.insert(p.name.clone(), t.clone());
            }
        }
        if let (Some(result), Some(expected)) = (&f.return_type, expected.as_ref()) {
            unify(
                result,
                expected,
                &variables,
                &mut found,
                callee.span,
                &mut self.budget,
            )?;
        }
        let parameters = f
            .params
            .iter()
            .map(|p| {
                self.budget.ty(&p.ty)?;
                Ok(p.ty.clone())
            })
            .collect::<R<Vec<_>>>()?;
        Ok(Some(CallHints {
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
        TypeKind::Array { element, .. }
        | TypeKind::Group(element)
        | TypeKind::Optional(element)
        | TypeKind::Reference { inner: element, .. } => contains(element, variable),
        _ => false,
    }
}

pub(super) struct CallHints {
    parameters: Vec<TypeRef>,
    variables: BTreeSet<String>,
    found: BTreeMap<String, TypeRef>,
}
impl CallHints {
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

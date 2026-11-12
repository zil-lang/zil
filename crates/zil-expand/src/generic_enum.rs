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

pub(super) fn source_field_actuals(
    formals: &[GenericParam],
    arguments: &[GenericArg],
    field: &TypeRef,
    budget: &mut Budget<'_>,
) -> R<TypeRef> {
    if formals.len() != arguments.len() {
        return Err(error(
            field.span,
            "source field needs full ordered nominal actuals",
        ));
    }
    let mut substitutions = BTreeMap::new();
    let mut lives = BTreeMap::new();
    for (formal, argument) in formals.iter().zip(arguments) {
        budget.text(&formal.name, formal.span)?;
        match (&formal.kind, argument) {
            (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                budget.text(name, field.span)?;
                budget.lineage_bytes(std::mem::size_of::<(String, String)>(), field.span)?;
                lives.insert(formal.name.clone(), name.clone());
            }
            (GenericParamKind::Type(_), GenericArg::Type(ty)) => {
                budget.ty(ty)?;
                budget.lineage_bytes(std::mem::size_of::<(String, TypeRef)>(), field.span)?;
                substitutions.insert(formal.name.clone(), ty.clone());
            }
            (GenericParamKind::Const(_), arg) => {
                let value = consts::argument(arg, Some(formal), budget)?;
                let value = consts::normalize(formal, &value, budget)?;
                budget.ty(&value)?;
                budget.lineage_bytes(std::mem::size_of::<(String, TypeRef)>(), field.span)?;
                substitutions.insert(formal.name.clone(), value);
            }
            _ => {
                return Err(error(
                    field.span,
                    "source field actual kind differs from formal",
                ));
            }
        }
    }
    budget.ty(field)?;
    let mut actual = field.clone();
    // Rebind only lifetimes written in the declaration. A caller's Type
    // actual may contain the same spelling with a different lexical owner;
    // inserting it first would capture that unrelated lifetime.
    source_field_lives(&mut actual, &lives, budget, 0)?;
    budget.substituted_type(&actual, &substitutions)?;
    Substitution {
        substitutions: &substitutions,
    }
    .ty(&mut actual);
    Ok(actual)
}

pub(super) fn source_field_lives(
    ty: &mut TypeRef,
    lives: &BTreeMap<String, String>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    match &mut ty.kind {
        TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
            for argument in args {
                match argument {
                    GenericArg::Lifetime(name) => {
                        budget.text(name, ty.span)?;
                        if let Some(actual) = lives.get(name) {
                            budget.text(actual, ty.span)?;
                            *name = actual.clone();
                        }
                    }
                    GenericArg::Type(inner) => source_field_lives(inner, lives, budget, depth + 1)?,
                    GenericArg::Const(_) => {}
                }
            }
        }
        TypeKind::Reference {
            lifetime, inner, ..
        } => {
            if let Some(name) = lifetime
                && let Some(actual) = lives.get(name)
            {
                budget.text(name, ty.span)?;
                budget.text(actual, ty.span)?;
                *name = actual.clone();
            }
            source_field_lives(inner, lives, budget, depth + 1)?;
        }
        TypeKind::Group(inner) | TypeKind::Optional(inner) => {
            source_field_lives(inner, lives, budget, depth + 1)?
        }
        TypeKind::Tuple(fields) | TypeKind::Union(fields) | TypeKind::Impl(fields) => {
            for field in fields {
                source_field_lives(field, lives, budget, depth + 1)?;
            }
        }
        TypeKind::Array { element, .. } => source_field_lives(element, lives, budget, depth + 1)?,
        TypeKind::Error { error, result } => {
            source_field_lives(error, lives, budget, depth + 1)?;
            source_field_lives(result, lives, budget, depth + 1)?;
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            for parameter in params {
                source_field_lives(parameter, lives, budget, depth + 1)?;
            }
            source_field_lives(result, lives, budget, depth + 1)?;
        }
        TypeKind::ForLifetime { .. } if !lives.is_empty() => {
            return Err(unsupported(
                ty.span,
                "source field nested lifetime binder needs scoped substitution",
            ));
        }
        _ => {}
    }
    Ok(())
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
    traits: &trait_bounds::Index<'_>,
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
    if decl
        .generics
        .iter()
        .any(|p| matches!(p.kind, GenericParamKind::Lifetime))
    {
        for variant in &decl.variants {
            for field in fields(decl, &variant.name, variant.span)? {
                if field.manages {
                    return Err(unsupported(field.span, "owned generic enum payload"));
                }
                scoped_shape(&field.ty, &decl.generics, nominals, budget, 0)?;
            }
        }
        // Every physical specialization is subsequently checked by the
        // stored nominal producer, Copy registration and three consumers.
        // The legacy layout walker represents formals as Type-only and
        // cannot faithfully encode this declaration's Life ordinals.
        return Ok(());
    }
    let variables = decl
        .generics
        .iter()
        .map(|p| p.name.clone())
        .collect::<BTreeSet<_>>();
    let bounds = traits.parameter_bounds(&decl.generics, budget)?;
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
            shape(field, &variables, &bounds, nominals, budget, 0)?;
        }
    }
    layout(
        &ty,
        &variables,
        &bounds,
        nominals,
        &mut BTreeSet::new(),
        budget,
        0,
    )
}

fn scoped_formal<'a>(
    formals: &'a [GenericParam],
    name: &str,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<Option<&'a GenericParam>> {
    for formal in formals {
        budget.node(formal.span, depth)?;
        budget.text(&formal.name, formal.span)?;
        budget.text(name, span)?;
        if formal.name == name {
            return Ok(Some(formal));
        }
    }
    Ok(None)
}

fn scoped_shape(
    ty: &TypeRef,
    formals: &[GenericParam],
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    match &ty.kind {
        TypeKind::Reference {
            mutable: false,
            lifetime: Some(lifetime),
            inner,
        } => {
            if !scoped_formal(formals, lifetime, budget, ty.span, depth + 1)?
                .is_some_and(|p| matches!(p.kind, GenericParamKind::Lifetime))
            {
                return Err(error(
                    ty.span,
                    "enum payload address lifetime is not a declaration binder",
                ));
            }
            scoped_shape(inner, formals, nominals, budget, depth + 1)
        }
        TypeKind::Named { path, args } if path.len() == 1 => {
            let name = &path[0];
            if let Some(formal) = scoped_formal(formals, name, budget, ty.span, depth + 1)? {
                if !matches!(formal.kind, GenericParamKind::Type(_)) || !args.is_empty() {
                    return Err(error(
                        ty.span,
                        "scoped enum payload formal needs a bare Type parameter",
                    ));
                }
                return Ok(());
            }
            if matches!(
                name.as_str(),
                "u8" | "u16" | "u64" | "usize" | "i32" | "i64" | "f32" | "f64" | "bool" | "void"
            ) {
                if !args.is_empty() {
                    return Err(error(ty.span, "scalar enum payload has type actuals"));
                }
                return Ok(());
            }
            budget.identity_work(
                nominals.len().saturating_mul(name.len().saturating_add(1)),
                ty.span,
            )?;
            let item = nominals
                .get(name)
                .ok_or_else(|| error(ty.span, "unknown scoped enum payload nominal"))?;
            let parameters = params(item);
            if parameters.len() != args.len() {
                return Err(error(
                    ty.span,
                    "scoped enum payload needs full actual arity",
                ));
            }
            for (parameter, actual) in parameters.iter().zip(args) {
                budget.node(parameter.span, depth + 1)?;
                match (&parameter.kind, actual) {
                    (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                        budget.text(name, ty.span)?;
                        if !scoped_formal(formals, name, budget, ty.span, depth + 1)?
                            .is_some_and(|p| matches!(p.kind, GenericParamKind::Lifetime))
                        {
                            return Err(error(
                                ty.span,
                                "scoped enum payload lifetime is not a declaration binder",
                            ));
                        }
                    }
                    (GenericParamKind::Type(_), GenericArg::Type(inner)) => {
                        scoped_shape(inner, formals, nominals, budget, depth + 1)?
                    }
                    (GenericParamKind::Const(domain), arg) => {
                        let carrier = consts::argument(arg, Some(parameter), budget)?;
                        let value = consts::expression(&carrier).ok_or_else(|| {
                            unsupported(ty.span, "scoped enum payload const has no expression")
                        })?;
                        if let ExprKind::Name(name) = &value.kind {
                            let Some(source) =
                                scoped_formal(formals, name, budget, ty.span, depth + 1)?
                            else {
                                return Err(error(
                                    ty.span,
                                    "scoped enum payload const lacks a source binder",
                                ));
                            };
                            let GenericParamKind::Const(source_domain) = &source.kind else {
                                return Err(error(
                                    ty.span,
                                    "scoped enum payload const binder kind differs",
                                ));
                            };
                            if type_key(domain, budget)? != type_key(source_domain, budget)? {
                                return Err(error(
                                    ty.span,
                                    "scoped enum payload const domain differs",
                                ));
                            }
                        } else {
                            consts::normalize(parameter, &carrier, budget)?;
                        }
                    }
                    _ => return Err(error(ty.span, "scoped enum payload actual kind differs")),
                }
            }
            Ok(())
        }
        TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
            for field in fields {
                scoped_shape(field, formals, nominals, budget, depth + 1)?;
            }
            Ok(())
        }
        TypeKind::Array {
            element,
            length: Some(length),
        } => {
            array_length(length)?;
            scoped_shape(element, formals, nominals, budget, depth + 1)
        }
        TypeKind::Group(inner) => scoped_shape(inner, formals, nominals, budget, depth + 1),
        _ => Err(unsupported(
            ty.span,
            "scoped enum payload requires finite Copy type syntax",
        )),
    }
}
// Validate every written type argument, including phantom arguments which do
// not contribute fields to the layout walk.
fn shape(
    ty: &TypeRef,
    variables: &BTreeSet<String>,
    bounds: &trait_bounds::Bounds,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    if trait_bounds::associated::parts(ty).is_some() {
        return copy::require(ty, nominals, bounds, variables, budget);
    }
    match &ty.kind {
        TypeKind::Named { path, args } if path.len() == 1 => {
            let name = &path[0];
            let arity = if name == "Managed" {
                1
            } else if variables.contains(name)
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
                )
            {
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
                shape(ty, variables, bounds, nominals, budget, depth + 1)?;
            }
        }
        TypeKind::Tuple(ts) | TypeKind::Union(ts) => {
            for ty in ts {
                shape(ty, variables, bounds, nominals, budget, depth + 1)?;
            }
        }
        TypeKind::Array {
            element,
            length: Some(length),
        } => {
            array_length(length)?;
            shape(element, variables, bounds, nominals, budget, depth + 1)?;
        }
        TypeKind::Group(t) => shape(t, variables, bounds, nominals, budget, depth + 1)?,
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
    bounds: &trait_bounds::Bounds,
    nominals: &BTreeMap<String, &Item>,
    active: &mut BTreeSet<String>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    if trait_bounds::associated::parts(ty).is_some() {
        return copy::require(ty, nominals, bounds, variables, budget);
    }
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
            if name == "Managed" {
                let [GenericArg::Type(payload)] = args.as_slice() else {
                    return Err(error(
                        ty.span,
                        "Managed requires one complete resource argument",
                    ));
                };
                return layout(
                    payload,
                    variables,
                    bounds,
                    nominals,
                    active,
                    budget,
                    depth + 1,
                );
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
                layout(
                    &field,
                    variables,
                    bounds,
                    nominals,
                    active,
                    budget,
                    depth + 1,
                )?;
            }
            active.remove(&key);
        }
        TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
            for field in fields {
                layout(
                    field,
                    variables,
                    bounds,
                    nominals,
                    active,
                    budget,
                    depth + 1,
                )?;
            }
        }
        TypeKind::Array {
            element,
            length: Some(length),
        } => {
            array_length(length)?;
            layout(
                element,
                variables,
                bounds,
                nominals,
                active,
                budget,
                depth + 1,
            )?;
        }
        TypeKind::Group(element) => layout(
            element,
            variables,
            bounds,
            nominals,
            active,
            budget,
            depth + 1,
        )?,
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
        let source_application = explicit.or_else(|| {
            expected.and_then(|ty| match &grouped(ty).kind {
                TypeKind::Named { path, args } if path.as_slice() == [decl.name.as_str()] => {
                    Some(args.as_slice())
                }
                _ => None,
            })
        });
        let scoped_enum = decl
            .generics
            .iter()
            .any(|p| matches!(p.kind, GenericParamKind::Lifetime));
        let stored_enum = self.stored_nominals
            && (scoped_enum
                || if let Some(args) = source_application {
                    self.stored_application(&decl.name, args, span, depth + 1)?
                } else {
                    false
                });
        if stored_enum {
            let source_arguments = if let Some(explicit) = explicit {
                if explicit.len() != decl.generics.len() {
                    return Err(error(
                        span,
                        "stored enum constructor needs full ordered actuals",
                    ));
                }
                for argument in explicit {
                    self.budget.node(span, depth)?;
                    match argument {
                        GenericArg::Type(ty) => self.budget.ty(ty)?,
                        GenericArg::Lifetime(name) => self.budget.text(name, span)?,
                        GenericArg::Const(value) => self.budget.const_expression(value)?,
                    }
                }
                explicit.to_vec()
            } else {
                let source_expected = expected.ok_or_else(|| {
                    unsupported(
                        span,
                        "stored enum constructor needs source actuals or a full source context",
                    )
                })?;
                let TypeKind::Named { path, args } = &grouped(source_expected).kind else {
                    return Err(unsupported(
                        span,
                        "stored enum constructor needs nominal source context",
                    ));
                };
                if path.as_slice() != [decl.name.as_str()] || args.len() != decl.generics.len() {
                    return Err(unsupported(
                        span,
                        "stored enum constructor source context differs",
                    ));
                }
                self.budget.ty(source_expected)?;
                args.clone()
            };
            let supplied = arguments.as_ref().map_or(0, |a| a.len());
            if supplied != raw_fields.len() {
                return Err(error(
                    span,
                    "stored enum constructor payload arity mismatch",
                ));
            }
            if let Some(values) = arguments {
                for (value, field) in values.iter_mut().zip(raw_fields) {
                    if value.spread {
                        return Err(unsupported(value.span, "enum payload tuple spread"));
                    }
                    let mut hint = source_field_actuals(
                        &decl.generics,
                        &source_arguments,
                        &field.ty,
                        &mut self.budget,
                    )?;
                    self.ty(&mut hint, depth + 1)?;
                    let actual = if let Some(actual) =
                        self.expr_expected(&mut value.value, env, Some(&hint), depth + 1)?
                    {
                        actual
                    } else {
                        self.value_hint(&value.value, env, depth + 1, true)?
                            .ok_or_else(|| {
                                error(value.span, "stored enum payload type cannot be inferred")
                            })?
                    };
                    let actual = self.original_type(&actual)?;
                    let expected = self.original_type(&hint)?;
                    if type_key(&actual, &mut self.budget)?
                        != type_key(&expected, &mut self.budget)?
                    {
                        return Err(error(
                            value.span,
                            "stored enum payload differs from source field",
                        ));
                    }
                }
            }
            for argument in &source_arguments {
                match argument {
                    GenericArg::Type(ty) => self.budget.ty(ty)?,
                    GenericArg::Lifetime(name) => self.budget.text(name, span)?,
                    GenericArg::Const(value) => self.budget.const_expression(value)?,
                }
            }
            let mut actuals = source_arguments.clone();
            let (generated, classes) =
                self.stored_nominal(&decl.name, &mut actuals, span, depth + 1)?;
            if let Some(expected) = expected
                && let TypeKind::Named { path, args } = &grouped(expected).kind
                && path.as_slice() == [decl.name.as_str()]
            {
                self.budget.ty(expected)?;
                let mut expected_args = args.clone();
                let (expected_name, expected_classes) =
                    self.stored_nominal(&decl.name, &mut expected_args, span, depth + 1)?;
                if generated != expected_name || classes != expected_classes {
                    return Err(error(
                        span,
                        "stored enum constructor differs from source context",
                    ));
                }
            }
            callee.kind = ExprKind::Member {
                object: Box::new(Expr {
                    kind: ExprKind::Name(generated.clone()),
                    span,
                }),
                member: MemberName::Name(variant),
            };
            // Keep the real source actuals in the expression environment for
            // a later pattern. The AST callee already names the physical enum;
            // a generated name alone cannot recover the caller's classes.
            return Ok(Some(TypeRef {
                span,
                kind: TypeKind::Named {
                    path: vec![decl.name.clone()],
                    args: source_arguments,
                },
            }));
        }
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
                let mut actual = self.original_type(ty)?;
                // A symbolic generic consumer may spell I.Item explicitly
                // while its concrete return context has already selected the
                // nominal impl equation. Resolve the actual projection by
                // that impl before comparing the two complete enum actuals.
                let selected_nominal = if self.traits.has_associated
                    && let Some((_, host, _)) = trait_bounds::associated::parts(&actual)
                    && let TypeKind::Named { path, .. } = &grouped(host).kind
                    && let [name] = path.as_slice()
                {
                    self.budget.text(name, host.span)?;
                    self.budget.identity_work(
                        self.nominals
                            .len()
                            .saturating_mul(name.len().saturating_add(1)),
                        host.span,
                    )?;
                    self.nominals.contains_key(name)
                } else {
                    false
                };
                if selected_nominal {
                    actual = self.traits.normalize_associated(
                        &actual,
                        &trait_bounds::Bounds::new(),
                        &BTreeSet::new(),
                        &mut self.budget,
                    )?;
                }
                found.insert(p.name.clone(), actual);
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
                let projected_hint = if !unresolved
                    && decl.generics.iter().any(|parameter| {
                        matches!(&parameter.kind, GenericParamKind::Type(bounds)
                            if bounds.iter().any(|bound| trait_instances::head(bound).is_some()))
                    })
                    && trait_bounds::associated::contains(field, &mut self.budget)?
                {
                    let bounds = self
                        .traits
                        .parameter_bounds(&decl.generics, &mut self.budget)?;
                    Some(self.traits.normalize_associated(
                        &hint,
                        &bounds,
                        &vars,
                        &mut self.budget,
                    )?)
                } else {
                    None
                };
                let actual = self.expr_expected(
                    &mut argument.value,
                    env,
                    (!unresolved).then_some(projected_hint.as_ref().unwrap_or(&hint)),
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
                let mut actual = self.original_type(&actual)?;
                if matches!(
                    argument.value.kind,
                    ExprKind::Unary {
                        op: UnaryOp::Give,
                        ..
                    }
                ) && let TypeKind::Named { path, args } = &grouped(&hint).kind
                    && path.as_slice() == ["Managed"]
                    && let [GenericArg::Type(resource)] = args.as_slice()
                    && type_key(resource, &mut self.budget)? == type_key(&actual, &mut self.budget)?
                {
                    // Source `give` supplies a value, while its earlier AST
                    // hint may still spell the nominal resource. Preserve the
                    // explicit complete type context. Types and both kernels
                    // still require an actual owning operand, not a Desc.
                    self.budget.ty(&hint)?;
                    actual = hint;
                }
                if let Some(projected_hint) = projected_hint {
                    unify(
                        &projected_hint,
                        &actual,
                        &BTreeSet::new(),
                        &mut found,
                        span,
                        &mut self.budget,
                    )?;
                } else {
                    unify(field, &actual, &vars, &mut found, span, &mut self.budget)?;
                }
            }
        }
        let mut result = application(decl, &found, span, &mut self.budget)?;
        let mut injection_context = expected;
        if !self.stored_enum_layouts.is_empty()
            && let Some(union) = injection_context.as_mut()
            && matches!(grouped(union).kind, TypeKind::Error { .. })
        {
            self.ty(union, depth + 1)?;
        }
        check_injection(&result, injection_context.as_ref(), &mut self.budget)?;
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
            PatternKind::ErrorArm {
                is_error,
                pattern: inner,
            } => {
                let ty =
                    ty.ok_or_else(|| error(pattern.span, "error arm type cannot be inferred"))?;
                let written = self.original_type(ty)?;
                let TypeKind::Error {
                    error: error_ty,
                    result,
                } = &grouped(&written).kind
                else {
                    return Err(error(pattern.span, "error arm requires an ErrorUnion"));
                };
                // Restoring source actuals identifies an aliased union, but
                // an already specialized arm keeps its complete physical
                // nominal instance for nested constructor patterns.
                let selected = match &grouped(ty).kind {
                    TypeKind::Error { error, result } => {
                        if *is_error {
                            error
                        } else {
                            result
                        }
                    }
                    _ => {
                        if *is_error {
                            error_ty
                        } else {
                            result
                        }
                    }
                };
                self.enum_pattern(inner, Some(selected), env, depth + 1)?;
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
                if let TypeKind::Named {
                    path: physical,
                    args,
                } = &grouped(ty).kind
                    && args.is_empty()
                    && physical.len() == 1
                    && !self.stored_enum_layouts.is_empty()
                {
                    self.stored_lookup_fee(
                        self.stored_enum_layouts.len(),
                        physical[0].len(),
                        pattern.span,
                    )?;
                    if let Some((template, variants)) = self.stored_enum_layouts.get(&physical[0]) {
                        self.budget.text(template, pattern.span)?;
                        if path.len() != 2 || (path[0] != *template && path[0] != physical[0]) {
                            return Err(error(
                                pattern.span,
                                "pattern enum differs from the actual specialized declaration",
                            ));
                        }
                        self.budget.identity_work(
                            variants
                                .len()
                                .saturating_mul(path[1].len().saturating_add(1)),
                            pattern.span,
                        )?;
                        let (_, fields) = variants
                            .iter()
                            .find(|(name, _)| *name == path[1])
                            .ok_or_else(|| {
                                error(pattern.span, "pattern tag is absent from the actual enum")
                            })?;
                        if fields.len() != arguments.len() {
                            return Err(error(
                                pattern.span,
                                "pattern payload arity differs from its actual tag",
                            ));
                        }
                        for field in fields {
                            self.budget.ty(field)?;
                        }
                        let fields = fields.clone();
                        path[0] = physical[0].clone();
                        for (child, field) in arguments.iter_mut().zip(fields) {
                            self.enum_pattern(child, Some(&field), env, depth + 1)?;
                        }
                        return Ok(());
                    }
                }
                let written = self.original_type(ty)?;
                if matches!(
                    &grouped(&written).kind,
                    TypeKind::Union(_) | TypeKind::Error { .. }
                ) {
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
                let scoped_enum = decl
                    .generics
                    .iter()
                    .any(|p| matches!(p.kind, GenericParamKind::Lifetime));
                let stored_enum = self.stored_nominals
                    && (scoped_enum
                        || self.stored_application(&decl.name, args, pattern.span, depth + 1)?);
                if stored_enum {
                    if args.len() != decl.generics.len() {
                        return Err(error(
                            pattern.span,
                            "stored enum pattern needs full source actuals",
                        ));
                    }
                    self.budget.ty(&written)?;
                    let mut instance_args = args.clone();
                    let (instance, _) = self.stored_nominal(
                        &decl.name,
                        &mut instance_args,
                        pattern.span,
                        depth + 1,
                    )?;
                    path[0] = instance;
                    for (child, field) in arguments.iter_mut().zip(raw) {
                        let mut actual = source_field_actuals(
                            &decl.generics,
                            args,
                            &field.ty,
                            &mut self.budget,
                        )?;
                        self.ty(&mut actual, depth + 1)?;
                        self.enum_pattern(child, Some(&actual), env, depth + 1)?;
                    }
                    return Ok(());
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
                if path.len() == 1 {
                    self.record_product_pattern(path, fields, ty, env, pattern.span, depth)?;
                } else {
                    self.record_enum_pattern(path, fields, ty, env, pattern.span, depth)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn record_field(&mut self, ty: &TypeRef, member: &str) -> R<Option<TypeRef>> {
        let Some(mut result) = self.source_record_field(ty, member)? else {
            return Ok(None);
        };
        self.ty(&mut result, 0)?;
        Ok(Some(result))
    }

    pub(super) fn source_record_field(&mut self, ty: &TypeRef, member: &str) -> R<Option<TypeRef>> {
        let original = self.original_type(ty)?;
        let TypeKind::Named { path, args } = &grouped(&original).kind else {
            return Ok(None);
        };
        let Some(Item {
            kind: ItemKind::Aggregate(decl),
            ..
        }) = self.nominals.get(&path.join("."))
        else {
            return self.stored_record_field_hint(&original, member);
        };
        let Some(field) = decl.fields.iter().find(|field| field.name == member) else {
            return Ok(None);
        };
        if decl.generics.len() != args.len() {
            return Err(error(ty.span, "generic record argument arity mismatch"));
        }
        if decl
            .generics
            .iter()
            .all(|p| matches!(p.kind, GenericParamKind::Type(_)))
            && args.iter().all(|arg| matches!(arg, GenericArg::Type(_)))
        {
            let mut found = BTreeMap::new();
            for (p, arg) in decl.generics.iter().zip(args) {
                let GenericArg::Type(t) = arg else {
                    unreachable!()
                };
                self.budget.ty(t)?;
                found.insert(p.name.clone(), t.clone());
            }
            return Ok(Some(substituted(&field.ty, &found, &mut self.budget)?));
        }
        Ok(Some(source_field_actuals(
            &decl.generics,
            args,
            &field.ty,
            &mut self.budget,
        )?))
    }
    pub(super) fn stored_record_field_hint(
        &mut self,
        ty: &TypeRef,
        member: &str,
    ) -> R<Option<TypeRef>> {
        let TypeKind::Named { path, args } = &grouped(ty).kind else {
            return Ok(None);
        };
        if path.len() != 1 || !args.is_empty() || self.stored_record_fields.is_empty() {
            return Ok(None);
        }
        self.stored_lookup_fee(self.stored_record_fields.len(), path[0].len(), ty.span)?;
        let Some(fields) = self.stored_record_fields.get(&path[0]) else {
            return Ok(None);
        };
        for (name, field) in fields {
            self.budget.text(name, ty.span)?;
            self.budget.text(member, ty.span)?;
            if name == member {
                self.budget.ty(field)?;
                return Ok(Some(field.clone()));
            }
        }
        Ok(None)
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
        let provider = self.provider(name, callee.span)?;
        let mut expected = expected.map(|ty| self.original_type(ty)).transpose()?;
        if !self.stored_enum_layouts.is_empty()
            && let Some(actual) = expected.as_mut()
        {
            let mut pending = vec![&*actual];
            let mut ordinary_sum = false;
            while let Some(ty) = pending.pop() {
                self.budget.node(ty.span, 0)?;
                match &grouped(ty).kind {
                    TypeKind::Named { path, .. } if path.len() == 1 => {
                        self.stored_lookup_fee(
                            self.stored_enum_layouts.len(),
                            path[0].len(),
                            ty.span,
                        )?;
                        ordinary_sum |= self
                            .stored_enum_layouts
                            .values()
                            .any(|(template, _)| template == &path[0]);
                    }
                    TypeKind::Error { error, result } => {
                        pending.push(error);
                        pending.push(result);
                    }
                    _ => {}
                }
            }
            if ordinary_sum {
                self.ty(actual, 0)?;
            }
        }
        let source_scope = self.source_unification_scope(env, callee.span)?;
        let Some(f) = self.signatures.get(name) else {
            return Ok(None);
        };
        let variables = type_parameters(f)
            .map(|p| p.name.clone())
            .collect::<BTreeSet<_>>();
        let mut found = BTreeMap::new();
        if let Some(args) = explicit {
            if args.len() != consts::parameters(f).count()
                && !(provider && args.len() < consts::parameters(f).count())
            {
                return Err(error(callee.span, "type argument arity mismatch"));
            }
            for (p, a) in consts::parameters(f).zip(args) {
                let actual = consts::argument(a, Some(p), &mut self.budget)?;
                let actual = consts::normalize(p, &actual, &mut self.budget)?;
                found.insert(p.name.clone(), actual);
            }
        }
        let source_result = if self.stored_nominals {
            f.return_type
                .as_ref()
                .map(|result| {
                    stored_option_source_hint(result, &self.instance_types, &mut self.budget)
                })
                .transpose()?
                .flatten()
        } else {
            None
        };
        if let (Some(result), Some(expected)) = (
            source_result.as_ref().or(f.return_type.as_ref()),
            expected.as_ref(),
        ) && callable::opaque_signature(result).is_none()
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
                    unify_scoped(
                        &result,
                        expected,
                        &variables,
                        &mut found,
                        callee.span,
                        &mut self.budget,
                        source_scope.as_ref(),
                    )?;
                }
            } else {
                unify_scoped(
                    result,
                    expected,
                    &variables,
                    &mut found,
                    callee.span,
                    &mut self.budget,
                    source_scope.as_ref(),
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
                if self.stored_nominals
                    && let Some(source) =
                        stored_option_source_hint(&p.ty, &self.instance_types, &mut self.budget)?
                {
                    return Ok(source);
                }
                Ok(p.ty.clone())
            })
            .collect::<R<Vec<_>>>()?;
        Ok(Some(CallHints {
            source_scope,
            callable: callable::template(f, &mut self.budget)?.map_or_else(Vec::new, |bounds| {
                bounds.roles.into_iter().map(|role| role.index).collect()
            }),
            parameters,
            variables,
            found,
        }))
    }
}
// Argument and result inference must compare the same private layout on both
// sides. The full retained catalog remains the only storage relation input.
fn stored_option_source_hint(
    ty: &TypeRef,
    instances: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<Option<TypeRef>> {
    let TypeKind::Named { path, args } = &grouped(ty).kind else {
        return Ok(None);
    };
    if path.len() != 1 || !args.is_empty() || instances.is_empty() {
        return Ok(None);
    }
    budget.identity_work(
        instances
            .len()
            .checked_ilog2()
            .map_or(1, |n| n as usize + 2)
            .saturating_mul(path[0].len().saturating_add(1))
            .saturating_mul(12),
        ty.span,
    )?;
    if instances.get(&path[0]).is_some_and(|source| {
        matches!(&grouped(source).kind, TypeKind::Named { path, .. }
            if path.as_slice() == [options::TEMPLATE])
    }) {
        return original_type(ty, instances, budget, 0).map(Some);
    }
    Ok(None)
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
    pub(super) source_scope: Option<Function>,
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
                unify_scoped(
                    parameter,
                    actual,
                    &self.variables,
                    &mut self.found,
                    span,
                    budget,
                    self.source_scope.as_ref(),
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
            unify_argument_scoped(
                parameter,
                actual,
                &self.variables,
                &mut self.found,
                span,
                budget,
                self.source_scope.as_ref(),
            )?;
        }
        Ok(())
    }
}

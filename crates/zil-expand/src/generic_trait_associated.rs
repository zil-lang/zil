//! Opaque, bound-supported associated type atoms. Equations are consulted only
//! after the ordinary impl solver has committed the selected premise tree.
use super::*;

pub(crate) const ATOM: &str = "$associated_projection";
pub(crate) fn parts(ty: &TypeRef) -> Option<(&str, &TypeRef, &TypeRef)> {
    let TypeKind::Named { path, args } = &grouped(ty).kind else {
        return None;
    };
    let [marker, member] = path.as_slice() else {
        return None;
    };
    let [GenericArg::Type(host), GenericArg::Type(application)] = args.as_slice() else {
        return None;
    };
    (marker == ATOM).then_some((member.as_str(), host, application))
}
pub(super) fn application(
    reference: &trait_instances::Reference,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<TypeRef> {
    budget.text(&reference.key.name, span)?;
    let mut args = Vec::new();
    for argument in &reference.arguments {
        budget.ty(argument)?;
        args.push(GenericArg::Type(argument.clone()));
    }
    Ok(TypeRef {
        kind: TypeKind::Named {
            path: vec![reference.key.name.clone()],
            args,
        },
        span,
    })
}
pub(crate) fn atom(
    host: &TypeRef,
    reference: &trait_instances::Reference,
    member: &str,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<TypeRef> {
    budget.ty(host)?;
    budget.text(ATOM, span)?;
    budget.text(member, span)?;
    Ok(TypeRef {
        kind: TypeKind::Named {
            path: vec![ATOM.into(), member.into()],
            args: vec![
                GenericArg::Type(host.clone()),
                GenericArg::Type(application(reference, budget, span)?),
            ],
        },
        span,
    })
}

/// One mapping belongs to one lexical declaration. The atom retains the actual
/// trait identity, all ordered arguments and the symbolic host; it is never a
/// nominal type looked up by the spelling of the member.
impl Index<'_> {
    fn projection_names(
        &self,
        available: &Bounds,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<BTreeMap<String, TypeRef>> {
        let mut result = BTreeMap::new();
        for (subject, bounds) in available {
            budget.text(subject, span)?;
            if subject.starts_with('\0') {
                continue;
            }
            for bound in bounds {
                budget.node(span, 0)?;
                let Bound::Nominal(reference) = bound else {
                    continue;
                };
                let (_, declaration) = self.declaration_item(reference);
                for member in &declaration.members {
                    budget.node(member.span, 0)?;
                    if let ItemKind::Const(constant) = &member.kind {
                        budget.text(subject, span)?;
                        budget.text(&constant.name, member.span)?;
                        let key = super::constants::key(subject, &constant.name);
                        budget.text(&key, span)?;
                        let value = super::constants::expression(
                            &named(subject, span),
                            reference,
                            &constant.name,
                            budget,
                            member.span,
                        )?;
                        let value = if let Some(prior) = result.get(&key) {
                            if consts::expression(prior)
                                .map(|e| super::constants::identity(e, budget))
                                .transpose()?
                                .flatten()
                                != super::constants::identity(&value, budget)?
                            {
                                Expr {
                                    span: member.span,
                                    kind: ExprKind::Name("$ambiguous_associated_constant".into()),
                                }
                            } else {
                                value
                            }
                        } else {
                            value
                        };
                        result.insert(key, consts::carrier(&value, budget)?);
                        budget.text(subject, span)?;
                        let marker = consts::carrier(
                            &Expr {
                                span,
                                kind: ExprKind::Name(super::constants::SUBJECT.into()),
                            },
                            budget,
                        )?;
                        result.insert(subject.clone(), marker);
                        continue;
                    }
                    let ItemKind::TypeAlias(alias) = &member.kind else {
                        continue;
                    };
                    budget.text(subject, span)?;
                    budget.text(&alias.name, member.span)?;
                    let key = format!("{subject}.{}", alias.name);
                    let value = atom(
                        &named(subject, span),
                        reference,
                        &alias.name,
                        budget,
                        member.span,
                    )?;
                    if let Some(prior) = result.get(&key) {
                        if type_key(prior, budget)? != type_key(&value, budget)? {
                            result.insert(
                                key,
                                named("$ambiguous_associated_projection", member.span),
                            );
                        }
                    } else {
                        result.insert(key, value);
                    }
                }
            }
        }
        Ok(result)
    }
    pub(crate) fn associated_bounds(
        &self,
        available: &mut Bounds,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let mut additions = Bounds::new();
        for (subject, bounds) in available.iter() {
            budget.node(Span::default(), 0)?;
            budget.text(subject, Span::default())?;
            if subject.starts_with('\0') {
                continue;
            }
            for bound in bounds {
                budget.node(Span::default(), 0)?;
                let Bound::Nominal(reference) = bound else {
                    continue;
                };
                let (item, _) = self.declaration_item(reference);
                budget.text(subject, item.span)?;
                self.associated_subject(
                    &named(subject, item.span),
                    reference,
                    &mut additions,
                    budget,
                )?;
            }
        }
        for (key, supplied) in additions {
            available.entry(key).or_default().extend(supplied);
        }
        Ok(())
    }
    pub(crate) fn associated_subject(
        &self,
        host: &TypeRef,
        reference: &trait_instances::Reference,
        available: &mut Bounds,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let (_, declaration) = self.declaration_item(reference);
        let mut substitutions = BTreeMap::new();
        for (parameter, actual) in declaration.generics.iter().zip(&reference.arguments) {
            budget.text(&parameter.name, parameter.span)?;
            budget.ty(actual)?;
            substitutions.insert(parameter.name.clone(), actual.clone());
        }
        budget.text("Self", host.span)?;
        budget.ty(host)?;
        substitutions.insert("Self".into(), host.clone());
        for member in &declaration.members {
            budget.node(member.span, 0)?;
            let ItemKind::TypeAlias(alias) = &member.kind else {
                continue;
            };
            let projected = atom(host, reference, &alias.name, budget, member.span)?;
            let key = subject_key(&projected, budget)?;
            let mut supplied = Vec::new();
            for bound in &alias.bounds {
                budget.substituted_type(bound, &substitutions)?;
                let mut bound = bound.clone();
                Substitution {
                    substitutions: &substitutions,
                }
                .ty(&mut bound);
                if copy::is_bound(&bound) {
                    supplied.push(Bound::Copy);
                } else if stores::is_bound(&bound) {
                    supplied.push(Bound::StoreElement);
                } else {
                    supplied.push(Bound::Nominal(self.pattern_reference(&bound, budget)?));
                }
            }
            available.entry(key).or_default().extend(supplied);
        }
        Ok(())
    }
}

fn method_substitutions(
    index: &Index<'_>,
    outer: &[GenericParam],
    member: &Item,
    bindings: Option<&Vec<trait_methods::Binding>>,
    budget: &mut Budget<'_>,
) -> R<Option<BTreeMap<String, TypeRef>>> {
    let Some(bindings) = bindings else {
        return Ok(None);
    };
    let ItemKind::Function(function) = &member.kind else {
        return Ok(None);
    };
    let mut parameters = Vec::new();
    for parameter in outer.iter().chain(&function.generics) {
        budget.node(parameter.span, 0)?;
        budget.text(&parameter.name, parameter.span)?;
        match &parameter.kind {
            GenericParamKind::Type(bounds) => {
                for bound in bounds {
                    budget.ty(bound)?;
                }
            }
            GenericParamKind::Const(ty) => budget.ty(ty)?,
            GenericParamKind::Lifetime => {}
        }
        parameters.push(parameter.clone());
    }
    for clause in &function.where_clause {
        budget.node(clause.span, 0)?;
        let BoundSubject::Type(subject) = &clause.subject else {
            continue;
        };
        let Some(subject) = plain(subject) else {
            continue;
        };
        for parameter in &mut parameters {
            budget.node(parameter.span, 0)?;
            budget.text(&parameter.name, parameter.span)?;
            budget.text(subject, clause.span)?;
            if parameter.name == subject {
                if let GenericParamKind::Type(bounds) = &mut parameter.kind {
                    for bound in &clause.bounds {
                        budget.ty(bound)?;
                        bounds.push(bound.clone());
                    }
                }
                break;
            }
        }
    }
    let mut bounds = index.parameter_bounds(&parameters, budget)?;
    let mut variables = BTreeSet::new();
    for parameter in &parameters {
        budget.node(parameter.span, 0)?;
        budget.text(&parameter.name, parameter.span)?;
        if matches!(parameter.kind, GenericParamKind::Type(_)) {
            variables.insert(parameter.name.clone());
        }
    }
    for clause in &function.where_clause {
        budget.node(clause.span, 0)?;
        if let BoundSubject::Type(subject) = &clause.subject
            && plain(subject).is_none()
        {
            index.add_subject_bounds(subject, &clause.bounds, &mut bounds, &variables, budget)?;
        }
    }
    let mut result = index.projection_names(&bounds, budget, member.span)?;
    for binding in bindings {
        if !matches!(binding.kind, GenericParamKind::Type(_)) {
            continue;
        }
        budget.text(&binding.original, member.span)?;
        budget.text(&binding.name, member.span)?;
        budget.text("\0constant:.", member.span)?;
        let old_value = format!("\0constant:{}.", binding.original);
        budget.text(&binding.name, member.span)?;
        budget.text("\0constant:.", member.span)?;
        let new_value = format!("\0constant:{}.", binding.name);
        for key in result.keys() {
            budget.node(member.span, 0)?;
            budget.text(key, member.span)?;
            budget.text(&binding.original, member.span)?;
        }
        result.retain(|key, _| {
            key != &binding.original
                && !key
                    .strip_prefix(&binding.original)
                    .is_some_and(|suffix| suffix.starts_with('.'))
                && !key.starts_with(&old_value)
        });
        let mut aliases = Vec::new();
        for (key, value) in &result {
            budget.node(member.span, 0)?;
            budget.text(key, member.span)?;
            budget.text(&binding.name, member.span)?;
            let suffix = if key == &binding.name {
                Some(("", false))
            } else if let Some(suffix) = key
                .strip_prefix(&binding.name)
                .filter(|suffix| suffix.starts_with('.'))
            {
                Some((suffix, false))
            } else {
                key.strip_prefix(&new_value).map(|suffix| (suffix, true))
            };
            if let Some((suffix, constant)) = suffix {
                budget.text(&binding.original, member.span)?;
                budget.text(suffix, member.span)?;
                budget.ty(value)?;
                let alias = if constant {
                    budget.text("\0constant:.", member.span)?;
                    format!("\0constant:{}.{}", binding.original, suffix)
                } else {
                    format!("{}{}", binding.original, suffix)
                };
                aliases.push((alias, value.clone()));
            }
        }
        for (key, value) in aliases {
            result.insert(key, value);
        }
    }
    Ok(Some(result))
}

pub(crate) fn prepare(
    module: &Module,
    enabled: bool,
    methods: Option<&BTreeMap<(usize, usize), Vec<trait_methods::Binding>>>,
    origins: Option<&origins::Origins>,
    budget: &mut Budget<'_>,
) -> R<Option<Module>> {
    if !enabled {
        return Ok(None);
    }
    for item in &module.items {
        budget.item(item)?;
    }
    let index = Index::declarations_with_origins(module, origins, budget)?;
    let mut result = module.clone();
    for (owner, item) in result.items.iter_mut().enumerate() {
        budget.context.origin = origins
            .map(|origins| origins.get(owner, item.span))
            .transpose()?;
        let original_parameters = match &item.kind {
            ItemKind::Impl(i) => i.generics.as_slice(),
            ItemKind::Trait(t) => t.generics.as_slice(),
            _ => params(item),
        };
        for parameter in original_parameters {
            budget.node(parameter.span, 0)?;
            budget.text(&parameter.name, parameter.span)?;
            match &parameter.kind {
                GenericParamKind::Type(bounds) => {
                    for bound in bounds {
                        budget.ty(bound)?;
                    }
                }
                GenericParamKind::Const(ty) => budget.ty(ty)?,
                GenericParamKind::Lifetime => {}
            }
        }
        let mut parameters = original_parameters.to_vec();
        if let ItemKind::Impl(implementation) = &item.kind {
            for clause in &implementation.where_clause {
                budget.node(clause.span, 0)?;
                let BoundSubject::Type(subject) = &clause.subject else {
                    continue;
                };
                let Some(subject) = plain(subject) else {
                    continue;
                };
                for parameter in &mut parameters {
                    budget.node(parameter.span, 0)?;
                    budget.text(&parameter.name, parameter.span)?;
                    budget.text(subject, clause.span)?;
                    if parameter.name == subject
                        && let GenericParamKind::Type(bounds) = &mut parameter.kind
                    {
                        for bound in &clause.bounds {
                            budget.ty(bound)?;
                            bounds.push(bound.clone());
                        }
                        break;
                    }
                }
            }
        }
        let self_application = match &item.kind {
            ItemKind::Impl(i) => {
                if let Some(application) = &i.trait_type {
                    budget.ty(application)?;
                }
                i.trait_type.clone()
            }
            ItemKind::Trait(t) => {
                budget.text(&t.name, item.span)?;
                let mut args = Vec::new();
                for p in &t.generics {
                    budget.node(p.span, 0)?;
                    budget.text(&p.name, p.span)?;
                    args.push(GenericArg::Type(named(&p.name, p.span)));
                }
                Some(TypeRef {
                    kind: TypeKind::Named {
                        path: vec![t.name.clone()],
                        args,
                    },
                    span: item.span,
                })
            }
            _ => None,
        };
        if let Some(application) = self_application {
            budget.text("Self", item.span)?;
            budget.node(item.span, 0)?;
            parameters.push(GenericParam {
                name: "Self".into(),
                kind: GenericParamKind::Type(vec![application]),
                span: item.span,
            });
        }
        // Ordinary callable parameters keep their independently checked callable
        // contract; the nominal projection index must not resolve Fn/FnMut as
        // a user trait merely because another item declares associated types.
        let bounds = if let ItemKind::Function(function) = &item.kind {
            index.bounds(function, budget)?
        } else {
            index.parameter_bounds(&parameters, budget)?
        };
        let substitutions = index.projection_names(&bounds, budget, item.span)?;
        match &mut item.kind {
            ItemKind::Trait(t) => {
                for (ordinal, member) in t.members.iter_mut().enumerate() {
                    let member_scope = method_substitutions(
                        &index,
                        &parameters,
                        member,
                        methods.and_then(|m| m.get(&(owner, ordinal))),
                        budget,
                    )?;
                    let substitutions = member_scope.as_ref().unwrap_or(&substitutions);
                    budget.substituted_item(member, substitutions)?;
                    if index.has_constants {
                        consts::prepay_scopes(member, substitutions, budget)?;
                    }
                    substitute_item(member, substitutions)?;
                }
            }
            ItemKind::Impl(i) => {
                budget.ty(&i.target)?;
                budget.text("Self", item.span)?;
                let self_substitution = BTreeMap::from([("Self".into(), i.target.clone())]);
                for clause in &mut i.where_clause {
                    if let BoundSubject::Type(subject) = &mut clause.subject {
                        budget.substituted_type(subject, &substitutions)?;
                        Substitution {
                            substitutions: &substitutions,
                        }
                        .ty(subject);
                    }
                    for bound in &mut clause.bounds {
                        budget.substituted_type(bound, &substitutions)?;
                        Substitution {
                            substitutions: &substitutions,
                        }
                        .ty(bound);
                    }
                    if let BoundSubject::Type(subject) = &mut clause.subject {
                        budget.substituted_type(subject, &self_substitution)?;
                        Substitution {
                            substitutions: &self_substitution,
                        }
                        .ty(subject);
                    }
                    for bound in &mut clause.bounds {
                        budget.substituted_type(bound, &self_substitution)?;
                        Substitution {
                            substitutions: &self_substitution,
                        }
                        .ty(bound);
                    }
                }
                for (ordinal, member) in i.members.iter_mut().enumerate() {
                    let member_scope = method_substitutions(
                        &index,
                        &parameters,
                        member,
                        methods.and_then(|m| m.get(&(owner, ordinal))),
                        budget,
                    )?;
                    let substitutions = member_scope.as_ref().unwrap_or(&substitutions);
                    budget.substituted_item(member, substitutions)?;
                    if index.has_constants {
                        consts::prepay_scopes(member, substitutions, budget)?;
                    }
                    substitute_item(member, substitutions)?;
                }
            }
            _ => {
                budget.substituted_item(item, &substitutions)?;
                if index.has_constants {
                    consts::prepay_scopes(item, &substitutions, budget)?;
                }
                substitute_item(item, &substitutions)?;
            }
        }
    }
    Ok(Some(result))
}

pub(crate) fn equation_key(ty: &TypeRef, budget: &mut Budget<'_>) -> R<String> {
    if budget.has_anonymous_sums() {
        budget.identity_work(11, ty.span)?;
    }
    Ok(format!("\0equation:{}", type_key_bounded(ty, budget)?))
}
// Substitution renders only after Budget::substituted_type/item paid its
// lookup and the subsequent replacement walk.
pub(crate) fn equation_key_prepaid(ty: &TypeRef) -> R<String> {
    Ok(format!("\0equation:{}", render_type_key(ty)?))
}
impl Index<'_> {
    pub(crate) fn definition_equations(
        &self,
        implementation: &ImplBlock,
        reference: &trait_instances::Reference,
        budget: &mut Budget<'_>,
    ) -> R<BTreeMap<String, TypeRef>> {
        let (_, declaration) = self.declaration_item(reference);
        let mut result = BTreeMap::new();
        let mut names = BTreeSet::new();
        for member in &implementation.members {
            budget.node(member.span, 0)?;
            if let ItemKind::TypeAlias(alias) = &member.kind {
                budget.text(&alias.name, member.span)?;
                if !names.insert(alias.name.as_str()) {
                    return Err(error(member.span, "duplicate associated type equation"));
                }
                let mut declared = false;
                for member in &declaration.members {
                    budget.node(member.span, 0)?;
                    if let ItemKind::TypeAlias(required) = &member.kind {
                        budget.text(&required.name, member.span)?;
                        budget.text(&alias.name, member.span)?;
                        declared |= required.name == alias.name;
                    }
                }
                if !declared {
                    return Err(error(
                        member.span,
                        "impl associated type is not declared by its trait",
                    ));
                }
                if alias.value.is_none() {
                    return Err(error(
                        member.span,
                        "impl associated type requires an equation",
                    ));
                }
                if !alias.generics.is_empty() || !member.attributes.is_empty() {
                    return Err(unsupported(
                        member.span,
                        "generic or attributed impl associated type",
                    ));
                }
            }
        }
        let mut substitutions = BTreeMap::new();
        budget.ty(&implementation.target)?;
        budget.text("Self", implementation.target.span)?;
        substitutions.insert("Self".into(), implementation.target.clone());
        for (p, a) in declaration.generics.iter().zip(&reference.arguments) {
            budget.text(&p.name, p.span)?;
            budget.ty(a)?;
            substitutions.insert(p.name.clone(), a.clone());
        }
        for member in &declaration.members {
            budget.node(member.span, 0)?;
            let ItemKind::TypeAlias(required) = &member.kind else {
                continue;
            };
            let mut value = None;
            for member in &implementation.members {
                budget.node(member.span, 0)?;
                if let ItemKind::TypeAlias(alias) = &member.kind {
                    budget.text(&alias.name, member.span)?;
                    budget.text(&required.name, member.span)?;
                    if alias.name == required.name {
                        value = alias.value.as_ref();
                        break;
                    }
                }
            }
            let from_impl = value.is_some();
            let value = value.or(required.value.as_ref()).ok_or_else(|| {
                error(
                    implementation.target.span,
                    "impl is missing its associated type equation",
                )
            })?;
            let self_substitution;
            let substitution = if from_impl {
                budget.text("Self", implementation.target.span)?;
                budget.ty(&implementation.target)?;
                self_substitution =
                    BTreeMap::from([("Self".into(), implementation.target.clone())]);
                &self_substitution
            } else {
                &substitutions
            };
            budget.substituted_type(value, substitution)?;
            let mut value = value.clone();
            Substitution {
                substitutions: substitution,
            }
            .ty(&mut value);
            let projected = atom(
                &implementation.target,
                reference,
                &required.name,
                budget,
                member.span,
            )?;
            budget.ty(&projected)?;
            let key = equation_key(&projected, budget)?;
            budget.text(&key, member.span)?;
            result.insert(key, value);
        }
        // A definition-local equation map checks signatures under the declared
        // impl assumptions. It is never passed to candidate premise solving.
        for value in result.values() {
            check_equation_cycles(value, &result, budget, &mut BTreeSet::new(), 0)?;
        }
        let mut expanded = BTreeMap::new();
        for (key, value) in &result {
            budget.text(key, value.span)?;
            budget.ty(value)?;
            let mut value = value.clone();
            let mut depth = 0;
            loop {
                budget.node(value.span, depth)?;
                let before = type_key(&value, budget)?;
                budget.substituted_type(&value, &result)?;
                Substitution {
                    substitutions: &result,
                }
                .ty(&mut value);
                if type_key(&value, budget)? == before {
                    break;
                }
                depth += 1;
            }
            expanded.insert(key.clone(), value);
        }
        Ok(expanded)
    }
}
fn check_equation_cycles(
    ty: &TypeRef,
    equations: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    active: &mut BTreeSet<String>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    if parts(ty).is_some() {
        budget.ty(ty)?;
        let key = equation_key(ty, budget)?;
        budget.text(&key, ty.span)?;
        if let Some(value) = equations.get(&key) {
            if !active.insert(key.clone()) {
                return Err(error(ty.span, "active associated type equation cycle"));
            }
            check_equation_cycles(value, equations, budget, active, depth + 1)?;
            active.remove(&key);
            return Ok(());
        }
    }
    match &ty.kind {
        TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
            for argument in args {
                budget.node(ty.span, depth)?;
                if let GenericArg::Type(t) = argument {
                    check_equation_cycles(t, equations, budget, active, depth + 1)?;
                }
            }
        }
        TypeKind::Reference { inner, .. }
        | TypeKind::Optional(inner)
        | TypeKind::Group(inner)
        | TypeKind::ForLifetime { inner, .. }
        | TypeKind::Array { element: inner, .. } => {
            check_equation_cycles(inner, equations, budget, active, depth + 1)?
        }
        TypeKind::Tuple(ts) | TypeKind::Union(ts) | TypeKind::Impl(ts) => {
            for t in ts {
                check_equation_cycles(t, equations, budget, active, depth + 1)?;
            }
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            for t in params {
                check_equation_cycles(t, equations, budget, active, depth + 1)?;
            }
            check_equation_cycles(result, equations, budget, active, depth + 1)?;
        }
        TypeKind::Error { error, result } => {
            check_equation_cycles(error, equations, budget, active, depth + 1)?;
            check_equation_cycles(result, equations, budget, active, depth + 1)?;
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn contains(ty: &TypeRef, budget: &mut Budget<'_>) -> R<bool> {
    use super::super::super::ct_selection::Visitor;
    struct Scan<'a, 'l> {
        budget: &'a mut Budget<'l>,
        found: bool,
    }
    impl Visitor for Scan<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            self.found |= parts(ty).is_some();
            super::super::super::ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut scan = Scan {
        budget,
        found: false,
    };
    scan.ty(ty, 0).map_err(|mut e| e.remove(0))?;
    Ok(scan.found)
}

impl Index<'_> {
    pub(super) fn validate_associated_defaults(
        &self,
        declaration: &TraitDecl,
        reference: &trait_instances::Reference,
        bounds: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let mut equations = BTreeMap::new();
        for member in &declaration.members {
            budget.node(member.span, 0)?;
            let ItemKind::TypeAlias(alias) = &member.kind else {
                continue;
            };
            if let Some(value) = &alias.value {
                budget.ty(value)?;
                let projection = atom(
                    &named("Self", member.span),
                    reference,
                    &alias.name,
                    budget,
                    member.span,
                )?;
                let key = equation_key(&projection, budget)?;
                budget.text(&key, member.span)?;
                equations.insert(key, value.clone());
            }
        }
        for value in equations.values() {
            check_equation_cycles(value, &equations, budget, &mut BTreeSet::new(), 0)?;
        }
        for member in &declaration.members {
            budget.node(member.span, 0)?;
            let ItemKind::TypeAlias(alias) = &member.kind else {
                continue;
            };
            for bound in &alias.bounds {
                budget.ty(bound)?;
                if !copy::is_bound(bound) && !stores::is_bound(bound) {
                    let reference = self.pattern_reference(bound, budget)?;
                    self.reference_arguments(&reference, bounds, variables, budget)?;
                }
            }
            let Some(value) = &alias.value else { continue };
            self.store_types(
                value,
                bounds,
                variables,
                budget,
                callable::StoreCallbacks::None,
            )?;
            for bound in &alias.bounds {
                budget.ty(bound)?;
                if copy::is_bound(bound) {
                    copy::require(value, &self.nominals, bounds, variables, budget)?;
                } else if stores::is_bound(bound) {
                    self.store_element(
                        value,
                        bounds,
                        variables,
                        budget,
                        callable::StoreCallbacks::None,
                    )?;
                } else {
                    let reference = self.pattern_reference(bound, budget)?;
                    self.solve(&reference, value, bounds, variables, budget)?;
                }
            }
        }
        Ok(())
    }
}

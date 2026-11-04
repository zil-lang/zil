//! Declaration-local method binders and ordinary generic provider recipes.
use super::*;

pub(super) struct Binding {
    pub(super) original: String,
    pub(super) name: String,
    pub(super) kind: GenericParamKind,
}
pub(super) struct Prepared {
    pub(super) module: Module,
    pub(super) bindings: BTreeMap<(usize, usize), Vec<Binding>>,
}
pub(super) fn prepare(
    module: &Module,
    enabled: bool,
    budget: &mut Budget<'_>,
) -> R<Option<Prepared>> {
    if !enabled {
        return Ok(None);
    }
    for item in &module.items {
        budget.item(item)?;
    }
    let mut module = module.clone();
    let mut bindings = BTreeMap::new();
    for (owner, item) in module.items.iter_mut().enumerate() {
        let members = match &mut item.kind {
            ItemKind::Trait(t) => &mut t.members,
            ItemKind::Impl(i) => &mut i.members,
            _ => continue,
        };
        for (ordinal, member) in members.iter_mut().enumerate() {
            budget.node(member.span, 0)?;
            let ItemKind::Function(method) = &member.kind else {
                continue;
            };
            if method.generics.is_empty() {
                continue;
            }
            let mut scope = Vec::new();
            let mut substitutions = BTreeMap::new();
            let mut names = BTreeSet::new();
            for (index, parameter) in method.generics.iter().enumerate() {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                if !names.insert((
                    matches!(parameter.kind, GenericParamKind::Lifetime),
                    parameter.name.as_str(),
                )) {
                    return Err(error(parameter.span, "duplicate method binder"));
                }
                if !matches!(parameter.kind, GenericParamKind::Lifetime)
                    && matches!(parameter.name.as_str(), "Self" | "self")
                {
                    return Err(error(parameter.span, "reserved method binder"));
                }
                let prefix = match parameter.kind {
                    GenericParamKind::Type(_) => "type",
                    GenericParamKind::Const(_) => "const",
                    GenericParamKind::Lifetime => "life",
                };
                budget.text(
                    "$method_type_00000000000000000000_00000000000000000000_00000000000000000000",
                    parameter.span,
                )?;
                let name = if matches!(parameter.kind, GenericParamKind::Lifetime) {
                    parameter.name.clone()
                } else {
                    format!("$method_{prefix}_{owner}_{ordinal}_{index}")
                };
                budget.text(&name, parameter.span)?;
                match &parameter.kind {
                    GenericParamKind::Type(bounds) => {
                        for bound in bounds {
                            budget.ty(bound)?;
                        }
                        substitutions.insert(parameter.name.clone(), named(&name, parameter.span));
                    }
                    GenericParamKind::Const(ty) => {
                        budget.ty(ty)?;
                        let value = consts::carrier(
                            &Expr {
                                span: parameter.span,
                                kind: ExprKind::Name(name.clone()),
                            },
                            budget,
                        )?;
                        substitutions.insert(parameter.name.clone(), value);
                    }
                    GenericParamKind::Lifetime => {}
                }
                scope.push(Binding {
                    original: parameter.name.clone(),
                    name,
                    kind: parameter.kind.clone(),
                });
            }
            budget.substituted_item(member, &substitutions)?;
            consts::prepay_scopes(member, &substitutions, budget)?;
            substitute_item(member, &substitutions)?;
            let ItemKind::Function(method) = &mut member.kind else {
                unreachable!()
            };
            for (parameter, binding) in method.generics.iter_mut().zip(&scope) {
                parameter.name = binding.name.clone();
            }
            bindings.insert((owner, ordinal), scope);
        }
    }
    Ok(Some(Prepared { module, bindings }))
}

const PROVIDER: &str = "$generic_method_";
pub(super) fn provider_name(
    owner: usize,
    member: usize,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<String> {
    budget.text(
        "$generic_method_00000000000000000000_00000000000000000000",
        span,
    )?;
    Ok(format!("{PROVIDER}{owner}_{member}"))
}
pub(super) fn helper(name: &str) -> bool {
    name.starts_with(PROVIDER)
}
fn parameter_copy(parameters: &[GenericParam], budget: &mut Budget<'_>) -> R<Vec<GenericParam>> {
    for parameter in parameters {
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
    Ok(parameters.to_vec())
}
fn merge_direct(function: &mut Function, budget: &mut Budget<'_>) -> R<()> {
    let mut remaining = Vec::new();
    for clause in std::mem::take(&mut function.where_clause) {
        budget.node(clause.span, 0)?;
        let name = match &clause.subject {
            BoundSubject::Type(subject) => trait_bounds::plain(subject),
            _ => None,
        };
        let mut index = None;
        if let Some(name) = name {
            for (ordinal, parameter) in function.generics.iter().enumerate() {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                budget.text(name, clause.span)?;
                if parameter.name == name {
                    index = Some(ordinal);
                    break;
                }
            }
        }
        if let Some(index) = index
            && clause.lifetime_bound.is_none()
        {
            if let GenericParamKind::Type(bounds) = &mut function.generics[index].kind {
                bounds.extend(clause.bounds);
                continue;
            }
        }
        remaining.push(clause);
    }
    function.where_clause = remaining;
    Ok(())
}
pub(super) fn lift(module: &Module, enabled: bool, budget: &mut Budget<'_>) -> R<Option<Module>> {
    if !enabled {
        return Ok(None);
    }
    for item in &module.items {
        budget.item(item)?;
    }
    let mut result = module.clone();
    let mut traits = BTreeMap::new();
    for item in &module.items {
        budget.node(item.span, 0)?;
        if let ItemKind::Trait(declaration) = &item.kind {
            budget.text(&declaration.name, item.span)?;
            traits.insert(declaration.name.as_str(), (item, declaration));
        }
    }
    for (owner, item) in module.items.iter().enumerate() {
        budget.node(item.span, 0)?;
        let members = match &item.kind {
            ItemKind::Trait(declaration) => &declaration.members,
            ItemKind::Impl(implementation) if implementation.trait_type.is_some() => {
                &implementation.members
            }
            _ => continue,
        };
        for (ordinal, member) in members.iter().enumerate() {
            budget.node(member.span, 0)?;
            let ItemKind::Function(method) = &member.kind else {
                continue;
            };
            if method.generics.is_empty() || matches!(method.body, FunctionBody::Declaration) {
                continue;
            }
            budget.item(member)?;
            let mut generated = method.clone();
            generated.name = provider_name(owner, ordinal, budget, member.span)?;
            let mut outer = match &item.kind {
                ItemKind::Trait(declaration) => {
                    let mut parameters = parameter_copy(&declaration.generics, budget)?;
                    let mut arguments = Vec::new();
                    for parameter in &declaration.generics {
                        budget.node(parameter.span, 0)?;
                        budget.text(&parameter.name, parameter.span)?;
                        arguments.push(GenericArg::Type(named(&parameter.name, parameter.span)));
                    }
                    budget.text(&declaration.name, item.span)?;
                    budget.text("Self", item.span)?;
                    parameters.push(GenericParam {
                        name: "Self".into(),
                        kind: GenericParamKind::Type(vec![TypeRef {
                            span: item.span,
                            kind: TypeKind::Named {
                                path: vec![declaration.name.clone()],
                                args: arguments,
                            },
                        }]),
                        span: item.span,
                    });
                    parameters
                }
                ItemKind::Impl(implementation) => {
                    let application = implementation.trait_type.as_ref().expect("guarded");
                    let name = trait_instances::head(application)
                        .ok_or_else(|| error(application.span, "method provider trait identity"))?;
                    budget.text(name, application.span)?;
                    let (declaration_item, declaration) =
                        traits.get(name).copied().ok_or_else(|| {
                            error(application.span, "method provider trait declaration absent")
                        })?;
                    let reference = trait_instances::arguments(
                        declaration_item.span,
                        declaration,
                        application,
                        budget,
                    )?;
                    let mut required = None;
                    for candidate in &declaration.members {
                        budget.node(candidate.span, 0)?;
                        if let ItemKind::Function(signature) = &candidate.kind {
                            budget.text(&signature.name, candidate.span)?;
                            budget.text(&method.name, member.span)?;
                            if signature.name == method.name {
                                required = Some(candidate);
                                break;
                            }
                        }
                    }
                    let required = required.ok_or_else(|| {
                        error(
                            member.span,
                            "generic impl method is not declared by its trait",
                        )
                    })?;
                    let signature =
                        trait_instances::signature(declaration, required, &reference, budget)?;
                    if !generated.params.first().is_some_and(|p| p.name == "self")
                        && let Some(receiver) =
                            signature.params.first().filter(|p| p.name == "self")
                    {
                        budget.item(required)?;
                        generated.params.insert(0, receiver.clone());
                    }
                    budget.text("Self", member.span)?;
                    budget.ty(&implementation.target)?;
                    let substitutions =
                        BTreeMap::from([("Self".into(), implementation.target.clone())]);
                    let mut generated_item = Item {
                        span: member.span,
                        public: false,
                        attributes: vec![],
                        kind: ItemKind::Function(generated),
                    };
                    budget.substituted_item(&generated_item, &substitutions)?;
                    substitute_item(&mut generated_item, &substitutions)?;
                    let ItemKind::Function(function) = generated_item.kind else {
                        unreachable!()
                    };
                    generated = function;
                    for clause in &implementation.where_clause {
                        budget.node(clause.span, 0)?;
                        if let BoundSubject::Type(subject) = &clause.subject {
                            budget.ty(subject)?;
                        }
                        for bound in &clause.bounds {
                            budget.ty(bound)?;
                        }
                        let mut clause = clause.clone();
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
                        generated.where_clause.push(clause);
                    }
                    parameter_copy(&implementation.generics, budget)?
                }
                _ => unreachable!(),
            };
            outer.extend(generated.generics);
            generated.generics = outer;
            merge_direct(&mut generated, budget)?;
            budget.node(member.span, 0)?;
            result.items.push(Item {
                span: member.span,
                public: false,
                attributes: vec![],
                kind: ItemKind::Function(generated),
            });
        }
    }
    Ok(Some(result))
}

impl trait_bounds::Index<'_> {
    pub(crate) fn method_provider(
        &self,
        witness: &trait_bounds::Witness<'_>,
        name: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<(String, Vec<TypeRef>)> {
        let ItemKind::Impl(implementation) = &witness.item.kind else {
            unreachable!()
        };
        let mut source = None;
        for (ordinal, member) in implementation.members.iter().enumerate() {
            budget.node(member.span, 0)?;
            if let ItemKind::Function(function) = &member.kind {
                budget.text(&function.name, member.span)?;
                budget.text(name, span)?;
                if function.name == name {
                    source = Some((witness.item, ordinal));
                    break;
                }
            }
        }
        let mut arguments = Vec::new();
        let (owner, member) = if let Some(source) = source {
            for parameter in &implementation.generics {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                if matches!(parameter.kind, GenericParamKind::Lifetime) {
                    continue;
                }
                let actual = witness
                    .substitutions
                    .get(&parameter.name)
                    .ok_or_else(|| error(span, "method provider impl argument not determined"))?;
                budget.ty(actual)?;
                arguments.push(actual.clone());
            }
            source
        } else {
            let (item, declaration) = self.declaration_item(&witness.reference);
            let mut source = None;
            for (ordinal, member) in declaration.members.iter().enumerate() {
                budget.node(member.span, 0)?;
                if let ItemKind::Function(function) = &member.kind {
                    budget.text(&function.name, member.span)?;
                    budget.text(name, span)?;
                    if function.name == name && !matches!(function.body, FunctionBody::Declaration)
                    {
                        source = Some((item, ordinal));
                        break;
                    }
                }
            }
            for actual in &witness.reference.arguments {
                budget.ty(actual)?;
                arguments.push(actual.clone());
            }
            budget.ty(&witness.actual)?;
            arguments.push(witness.actual.clone());
            source.ok_or_else(|| error(span, "generic method provider absent"))?
        };
        let ordinal = self.declaration_ordinal(owner, budget, span)?;
        Ok((provider_name(ordinal, member, budget, span)?, arguments))
    }
}

fn clauses(clauses: &[BoundClause], budget: &mut Budget<'_>) -> R<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for clause in clauses {
        budget.node(clause.span, 0)?;
        if clause.lifetime_bound.is_some() {
            return Err(unsupported(
                clause.span,
                "generic method life where relation",
            ));
        }
        let BoundSubject::Type(subject) = &clause.subject else {
            return Err(unsupported(clause.span, "generic method where subject"));
        };
        let subject = type_key(subject, budget)?;
        for bound in &clause.bounds {
            let bound = type_key(bound, budget)?;
            budget.text(&subject, clause.span)?;
            budget.text(&bound, clause.span)?;
            budget.text(":", clause.span)?;
            keys.insert(format!("{subject}:{bound}"));
        }
    }
    Ok(keys)
}
pub(super) fn align(
    mut actual: Function,
    required: &Function,
    budget: &mut Budget<'_>,
) -> R<Function> {
    if actual.generics.is_empty()
        && required.generics.is_empty()
        && actual.where_clause.is_empty()
        && required.where_clause.is_empty()
    {
        return Ok(actual);
    }
    if actual.generics.len() != required.generics.len() {
        return Err(error(
            actual.span,
            "impl method generic binder count differs",
        ));
    }
    let mut substitutions = BTreeMap::new();
    for (source, target) in actual.generics.iter().zip(&required.generics) {
        budget.node(source.span, 0)?;
        budget.text(&source.name, source.span)?;
        budget.text(&target.name, target.span)?;
        match (&source.kind, &target.kind) {
            (GenericParamKind::Type(_), GenericParamKind::Type(_)) => {
                substitutions.insert(source.name.clone(), named(&target.name, target.span));
            }
            (GenericParamKind::Const(_), GenericParamKind::Const(_)) => {
                let value = consts::carrier(
                    &Expr {
                        span: source.span,
                        kind: ExprKind::Name(target.name.clone()),
                    },
                    budget,
                )?;
                substitutions.insert(source.name.clone(), value);
            }
            (GenericParamKind::Lifetime, GenericParamKind::Lifetime) => {}
            _ => {
                return Err(error(
                    source.span,
                    "impl method generic binder kind differs",
                ));
            }
        }
    }
    let mut item = Item {
        span: actual.span,
        public: false,
        attributes: vec![],
        kind: ItemKind::Function(actual),
    };
    budget.substituted_item(&item, &substitutions)?;
    consts::prepay_scopes(&item, &substitutions, budget)?;
    substitute_item(&mut item, &substitutions)?;
    let ItemKind::Function(function) = item.kind else {
        unreachable!()
    };
    actual = function;
    for (source, target) in actual.generics.iter_mut().zip(&required.generics) {
        budget.text(&target.name, target.span)?;
        source.name = target.name.clone();
        match (&source.kind, &target.kind) {
            (GenericParamKind::Type(actual), GenericParamKind::Type(required)) => {
                let actual = actual
                    .iter()
                    .map(|ty| type_key(ty, budget))
                    .collect::<R<BTreeSet<_>>>()?;
                let required = required
                    .iter()
                    .map(|ty| type_key(ty, budget))
                    .collect::<R<BTreeSet<_>>>()?;
                if actual != required {
                    return Err(error(
                        source.span,
                        "impl method cannot strengthen or change generic bounds",
                    ));
                }
            }
            (GenericParamKind::Const(actual), GenericParamKind::Const(required)) => {
                if type_key(actual, budget)? != type_key(required, budget)? {
                    return Err(error(source.span, "impl method const domain differs"));
                }
            }
            (GenericParamKind::Lifetime, GenericParamKind::Lifetime) => {}
            _ => unreachable!(),
        }
    }
    if clauses(&actual.where_clause, budget)? != clauses(&required.where_clause, budget)? {
        return Err(error(
            actual.span,
            "impl method cannot strengthen or change where premises",
        ));
    }
    Ok(actual)
}

pub(super) fn extend_bounds(
    target: &mut trait_bounds::Bounds,
    source: &trait_bounds::Bounds,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    for (name, supplied) in source {
        budget.node(span, 0)?;
        budget.text(name, span)?;
        let entries = target.entry(name.clone()).or_default();
        for bound in supplied {
            budget.node(span, 0)?;
            for prior in entries.iter() {
                budget.node(span, 0)?;
                if let trait_bounds::Bound::Nominal(reference) = prior {
                    budget.text(&reference.key.name, span)?;
                    for argument in &reference.key.arguments {
                        budget.text(argument, span)?;
                    }
                }
            }
            if entries.contains(bound) {
                continue;
            }
            entries.push(match bound {
                trait_bounds::Bound::Copy => trait_bounds::Bound::Copy,
                trait_bounds::Bound::StoreElement => trait_bounds::Bound::StoreElement,
                trait_bounds::Bound::Nominal(reference) => {
                    let key = trait_instances::copy_key(&reference.key, budget, span)?;
                    let mut arguments = Vec::new();
                    for argument in &reference.arguments {
                        budget.ty(argument)?;
                        arguments.push(argument.clone());
                    }
                    trait_bounds::Bound::Nominal(trait_instances::Reference { key, arguments })
                }
            });
        }
    }
    Ok(())
}

pub(super) struct Arguments<'a>{
    pub(super) values:&'a [Argument],
    pub(super) explicit:Option<&'a [GenericArg]>,
    pub(super) expected:Option<&'a TypeRef>,
}

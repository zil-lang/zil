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
    origins: Option<&origins::Origins>,
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
        let declaration = origins
            .ok_or_else(|| error(item.span, "method owner provenance absent"))?
            .get(owner, item.span)?
            .declaration;
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
                    format!(
                        "$method_{prefix}_{}_{}_{ordinal}_{index}",
                        declaration.module.0, declaration.ordinal
                    )
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
                if !matches!(parameter.kind, GenericParamKind::Lifetime) {
                    budget.text(&binding.name, parameter.span)?;
                    parameter.name = binding.name.clone();
                }
            }
            bindings.insert((owner, ordinal), scope);
        }
    }
    inherit_method_headers(&mut module, &mut bindings, budget)?;
    Ok(Some(Prepared { module, bindings }))
}

fn inherit_method_headers(
    module: &mut Module,
    bindings: &mut BTreeMap<(usize, usize), Vec<Binding>>,
    budget: &mut Budget<'_>,
) -> R<()> {
    // Clone only the paid declaration inventory. An impl inherits the declared
    // method premises before projections are rewritten in its unused body.
    let mut traits = BTreeMap::new();
    for item in &module.items {
        budget.node(item.span, 0)?;
        if let ItemKind::Trait(declaration) = &item.kind {
            budget.item(item)?;
            budget.text(&declaration.name, item.span)?;
            traits.insert(declaration.name.clone(), (item.span, declaration.clone()));
        }
    }
    for (owner, item) in module.items.iter_mut().enumerate() {
        budget.node(item.span, 0)?;
        let ItemKind::Impl(implementation) = &mut item.kind else {
            continue;
        };
        let Some(application) = &implementation.trait_type else {
            continue;
        };
        let Some(name) = trait_instances::head(application) else {
            continue;
        };
        budget.text(name, application.span)?;
        let Some((span, declaration)) = traits.get(name) else {
            continue;
        };
        let reference = trait_instances::arguments(*span, declaration, application, budget)?;
        for (ordinal, member) in implementation.members.iter_mut().enumerate() {
            budget.node(member.span, 0)?;
            let ItemKind::Function(function) = &member.kind else {
                continue;
            };
            if function.generics.is_empty() {
                continue;
            }
            let mut required = None;
            for candidate in &declaration.members {
                budget.node(candidate.span, 0)?;
                if let ItemKind::Function(signature) = &candidate.kind {
                    budget.text(&signature.name, candidate.span)?;
                    budget.text(&function.name, member.span)?;
                    if signature.name == function.name {
                        required = Some(candidate);
                        break;
                    }
                }
            }
            let required =
                required.ok_or_else(|| error(member.span, "generic method absent from trait"))?;
            let signature = trait_instances::signature(declaration, required, &reference, budget)?;
            budget.item(member)?;
            let function = align(function.clone(), &signature, budget)?;
            if let Some(bindings) = bindings.get_mut(&(owner, ordinal)) {
                for (binding, parameter) in bindings.iter_mut().zip(&function.generics) {
                    budget.node(parameter.span, 0)?;
                    budget.text(&parameter.name, parameter.span)?;
                    binding.name = parameter.name.clone();
                }
            }
            member.kind = ItemKind::Function(function);
        }
    }
    Ok(())
}

const PROVIDER: &str = "$generic_method_";
pub(super) fn provider_name(
    member: origins::MemberId,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<String> {
    budget.text("$generic_method_m0000000000_d0000000000_n0000000000", span)?;
    Ok(format!(
        "{PROVIDER}m{}_d{}_c{}_n{}",
        member.declaration.module.0,
        member.declaration.ordinal,
        u8::from(member.declaration.compiler),
        member.ordinal
    ))
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
            && let GenericParamKind::Type(bounds) = &mut function.generics[index].kind
        {
            bounds.extend(clause.bounds);
            continue;
        }
        remaining.push(clause);
    }
    function.where_clause = remaining;
    Ok(())
}
pub(super) fn lift(
    module: &Module,
    enabled: bool,
    origins: Option<&mut origins::Origins>,
    budget: &mut Budget<'_>,
) -> R<Option<Module>> {
    if !enabled {
        return Ok(None);
    }
    for item in &module.items {
        budget.item(item)?;
    }
    let origins = origins.ok_or_else(|| error(module.span, "method lift provenance absent"))?;
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
            let mut origin = origins
                .get(owner, item.span)?
                .member(ordinal, member.span)?;
            origin.provider = true;
            generated.name = provider_name(origin.id(member.span)?, budget, member.span)?;
            let mut public = item.public;
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
                    public = declaration_item.public;
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
                    generated = align(generated, &signature, budget)?;
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
                    budget.ty(&implementation.target)?;
                    budget.ty(application)?;
                    budget.node(member.span, 0)?;
                    generated.where_clause.push(BoundClause {
                        span: member.span,
                        subject: BoundSubject::Type(implementation.target.clone()),
                        bounds: vec![application.clone()],
                        lifetime_bound: None,
                    });
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
            let mut provider = Item {
                span: member.span,
                public,
                attributes: vec![],
                kind: ItemKind::Function(generated),
            };
            trait_instances::rename_default_receiver(&mut provider, budget)?;
            result.items.push(provider);
            budget.node(member.span, 0)?;
            origins.entries.push(origin);
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
        let origin = self
            .origin(owner, budget)?
            .ok_or_else(|| error(span, "method provider origin absent"))?
            .member(member, span)?;
        Ok((provider_name(origin.id(span)?, budget, span)?, arguments))
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
fn contract(function: &Function, budget: &mut Budget<'_>) -> R<BTreeSet<String>> {
    let mut result = clauses(&function.where_clause, budget)?;
    for parameter in &function.generics {
        budget.node(parameter.span, 0)?;
        let GenericParamKind::Type(bounds) = &parameter.kind else {
            continue;
        };
        budget.text(&parameter.name, parameter.span)?;
        budget.node(parameter.span, 0)?;
        let subject = type_key(&named(&parameter.name, parameter.span), budget)?;
        for bound in bounds {
            let bound = type_key(bound, budget)?;
            budget.text(&subject, parameter.span)?;
            budget.text(&bound, parameter.span)?;
            budget.text(":", parameter.span)?;
            result.insert(format!("{subject}:{bound}"));
        }
    }
    Ok(result)
}
fn contract_subset(
    actual: &BTreeSet<String>,
    required: &BTreeSet<String>,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<bool> {
    let mut required = required.iter();
    let mut next = required.next();
    for actual in actual {
        loop {
            budget.node(span, 0)?;
            let Some(bound) = next else { return Ok(false) };
            budget.text(actual, span)?;
            budget.text(bound, span)?;
            match actual.cmp(bound) {
                std::cmp::Ordering::Less => return Ok(false),
                std::cmp::Ordering::Equal => {
                    next = required.next();
                    break;
                }
                std::cmp::Ordering::Greater => next = required.next(),
            }
        }
    }
    Ok(true)
}

// Type keys intentionally erase reference lifetime spelling. A trait method
// signature needs a second comparison for the complete input/result relation:
// binder ordinal zero in an impl has the meaning of ordinal zero in the trait,
// even when the source names differ.
fn same_life_relation(
    actual: &TypeRef,
    required: &TypeRef,
    actual_lives: &[&str],
    required_lives: &[&str],
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<bool> {
    budget.node(actual.span, depth)?;
    budget.node(required.span, depth)?;
    let actual = grouped(actual);
    let required = grouped(required);
    match (&actual.kind, &required.kind) {
        (
            TypeKind::Reference {
                lifetime: a,
                inner: ai,
                ..
            },
            TypeKind::Reference {
                lifetime: b,
                inner: bi,
                ..
            },
        ) => {
            let ordinal = |name: &str, names: &[&str], budget: &mut Budget<'_>, span| -> R<_> {
                for (index, candidate) in names.iter().enumerate() {
                    budget.node(span, depth)?;
                    budget.text(name, span)?;
                    budget.text(candidate, span)?;
                    if *candidate == name {
                        return Ok(Some(index));
                    }
                }
                Err(error(
                    span,
                    "method reference uses an undeclared life binder",
                ))
            };
            let a = a
                .as_ref()
                .map(|name| ordinal(name, actual_lives, budget, actual.span))
                .transpose()?;
            let b = b
                .as_ref()
                .map(|name| ordinal(name, required_lives, budget, required.span))
                .transpose()?;
            Ok(a == b
                && same_life_relation(ai, bi, actual_lives, required_lives, budget, depth + 1)?)
        }
        (TypeKind::Named { args: a, .. }, TypeKind::Named { args: b, .. })
            if a.len() == b.len() =>
        {
            for (a, b) in a.iter().zip(b) {
                budget.node(actual.span, depth)?;
                match (a, b) {
                    (GenericArg::Type(a), GenericArg::Type(b)) => {
                        if !same_life_relation(
                            a,
                            b,
                            actual_lives,
                            required_lives,
                            budget,
                            depth + 1,
                        )? {
                            return Ok(false);
                        }
                    }
                    (GenericArg::Lifetime(a), GenericArg::Lifetime(b)) => {
                        let find = |name: &str, names: &[&str], budget: &mut Budget<'_>| -> R<_> {
                            for (index, candidate) in names.iter().enumerate() {
                                budget.node(actual.span, depth)?;
                                budget.text(name, actual.span)?;
                                budget.text(candidate, actual.span)?;
                                if *candidate == name {
                                    return Ok(Some(index));
                                }
                            }
                            Err(error(
                                actual.span,
                                "method type argument uses an undeclared life binder",
                            ))
                        };
                        if find(a, actual_lives, budget)? != find(b, required_lives, budget)? {
                            return Ok(false);
                        }
                    }
                    (GenericArg::Const(_), GenericArg::Const(_)) => {}
                    _ => return Ok(false),
                }
            }
            Ok(true)
        }
        (TypeKind::Optional(a), TypeKind::Optional(b))
        | (TypeKind::Array { element: a, .. }, TypeKind::Array { element: b, .. }) => {
            same_life_relation(a, b, actual_lives, required_lives, budget, depth + 1)
        }
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                if !same_life_relation(a, b, actual_lives, required_lives, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (TypeKind::Function { .. }, _)
        | (TypeKind::ForeignFunction { .. }, _)
        | (TypeKind::Error { .. }, _)
        | (TypeKind::Union(_), _)
        | (TypeKind::Impl(_), _)
        | (TypeKind::Dyn { .. }, _)
        | (TypeKind::ForLifetime { .. }, _)
        | (TypeKind::MetaEval(_), _)
        | (TypeKind::MetaSplice(_), _)
        | (TypeKind::MetaBlock(_), _) => Err(unsupported(
            actual.span,
            "method-local life in this type constructor needs a scoped relation check",
        )),
        // Scalar and other leaf positions carry no method-local life.
        _ => Ok(true),
    }
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
    let mut life_count = 0usize;
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
            (GenericParamKind::Lifetime, GenericParamKind::Lifetime) => {
                life_count = life_count.saturating_add(1);
            }
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
    let mut actual_lives = Vec::new();
    let mut required_lives = Vec::new();
    if life_count != 0 {
        budget.lineage_bytes(
            life_count
                .saturating_mul(2)
                .saturating_mul(std::mem::size_of::<&str>()),
            actual.span,
        )?;
        actual_lives.reserve_exact(life_count);
        required_lives.reserve_exact(life_count);
        for (source, target) in actual.generics.iter().zip(&required.generics) {
            budget.node(source.span, 0)?;
            budget.node(target.span, 0)?;
            if matches!(source.kind, GenericParamKind::Lifetime) {
                actual_lives.push(source.name.as_str());
                required_lives.push(target.name.as_str());
            }
        }
    }
    if !actual_lives.is_empty() {
        let implicit_receiver = actual
            .params
            .first()
            .map(|p| p.name != "self")
            .unwrap_or(false)
            && required.params.first().is_some_and(|p| p.name == "self");
        if actual.params.len() + usize::from(implicit_receiver) != required.params.len() {
            return Err(error(actual.span, "impl method parameter count differs"));
        }
        for (source, target) in actual
            .params
            .iter()
            .zip(required.params.iter().skip(usize::from(implicit_receiver)))
        {
            if !same_life_relation(
                &source.ty,
                &target.ty,
                &actual_lives,
                &required_lives,
                budget,
                0,
            )? {
                return Err(error(
                    source.span,
                    "impl method input life relation differs from trait declaration",
                ));
            }
        }
        if let (Some(source), Some(target)) = (&actual.return_type, &required.return_type)
            && !same_life_relation(source, target, &actual_lives, &required_lives, budget, 0)?
        {
            return Err(error(
                source.span,
                "impl method result life relation differs from trait declaration",
            ));
        }
    }
    for (source, target) in actual.generics.iter_mut().zip(&required.generics) {
        budget.text(&target.name, target.span)?;
        if !matches!(source.kind, GenericParamKind::Lifetime) {
            source.name = target.name.clone();
        }
        match (&source.kind, &target.kind) {
            (GenericParamKind::Type(_), GenericParamKind::Type(_)) => {}
            (GenericParamKind::Const(actual), GenericParamKind::Const(required)) => {
                if type_key(actual, budget)? != type_key(required, budget)? {
                    return Err(error(source.span, "impl method const domain differs"));
                }
            }
            (GenericParamKind::Lifetime, GenericParamKind::Lifetime) => {}
            _ => unreachable!(),
        }
    }
    if !contract_subset(
        &contract(&actual, budget)?,
        &contract(required, budget)?,
        budget,
        actual.span,
    )? {
        return Err(error(
            actual.span,
            "impl method cannot strengthen or change where premises",
        ));
    }
    for (source, target) in actual.generics.iter_mut().zip(&required.generics) {
        match &target.kind {
            GenericParamKind::Type(bounds) => {
                for bound in bounds {
                    budget.ty(bound)?;
                }
            }
            GenericParamKind::Const(ty) => budget.ty(ty)?,
            GenericParamKind::Lifetime => {}
        }
        source.kind = target.kind.clone();
    }
    for clause in &required.where_clause {
        budget.node(clause.span, 0)?;
        if let BoundSubject::Type(subject) = &clause.subject {
            budget.ty(subject)?;
        }
        for bound in &clause.bounds {
            budget.ty(bound)?;
        }
    }
    actual.where_clause = required.where_clause.clone();
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

pub(super) struct Arguments<'a> {
    pub(super) site: &'a Expr,
    pub(super) values: &'a [Argument],
    pub(super) explicit: Option<&'a [GenericArg]>,
    pub(super) expected: Option<&'a TypeRef>,
}

impl Engine<'_> {
    /// Rewrites a proven method into an existing ordinary generic call. The
    /// provider registry is supplied by the private declaration ledger.
    pub(super) fn method_call(&mut self, expression: &mut Expr, env: &Env, depth: usize) -> R<()> {
        if !self.traits.has_generic_methods {
            return Ok(());
        }
        let ExprKind::Call { callee, .. } = &expression.kind else {
            return Ok(());
        };
        let member = match &callee.kind {
            ExprKind::TypeApply { callee, .. } => &callee.kind,
            kind => kind,
        };
        if !matches!(
            member,
            ExprKind::Member {
                member: MemberName::Name(_) | MemberName::Constrained { .. },
                ..
            }
        ) {
            return Ok(());
        }
        self.method_call_selected(expression, env, depth)
    }

    // The provider/witness locals must not occupy ordinary recursive calls,
    // including requests that contain unrelated generic method declarations.
    #[inline(never)]
    fn method_call_selected(&mut self, expression: &mut Expr, env: &Env, depth: usize) -> R<()> {
        let ExprKind::Call { callee, .. } = &expression.kind else {
            return Ok(());
        };
        let (member_expr, explicit) = match &callee.kind {
            ExprKind::TypeApply { callee, arguments } => (callee.as_ref(), Some(arguments)),
            _ => (callee.as_ref(), None),
        };
        let ExprKind::Member {
            object,
            member: MemberName::Name(member) | MemberName::Constrained { name: member, .. },
        } = &member_expr.kind
        else {
            return Ok(());
        };
        let mut qualified = false;
        let mut actual = self.value_hint(object, env, depth + 1, false)?;
        if actual.is_none() {
            let no_variables = BTreeSet::new();
            let no_bounds = trait_bounds::Bounds::new();
            if let Some(static_host::Host::Nominal(host)) = static_host::classify(
                object,
                member,
                |name| env.contains_key(name),
                &self.nominals,
                &no_variables,
                &no_bounds,
                &self.traits,
                &mut self.budget,
            )? {
                actual = Some(host);
                qualified = true;
            }
        }
        let Some(actual) = actual else { return Ok(()) };
        let actual = self.original_type(&actual)?;
        let host = projection_shape(&actual);
        if dynamic::is_interface(host) {
            return Ok(());
        }
        let captured = self
            .method_calls
            .remove(&(member_expr as *const Expr as usize));
        let selected = if let Some(target) = captured {
            let witness = self
                .traits
                .solve(
                    &target.reference,
                    host,
                    &trait_bounds::Bounds::new(),
                    &BTreeSet::new(),
                    &mut self.budget,
                )?
                .ok_or_else(|| error(expression.span, "generic method witness absent"))?;
            let (_, declaration) = self.traits.declaration_item(&target.reference);
            let mut signature = None;
            for candidate in &declaration.members {
                self.budget.node(candidate.span, 0)?;
                if let ItemKind::Function(function) = &candidate.kind {
                    self.budget.text(&function.name, candidate.span)?;
                    if function.name == target.member {
                        let selected = trait_instances::signature(
                            declaration,
                            candidate,
                            &target.reference,
                            &mut self.budget,
                        )?;
                        if let Cow::Borrowed(function) = &selected {
                            self.budget.const_function(function)?;
                        }
                        signature = Some(selected.into_owned());
                        break;
                    }
                }
            }
            Some((
                target.member,
                signature.ok_or_else(|| error(expression.span, "captured method absent"))?,
                witness,
            ))
        } else {
            self.traits
                .recipe_generic_method(host, member, &mut self.budget, expression.span)?
                .map(|(_, mut signature, witness)| {
                    (std::mem::take(&mut signature.name), signature, witness)
                })
        };
        let Some((name, signature, witness)) = selected else {
            return Ok(());
        };
        let (provider, outer) =
            self.traits
                .method_provider(&witness, &name, &mut self.budget, expression.span)?;
        let mut args = Vec::new();
        for actual in outer {
            self.budget.ty(&actual)?;
            args.push(GenericArg::Type(actual));
        }
        if let Some(explicit) = explicit {
            for argument in explicit {
                match argument {
                    GenericArg::Lifetime(name) => self.budget.text(name, expression.span)?,
                    GenericArg::Type(ty) => self.budget.ty(ty)?,
                    GenericArg::Const(value) => self.budget.const_expression(value)?,
                };
            }
            args.extend(explicit.iter().cloned());
        }
        self.budget.node(expression.span, depth)?;
        let receiver = signature
            .params
            .first()
            .filter(|parameter| parameter.name == "self");
        if receiver.is_none() && !qualified {
            return Err(error(
                expression.span,
                "static trait method requires a type-qualified call",
            ));
        }
        let borrow = receiver.and_then(|receiver| match &grouped(&receiver.ty).kind {
            TypeKind::Reference { mutable, .. } => Some(if *mutable {
                UnaryOp::BorrowMut
            } else {
                UnaryOp::Borrow
            }),
            _ => None,
        });
        let ExprKind::Call { callee, arguments } = &mut expression.kind else {
            unreachable!()
        };
        let base = match &mut callee.kind {
            ExprKind::TypeApply { callee, .. } => callee.as_mut(),
            _ => callee.as_mut(),
        };
        let ExprKind::Member { object, .. } = &mut base.kind else {
            unreachable!()
        };
        if !qualified {
            for argument in arguments.iter() {
                self.budget.node(argument.span, depth)?;
            }
            let mut value = std::mem::replace(
                object,
                Box::new(Expr {
                    span: expression.span,
                    kind: ExprKind::Name(String::new()),
                }),
            );
            if let Some(op) = borrow
                && !matches!(grouped(&actual).kind, TypeKind::Reference { .. })
            {
                self.budget.node(value.span, depth)?;
                value = Box::new(Expr {
                    span: value.span,
                    kind: ExprKind::Unary { op, operand: value },
                });
            }
            arguments.insert(
                0,
                Argument {
                    span: value.span,
                    spread: false,
                    value: *value,
                },
            );
        }
        callee.kind = if args.is_empty() {
            ExprKind::Name(provider)
        } else {
            ExprKind::TypeApply {
                callee: Box::new(Expr {
                    span: callee.span,
                    kind: ExprKind::Name(provider),
                }),
                arguments: args,
            }
        };
        Ok(())
    }
}

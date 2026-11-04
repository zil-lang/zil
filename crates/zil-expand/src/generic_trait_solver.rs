//! First-order impl recipes. A matching head is provisional until every premise
//! has been proved; only then may its source body be materialized for dispatch.
use super::*;

pub(crate) struct Witness<'a> {
    pub item: &'a Item,
    pub recipe: usize,
    pub substitutions: BTreeMap<String, TypeRef>,
    pub reference: trait_instances::Reference,
    pub actual: TypeRef,
    pub dependencies: Vec<Witness<'a>>,
}

fn substituted(ty: &TypeRef, substitutions: &BTreeMap<String, TypeRef>, budget: &mut Budget<'_>) -> R<TypeRef> {
    budget.substituted_type(ty, substitutions)?;
    let mut result = ty.clone();
    Substitution { substitutions }.ty(&mut result);
    Ok(result)
}

impl<'a> Index<'a> {
    pub(crate) fn solve(
        &self, reference: &trait_instances::Reference, actual: &TypeRef,
        available: &Bounds, variables: &BTreeSet<String>, budget: &mut Budget<'_>,
    ) -> R<Option<Witness<'a>>> {
        self.solve_active(reference, actual, available, variables, budget, &mut BTreeSet::new(), 0)
    }

    fn solve_active(
        &self, reference: &trait_instances::Reference, actual: &TypeRef,
        available: &Bounds, variables: &BTreeSet<String>, budget: &mut Budget<'_>,
        active: &mut BTreeSet<(trait_instances::Key, String)>, depth: usize,
    ) -> R<Option<Witness<'a>>> {
        budget.node(actual.span, depth)?;
        let actual_key = type_key(actual, budget)?;
        if let Some(variable) = plain(actual).filter(|v| variables.contains(*v)) {
            for bound in available.get(variable).into_iter().flatten() {
                budget.node(actual.span, depth)?;
                if let Bound::Nominal(r) = bound {
                    trait_instances::copy_key(&r.key, budget, actual.span)?;
                    if r.key == reference.key { return Ok(None); }
                }
            }
            return Err(error(actual.span, "nominal obligation is not among the declared bounds"));
        }
        if let Some(host) = plain(actual)
            && self.implementations.contains(&(trait_instances::copy_key(&reference.key, budget, actual.span)?, host)) {
            return Ok(None);
        }
        let goal = (trait_instances::copy_key(&reference.key, budget, actual.span)?, actual_key);
        if active.contains(&goal) {
            return Err(error(actual.span, "active impl obligation cycle"));
        }
        active.insert(goal.clone());
        let mut selected = None;
        for (recipe, item) in self.recipes.iter().enumerate() {
            budget.node(item.span, depth)?;
            let ItemKind::Impl(implementation) = &item.kind else { unreachable!() };
            let application = implementation.trait_type.as_ref().expect("trait recipe");
            if trait_instances::head(application) != Some(reference.key.name.as_str()) { continue; }
            let variables: BTreeSet<_> = implementation.generics.iter().map(|p| p.name.clone()).collect();
            let mut found = BTreeMap::new();
            if !matches_head(&implementation.target, actual, &variables, &mut found, item.span, budget)? { continue; }
            let pattern = self.pattern_reference(application, budget)?;
            if pattern.arguments.len() != reference.arguments.len() { continue; }
            let mut matched = true;
            for (pattern, actual) in pattern.arguments.iter().zip(&reference.arguments) {
                if !matches_head(pattern, actual, &variables, &mut found, item.span, budget)? { matched = false; break; }
            }
            if !matched { continue; }
            if selected.is_some() { return Err(error(item.span, "overlapping nominal impl recipes")); }
            for parameter in &implementation.generics {
                budget.node(parameter.span, depth)?;
                if !found.contains_key(&parameter.name) {
                    return Err(error(parameter.span, "impl binder is not determined by its nominal head"));
                }
            }
            selected = Some((recipe, *item, found));
        }
        let Some((recipe, item, substitutions)) = selected else {
            return Err(error(actual.span, format!("type does not satisfy nominal trait bound {}", reference.key.name)));
        };
        let ItemKind::Impl(implementation) = &item.kind else { unreachable!() };
        let mut dependencies = Vec::new();
        // Do not add the selected head or its equations to available evidence.
        for parameter in &implementation.generics {
            let GenericParamKind::Type(bounds) = &parameter.kind else {
                return Err(unsupported(parameter.span, "impl lifetime/const binders require their solver"));
            };
            let subject = &substitutions[&parameter.name];
            for bound in bounds {
                let bound = substituted(bound, &substitutions, budget)?;
                if let Some(witness) = self.prove_premise(subject, &bound, available, variables, budget, active, depth + 1)? { dependencies.push(witness); }
            }
        }
        for clause in &implementation.where_clause {
            budget.node(clause.span, depth)?;
            let BoundSubject::Type(subject) = &clause.subject else {
                return Err(unsupported(clause.span, "impl lifetime where premise"));
            };
            if clause.lifetime_bound.is_some() { return Err(unsupported(clause.span, "impl lifetime where premise")); }
            let subject = substituted(subject, &substitutions, budget)?;
            for bound in &clause.bounds {
                let bound = substituted(bound, &substitutions, budget)?;
                if let Some(witness) = self.prove_premise(&subject, &bound, available, variables, budget, active, depth + 1)? { dependencies.push(witness); }
            }
        }
        let (_, declaration) = self.declaration_item(reference);
        let mut inherited_substitutions = BTreeMap::new();
        budget.ty(actual)?;
        inherited_substitutions.insert("Self".into(), actual.clone());
        for (parameter, argument) in declaration.generics.iter().zip(&reference.arguments) {
            budget.text(&parameter.name, parameter.span)?;
            budget.ty(argument)?;
            inherited_substitutions.insert(parameter.name.clone(), argument.clone());
        }
        for bound in &declaration.bounds {
            let bound = substituted(bound, &inherited_substitutions, budget)?;
            if let Some(witness) = self.prove_premise(actual, &bound, available, variables, budget, active, depth + 1)? { dependencies.push(witness); }
        }
        active.remove(&goal);
        budget.ty(actual)?;
        trait_instances::copy_key(&reference.key, budget, actual.span)?;
        for argument in &reference.arguments { budget.ty(argument)?; }
        Ok(Some(Witness { item, recipe, substitutions, reference: reference.clone(), actual: actual.clone(), dependencies }))
    }

    fn prove_premise(
        &self, subject: &TypeRef, bound: &TypeRef, available: &Bounds, variables: &BTreeSet<String>,
        budget: &mut Budget<'_>, active: &mut BTreeSet<(trait_instances::Key, String)>, depth: usize,
    ) -> R<Option<Witness<'a>>> {
        budget.node(bound.span, depth)?;
        if copy::is_bound(bound) { return copy::require(subject, &self.nominals, available, variables, budget).map(|_| None); }
        if stores::is_bound(bound) { return self.store_element(subject, available, variables, budget, callable::StoreCallbacks::None).map(|_| None); }
        let reference = self.pattern_reference(bound, budget)?;
        self.solve_active(&reference, subject, available, variables, budget, active, depth)
    }

    pub(crate) fn pattern_reference(&self, application: &TypeRef, budget: &mut Budget<'_>) -> R<trait_instances::Reference> {
        let declaration = self.declaration(application, budget)?;
        let (item, _) = self.declarations[declaration.name.as_str()];
        trait_instances::arguments(item.span, declaration, application, budget)
    }
}

fn matches_head(pattern: &TypeRef, actual: &TypeRef, variables: &BTreeSet<String>, found: &mut BTreeMap<String, TypeRef>, span: Span, budget: &mut Budget<'_>) -> R<bool> {
    match unify(pattern, actual, variables, found, span, budget) {
        Ok(()) => Ok(true),
        Err(diagnostic) if diagnostic.category == zil_base::ResultCategory::ResourceLimit => Err(diagnostic),
        Err(_) => Ok(false),
    }
}

impl Index<'_> {
    pub(crate) fn coherence(&self, module: &Module, budget: &mut Budget<'_>) -> R<()> {
        if self.recipes.is_empty() { return Ok(()); }
        let mut heads = Vec::new();
        for item in &module.items {
            budget.node(item.span, 0)?;
            if let ItemKind::Impl(implementation) = &item.kind
                && let Some(application) = &implementation.trait_type
                && trait_instances::head(application).is_some_and(|name| self.declarations.contains_key(name)) {
                heads.push((item, implementation, application));
            }
        }
        for (position, (item, implementation, application)) in heads.iter().enumerate() {
            let mut constrained = BTreeSet::new();
            for parameter in &implementation.generics {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                budget.ty(&implementation.target)?;
                budget.ty(application)?;
                if !callable::refers(&implementation.target, &parameter.name) && !callable::refers(application, &parameter.name) {
                    return Err(error(parameter.span, "impl binder is not constrained by its nominal head"));
                }
                if !constrained.insert(&parameter.name) { return Err(error(parameter.span, "duplicate impl binder")); }
            }
            for (other, other_impl, other_application) in &heads[..position] {
                budget.node(item.span, 0)?;
                if trait_instances::head(application) != trait_instances::head(other_application) { continue; }
                let mut variables = BTreeSet::new();
                let mut sides = Vec::new();
                for (side, implementation, application) in [(0, *implementation, *application), (1, *other_impl, *other_application)] {
                    let mut substitutions = BTreeMap::new();
                    for (ordinal, parameter) in implementation.generics.iter().enumerate() {
                        budget.node(parameter.span, 0)?;
                        budget.text(&parameter.name, parameter.span)?;
                        let identity = format!("$impl_head_{side}_{ordinal}");
                        budget.text(&identity, parameter.span)?;
                        variables.insert(identity.clone());
                        substitutions.insert(parameter.name.clone(), named(&identity, parameter.span));
                    }
                    sides.push((substituted(&implementation.target, &substitutions, budget)?, substituted(application, &substitutions, budget)?));
                }
                let mut found = BTreeMap::new();
                if overlap_terms(&sides[0].0, &sides[1].0, &variables, &mut found, budget, 0)?
                    && overlap_terms(&sides[0].1, &sides[1].1, &variables, &mut found, budget, 0)? {
                    return Err(error(item.span, format!("overlapping nominal implementation heads (previous declaration at {})", other.span.start)));
                }
            }
        }
        Ok(())
    }
}

fn resolve<'a>(mut ty: &'a TypeRef, found: &'a BTreeMap<String, TypeRef>, budget: &mut Budget<'_>, depth: usize) -> R<&'a TypeRef> {
    loop {
        budget.node(ty.span, depth)?;
        let Some(name) = plain(ty) else { return Ok(grouped(ty)); };
        let Some(next) = found.get(name) else { return Ok(grouped(ty)); };
        ty = next;
    }
}
fn occurs(name: &str, ty: &TypeRef, found: &BTreeMap<String, TypeRef>, budget: &mut Budget<'_>, depth: usize) -> R<bool> {
    let ty = resolve(ty, found, budget, depth)?;
    budget.node(ty.span, depth)?;
    if plain(ty) == Some(name) { return Ok(true); }
    match &ty.kind {
        TypeKind::Named { args, .. } => {
            for arg in args { if let GenericArg::Type(ty) = arg && occurs(name, ty, found, budget, depth + 1)? { return Ok(true); } }
        }
        TypeKind::Tuple(types) | TypeKind::Union(types) => {
            for ty in types { if occurs(name, ty, found, budget, depth + 1)? { return Ok(true); } }
        }
        TypeKind::Reference { inner, .. } | TypeKind::Optional(inner) | TypeKind::Array { element: inner, .. } => return occurs(name, inner, found, budget, depth + 1),
        _ => {}
    }
    Ok(false)
}
fn overlap_terms(left: &TypeRef, right: &TypeRef, variables: &BTreeSet<String>, found: &mut BTreeMap<String, TypeRef>, budget: &mut Budget<'_>, depth: usize) -> R<bool> {
    let left = resolve(left, found, budget, depth)?;
    let right = resolve(right, found, budget, depth)?;
    // Make these finite original head terms independent of mutations to found.
    budget.ty(left)?;
    budget.ty(right)?;
    let (left, right) = (left.clone(), right.clone());
    for (variable, value) in [(&left, &right), (&right, &left)] {
        if let Some(name) = plain(variable).filter(|name| variables.contains(*name)) {
            if plain(value) == Some(name) { return Ok(true); }
            if occurs(name, value, found, budget, depth + 1)? { return Ok(false); }
            budget.text(name, variable.span)?;
            budget.ty(value)?;
            found.insert(name.to_owned(), value.clone());
            return Ok(true);
        }
    }
    match (&left.kind, &right.kind) {
        (TypeKind::Named { path: a, args: x }, TypeKind::Named { path: b, args: y }) if a == b && x.len() == y.len() => {
            for (a,b) in x.iter().zip(y) {
                let (GenericArg::Type(a), GenericArg::Type(b)) = (a,b) else { return Err(unsupported(left.span, "const impl head coherence")); };
                if !overlap_terms(a,b,variables,found,budget,depth+1)? { return Ok(false); }
            }
            Ok(true)
        }
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) if a.len() == b.len() => {
            for (a,b) in a.iter().zip(b) { if !overlap_terms(a,b,variables,found,budget,depth+1)? { return Ok(false); } }
            Ok(true)
        }
        (TypeKind::Reference { mutable:a, inner:x, .. }, TypeKind::Reference { mutable:b, inner:y, .. }) if a==b => overlap_terms(x,y,variables,found,budget,depth+1),
        (TypeKind::Optional(a), TypeKind::Optional(b)) => overlap_terms(a,b,variables,found,budget,depth+1),
        (TypeKind::Array { element:a, length:x }, TypeKind::Array { element:b, length:y }) if x.as_deref().map(|e| consts::length_key(e, &BTreeMap::new())).transpose()? == y.as_deref().map(|e| consts::length_key(e, &BTreeMap::new())).transpose()? => overlap_terms(a,b,variables,found,budget,depth+1),
        _ => Ok(type_key(&left,budget)? == type_key(&right,budget)?),
    }
}

impl<'a> Index<'a> {
    pub(crate) fn recipe_method(&self, actual: &TypeRef, name: &str, budget: &mut Budget<'_>, span: Span) -> R<Option<(String, Function, Witness<'a>)>> {
        let mut selected = None;
        for item in &self.recipes {
            budget.node(item.span, 0)?;
            let ItemKind::Impl(implementation) = &item.kind else { unreachable!() };
            let mut variables = BTreeSet::new();
            for parameter in &implementation.generics {
                budget.text(&parameter.name, parameter.span)?;
                variables.insert(parameter.name.clone());
            }
            let mut found = BTreeMap::new();
            if !matches_head(&implementation.target, actual, &variables, &mut found, span, budget)? { continue; }
            if implementation.generics.iter().any(|p| !found.contains_key(&p.name)) { continue; }
            let application = substituted(implementation.trait_type.as_ref().expect("trait recipe"), &found, budget)?;
            let reference = self.reference(&application, budget)?;
            let (_, declaration) = self.declaration_item(&reference);
            for member in &declaration.members {
                budget.node(member.span, 0)?;
                let ItemKind::Function(method) = &member.kind else { continue; };
                let target = trait_instances::method_name(&reference, &method.name, budget, span)?;
                if method.name != name && target != name { continue; }
                if selected.is_some() { return Err(error(span, "method is ambiguous between nominal impl recipes")); }
                let Some(witness) = self.solve(&reference, actual, &Bounds::new(), &BTreeSet::new(), budget)? else { continue; };
                let mut signature = trait_instances::signature(declaration, member, &reference, budget)?.into_owned();
                let substitutions = BTreeMap::from([("Self".into(), actual.clone())]);
                for parameter in &mut signature.params { parameter.ty = substituted(&parameter.ty, &substitutions, budget)?; }
                if let Some(result) = &mut signature.return_type { *result = substituted(result, &substitutions, budget)?; }
                selected = Some((target, signature, witness));
            }
        }
        Ok(selected)
    }
}

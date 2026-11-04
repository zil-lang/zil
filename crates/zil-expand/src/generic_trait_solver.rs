//! First-order impl recipes. A matching head is provisional until every premise
//! has been proved; only then may its source body be materialized for dispatch.
use super::*;

pub(crate) struct Witness<'a> {
    pub item: &'a Item,
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
        for item in &self.recipes {
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
            selected = Some((*item, found));
        }
        let Some((item, substitutions)) = selected else {
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
        Ok(Some(Witness { item, substitutions, reference: reference.clone(), actual: actual.clone(), dependencies }))
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

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

fn substituted(
    ty: &TypeRef,
    substitutions: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<TypeRef> {
    budget.substituted_type(ty, substitutions)?;
    let mut result = ty.clone();
    Substitution { substitutions }.ty(&mut result);
    Ok(result)
}

impl<'a> Index<'a> {
    pub(crate) fn solve(
        &self,
        reference: &trait_instances::Reference,
        actual: &TypeRef,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<Option<Witness<'a>>> {
        self.solve_active(
            reference,
            actual,
            available,
            variables,
            budget,
            &mut BTreeSet::new(),
            0,
        )
    }

    fn solve_active(
        &self,
        reference: &trait_instances::Reference,
        actual: &TypeRef,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        active: &mut BTreeSet<(trait_instances::Key, String)>,
        depth: usize,
    ) -> R<Option<Witness<'a>>> {
        budget.node(actual.span, depth)?;
        let actual_key = type_key(actual, budget)?;
        let subject = subject_key(actual, budget)?;
        if let Some(bounds) = available.get(&subject) {
            for bound in bounds {
                budget.node(actual.span, depth)?;
                if let Bound::Nominal(r) = bound {
                    trait_instances::copy_key(&r.key, budget, actual.span)?;
                    if r.key == reference.key {
                        return Ok(None);
                    }
                }
            }
        }
        if let Some(variable) = plain(actual).filter(|v| variables.contains(*v)) {
            for bound in available.get(variable).into_iter().flatten() {
                budget.node(actual.span, depth)?;
                if let Bound::Nominal(r) = bound {
                    trait_instances::copy_key(&r.key, budget, actual.span)?;
                    if r.key == reference.key {
                        return Ok(None);
                    }
                }
            }
            return Err(error(
                actual.span,
                "nominal obligation is not among the declared bounds",
            ));
        }
        if let Some(host) = plain(actual)
            && self.implementations.contains(&(
                trait_instances::copy_key(&reference.key, budget, actual.span)?,
                host,
            ))
        {
            return Ok(None);
        }
        let goal = (
            trait_instances::copy_key(&reference.key, budget, actual.span)?,
            actual_key,
        );
        if active.contains(&goal) {
            return Err(error(actual.span, "active impl obligation cycle"));
        }
        trait_instances::copy_key(&goal.0, budget, actual.span)?;
        budget.text(&goal.1, actual.span)?;
        active.insert(goal.clone());
        let mut selected = None;
        for (recipe, item) in self.recipes.iter().enumerate() {
            budget.node(item.span, depth)?;
            let ItemKind::Impl(implementation) = &item.kind else {
                unreachable!()
            };
            let application = implementation.trait_type.as_ref().expect("trait recipe");
            if trait_instances::head(application) != Some(reference.key.name.as_str()) {
                continue;
            }
            let mut variables = BTreeSet::new();
            for parameter in &implementation.generics {
                budget.node(parameter.span, depth)?;
                budget.text(&parameter.name, parameter.span)?;
                variables.insert(parameter.name.clone());
            }
            let mut found = BTreeMap::new();
            if !matches_head(
                &implementation.target,
                actual,
                &variables,
                &mut found,
                item.span,
                budget,
            )? {
                continue;
            }
            let pattern = self.pattern_reference(application, budget)?;
            if pattern.arguments.len() != reference.arguments.len() {
                continue;
            }
            let mut matched = true;
            for (pattern, actual) in pattern.arguments.iter().zip(&reference.arguments) {
                if !matches_head(pattern, actual, &variables, &mut found, item.span, budget)? {
                    matched = false;
                    break;
                }
            }
            if !matched {
                continue;
            }
            if selected.is_some() {
                return Err(error(item.span, "overlapping nominal impl recipes"));
            }
            for parameter in &implementation.generics {
                budget.node(parameter.span, depth)?;
                if !found.contains_key(&parameter.name) {
                    return Err(error(
                        parameter.span,
                        "impl binder is not determined by its nominal head",
                    ));
                }
            }
            selected = Some((recipe, *item, found));
        }
        let Some((recipe, item, mut substitutions)) = selected else {
            return Err(error(
                actual.span,
                format!(
                    "type does not satisfy nominal trait bound {}",
                    reference.key.name
                ),
            ));
        };
        let ItemKind::Impl(implementation) = &item.kind else {
            unreachable!()
        };
        if self.has_associated {
            budget.text("Self", actual.span)?;
            budget.ty(actual)?;
            substitutions.insert("Self".into(), actual.clone());
        }
        let mut dependencies = Vec::new();
        // Do not add the selected head or its equations to available evidence.
        for parameter in &implementation.generics {
            let GenericParamKind::Type(bounds) = &parameter.kind else {
                return Err(unsupported(
                    parameter.span,
                    "impl lifetime/const binders require their solver",
                ));
            };
            let subject = &substitutions[&parameter.name];
            for bound in bounds {
                let bound = substituted(bound, &substitutions, budget)?;
                if let Some(witness) = self.prove_premise(
                    subject,
                    &bound,
                    available,
                    variables,
                    budget,
                    active,
                    depth + 1,
                )? {
                    dependencies.push(witness);
                }
            }
        }
        for clause in &implementation.where_clause {
            budget.node(clause.span, depth)?;
            let BoundSubject::Type(subject) = &clause.subject else {
                return Err(unsupported(clause.span, "impl lifetime where premise"));
            };
            if clause.lifetime_bound.is_some() {
                return Err(unsupported(clause.span, "impl lifetime where premise"));
            }
            let subject = substituted(subject, &substitutions, budget)?;
            for bound in &clause.bounds {
                let bound = substituted(bound, &substitutions, budget)?;
                if let Some(witness) = self.prove_premise(
                    &subject,
                    &bound,
                    available,
                    variables,
                    budget,
                    active,
                    depth + 1,
                )? {
                    dependencies.push(witness);
                }
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
            if let Some(witness) = self.prove_premise(
                actual,
                &bound,
                available,
                variables,
                budget,
                active,
                depth + 1,
            )? {
                dependencies.push(witness);
            }
        }
        active.remove(&goal);
        budget.ty(actual)?;
        trait_instances::copy_key(&reference.key, budget, actual.span)?;
        for argument in &reference.arguments {
            budget.ty(argument)?;
        }
        Ok(Some(Witness {
            item,
            recipe,
            substitutions,
            reference: reference.clone(),
            actual: actual.clone(),
            dependencies,
        }))
    }

    fn prove_premise(
        &self,
        subject: &TypeRef,
        bound: &TypeRef,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        active: &mut BTreeSet<(trait_instances::Key, String)>,
        depth: usize,
    ) -> R<Option<Witness<'a>>> {
        budget.node(bound.span, depth)?;
        let normalized_subject;
        let normalized_bound;
        let (subject, bound) = if self.has_associated {
            normalized_subject =
                self.normalize_active(subject, available, variables, budget, active, depth + 1)?;
            normalized_bound =
                self.normalize_active(bound, available, variables, budget, active, depth + 1)?;
            (&normalized_subject, &normalized_bound)
        } else {
            (subject, bound)
        };
        if copy::is_bound(bound) {
            return copy::require(subject, &self.nominals, available, variables, budget)
                .map(|_| None);
        }
        if stores::is_bound(bound) {
            return self
                .store_element(
                    subject,
                    available,
                    variables,
                    budget,
                    callable::StoreCallbacks::None,
                )
                .map(|_| None);
        }
        let reference = self.pattern_reference(bound, budget)?;
        self.solve_active(
            &reference, subject, available, variables, budget, active, depth,
        )
    }

    pub(crate) fn pattern_reference(
        &self,
        application: &TypeRef,
        budget: &mut Budget<'_>,
    ) -> R<trait_instances::Reference> {
        let declaration = self.declaration(application, budget)?;
        let (item, _) = self.declarations[declaration.name.as_str()];
        trait_instances::arguments(item.span, declaration, application, budget)
    }
}

fn matches_head(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<bool> {
    budget.ty(pattern)?;
    budget.ty(actual)?;
    match_terms(pattern, actual, variables, found, budget, 0).map_err(|mut e| {
        e.span = Some(span);
        e
    })
}
fn match_terms(
    pattern: &TypeRef,
    actual: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<bool> {
    let (pattern, actual) = (grouped(pattern), grouped(actual));
    budget.node(pattern.span, depth)?;
    if let Some(name) = plain(pattern).filter(|name| variables.contains(*name)) {
        if let Some(prior) = found.get(name) {
            return Ok(type_key(prior, budget)? == type_key(actual, budget)?);
        }
        budget.text(name, pattern.span)?;
        budget.ty(actual)?;
        found.insert(name.to_owned(), actual.clone());
        return Ok(true);
    }
    match (&pattern.kind, &actual.kind) {
        (TypeKind::Named { path: a, args: x }, TypeKind::Named { path: b, args: y })
            if a == b && x.len() == y.len() =>
        {
            for (a, b) in x.iter().zip(y) {
                let (GenericArg::Type(a), GenericArg::Type(b)) = (a, b) else {
                    return Err(unsupported(pattern.span, "const impl head matching"));
                };
                if !match_terms(a, b, variables, found, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                if !match_terms(a, b, variables, found, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (
            TypeKind::Reference {
                mutable: a,
                inner: x,
                ..
            },
            TypeKind::Reference {
                mutable: b,
                inner: y,
                ..
            },
        ) if a == b => match_terms(x, y, variables, found, budget, depth + 1),
        (TypeKind::Optional(a), TypeKind::Optional(b)) => {
            match_terms(a, b, variables, found, budget, depth + 1)
        }
        (
            TypeKind::Array {
                element: a,
                length: x,
            },
            TypeKind::Array {
                element: b,
                length: y,
            },
        ) if x
            .as_deref()
            .map(|e| consts::length_key(e, found))
            .transpose()?
            == y.as_deref()
                .map(|e| consts::length_key(e, &BTreeMap::new()))
                .transpose()? =>
        {
            match_terms(a, b, variables, found, budget, depth + 1)
        }
        (
            TypeKind::Error {
                error: a,
                result: x,
            },
            TypeKind::Error {
                error: b,
                result: y,
            },
        ) => Ok(match_terms(a, b, variables, found, budget, depth + 1)?
            && match_terms(x, y, variables, found, budget, depth + 1)?),
        (
            TypeKind::Function {
                params: a,
                result: x,
            },
            TypeKind::Function {
                params: b,
                result: y,
            },
        ) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                if !match_terms(a, b, variables, found, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            match_terms(x, y, variables, found, budget, depth + 1)
        }
        (
            TypeKind::ForeignFunction {
                abi: a,
                params: x,
                result: r,
            },
            TypeKind::ForeignFunction {
                abi: b,
                params: y,
                result: s,
            },
        ) if a == b && x.len() == y.len() => {
            for (a, b) in x.iter().zip(y) {
                if !match_terms(a, b, variables, found, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            match_terms(r, s, variables, found, budget, depth + 1)
        }
        _ => Ok(false),
    }
}
fn head_type(ty: &TypeRef, budget: &mut Budget<'_>, depth: usize) -> R<()> {
    budget.node(ty.span, depth)?;
    match &grouped(ty).kind {
        TypeKind::Named { path, args } => {
            if path.len() != 1 {
                return Err(unsupported(
                    ty.span,
                    "projected impl head requires projection solving",
                ));
            }
            for arg in args {
                let GenericArg::Type(ty) = arg else {
                    return Err(unsupported(ty.span, "const impl head"));
                };
                head_type(ty, budget, depth + 1)?;
            }
        }
        TypeKind::Tuple(types) => {
            for ty in types {
                head_type(ty, budget, depth + 1)?;
            }
        }
        TypeKind::Reference { inner, .. }
        | TypeKind::Optional(inner)
        | TypeKind::Array { element: inner, .. } => head_type(inner, budget, depth + 1)?,
        TypeKind::Error { error, result } => {
            head_type(error, budget, depth + 1)?;
            head_type(result, budget, depth + 1)?;
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            for ty in params {
                head_type(ty, budget, depth + 1)?;
            }
            head_type(result, budget, depth + 1)?;
        }
        _ => {
            return Err(unsupported(
                ty.span,
                "impl head constructor requires its structural solver",
            ));
        }
    }
    Ok(())
}

impl Index<'_> {
    pub(crate) fn coherence(&self, module: &Module, budget: &mut Budget<'_>) -> R<()> {
        if self.recipes.is_empty() {
            return Ok(());
        }
        let mut heads = Vec::new();
        for item in &module.items {
            budget.node(item.span, 0)?;
            if let ItemKind::Impl(implementation) = &item.kind
                && let Some(application) = &implementation.trait_type
                && trait_instances::head(application)
                    .is_some_and(|name| self.declarations.contains_key(name))
            {
                head_type(&implementation.target, budget, 0)?;
                head_type(application, budget, 0)?;
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
                if !callable::refers(&implementation.target, &parameter.name)
                    && !callable::refers(application, &parameter.name)
                {
                    return Err(error(
                        parameter.span,
                        "impl binder is not constrained by its nominal head",
                    ));
                }
                if !constrained.insert(&parameter.name) {
                    return Err(error(parameter.span, "duplicate impl binder"));
                }
            }
            for (other, other_impl, other_application) in &heads[..position] {
                budget.node(item.span, 0)?;
                if trait_instances::head(application) != trait_instances::head(other_application) {
                    continue;
                }
                let mut variables = BTreeSet::new();
                let mut sides = Vec::new();
                for (side, implementation, application) in [
                    (0, *implementation, *application),
                    (1, *other_impl, *other_application),
                ] {
                    let mut substitutions = BTreeMap::new();
                    for (ordinal, parameter) in implementation.generics.iter().enumerate() {
                        budget.node(parameter.span, 0)?;
                        budget.text(&parameter.name, parameter.span)?;
                        let identity = format!("$impl_head_{side}_{ordinal}");
                        budget.text(&identity, parameter.span)?;
                        variables.insert(identity.clone());
                        substitutions
                            .insert(parameter.name.clone(), named(&identity, parameter.span));
                    }
                    sides.push((
                        substituted(&implementation.target, &substitutions, budget)?,
                        substituted(application, &substitutions, budget)?,
                    ));
                }
                let mut found = BTreeMap::new();
                if overlap_terms(&sides[0].0, &sides[1].0, &variables, &mut found, budget, 0)?
                    && overlap_terms(&sides[0].1, &sides[1].1, &variables, &mut found, budget, 0)?
                {
                    return Err(error(
                        item.span,
                        format!(
                            "overlapping nominal implementation heads (previous declaration at {})",
                            other.span.start
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn resolve<'a>(
    mut ty: &'a TypeRef,
    found: &'a BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<&'a TypeRef> {
    loop {
        budget.node(ty.span, depth)?;
        let Some(name) = plain(ty) else {
            return Ok(grouped(ty));
        };
        let Some(next) = found.get(name) else {
            return Ok(grouped(ty));
        };
        ty = next;
    }
}
fn occurs(
    name: &str,
    ty: &TypeRef,
    found: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<bool> {
    let ty = resolve(ty, found, budget, depth)?;
    budget.node(ty.span, depth)?;
    if plain(ty) == Some(name) {
        return Ok(true);
    }
    match &ty.kind {
        TypeKind::Named { args, .. } => {
            for arg in args {
                if let GenericArg::Type(ty) = arg
                    && occurs(name, ty, found, budget, depth + 1)?
                {
                    return Ok(true);
                }
            }
        }
        TypeKind::Tuple(types) | TypeKind::Union(types) => {
            for ty in types {
                if occurs(name, ty, found, budget, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        TypeKind::Reference { inner, .. }
        | TypeKind::Optional(inner)
        | TypeKind::Array { element: inner, .. } => {
            return occurs(name, inner, found, budget, depth + 1);
        }
        TypeKind::Error { error, result } => {
            return Ok(occurs(name, error, found, budget, depth + 1)?
                || occurs(name, result, found, budget, depth + 1)?);
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            for ty in params {
                if occurs(name, ty, found, budget, depth + 1)? {
                    return Ok(true);
                }
            }
            return occurs(name, result, found, budget, depth + 1);
        }
        _ => {}
    }
    Ok(false)
}
fn overlap_terms(
    left: &TypeRef,
    right: &TypeRef,
    variables: &BTreeSet<String>,
    found: &mut BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<bool> {
    let left = resolve(left, found, budget, depth)?;
    let right = resolve(right, found, budget, depth)?;
    // Make these finite original head terms independent of mutations to found.
    budget.ty(left)?;
    budget.ty(right)?;
    let (left, right) = (left.clone(), right.clone());
    for (variable, value) in [(&left, &right), (&right, &left)] {
        if let Some(name) = plain(variable).filter(|name| variables.contains(*name)) {
            if plain(value) == Some(name) {
                return Ok(true);
            }
            if occurs(name, value, found, budget, depth + 1)? {
                return Ok(false);
            }
            budget.text(name, variable.span)?;
            budget.ty(value)?;
            found.insert(name.to_owned(), value.clone());
            return Ok(true);
        }
    }
    match (&left.kind, &right.kind) {
        (TypeKind::Named { path: a, args: x }, TypeKind::Named { path: b, args: y })
            if a == b && x.len() == y.len() =>
        {
            for (a, b) in x.iter().zip(y) {
                let (GenericArg::Type(a), GenericArg::Type(b)) = (a, b) else {
                    return Err(unsupported(left.span, "const impl head coherence"));
                };
                if !overlap_terms(a, b, variables, found, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (TypeKind::Tuple(a), TypeKind::Tuple(b)) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                if !overlap_terms(a, b, variables, found, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (
            TypeKind::Reference {
                mutable: a,
                inner: x,
                ..
            },
            TypeKind::Reference {
                mutable: b,
                inner: y,
                ..
            },
        ) if a == b => overlap_terms(x, y, variables, found, budget, depth + 1),
        (TypeKind::Optional(a), TypeKind::Optional(b)) => {
            overlap_terms(a, b, variables, found, budget, depth + 1)
        }
        (
            TypeKind::Array {
                element: a,
                length: x,
            },
            TypeKind::Array {
                element: b,
                length: y,
            },
        ) if x
            .as_deref()
            .map(|e| consts::length_key(e, &BTreeMap::new()))
            .transpose()?
            == y.as_deref()
                .map(|e| consts::length_key(e, &BTreeMap::new()))
                .transpose()? =>
        {
            overlap_terms(a, b, variables, found, budget, depth + 1)
        }
        (
            TypeKind::Error {
                error: a,
                result: x,
            },
            TypeKind::Error {
                error: b,
                result: y,
            },
        ) => Ok(overlap_terms(a, b, variables, found, budget, depth + 1)?
            && overlap_terms(x, y, variables, found, budget, depth + 1)?),
        (
            TypeKind::Function {
                params: a,
                result: x,
            },
            TypeKind::Function {
                params: b,
                result: y,
            },
        ) if a.len() == b.len() => {
            for (a, b) in a.iter().zip(b) {
                if !overlap_terms(a, b, variables, found, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            overlap_terms(x, y, variables, found, budget, depth + 1)
        }
        (
            TypeKind::ForeignFunction {
                abi: a,
                params: x,
                result: r,
            },
            TypeKind::ForeignFunction {
                abi: b,
                params: y,
                result: s,
            },
        ) if a == b && x.len() == y.len() => {
            for (a, b) in x.iter().zip(y) {
                if !overlap_terms(a, b, variables, found, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            overlap_terms(r, s, variables, found, budget, depth + 1)
        }
        _ => Ok(false),
    }
}

impl<'a> Index<'a> {
    pub(crate) fn recipe_method(
        &self,
        actual: &TypeRef,
        name: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<(String, Function, Witness<'a>)>> {
        let mut selected = None;
        for item in &self.recipes {
            budget.node(item.span, 0)?;
            let ItemKind::Impl(implementation) = &item.kind else {
                unreachable!()
            };
            let mut variables = BTreeSet::new();
            for parameter in &implementation.generics {
                budget.text(&parameter.name, parameter.span)?;
                variables.insert(parameter.name.clone());
            }
            let mut found = BTreeMap::new();
            if !matches_head(
                &implementation.target,
                actual,
                &variables,
                &mut found,
                span,
                budget,
            )? {
                continue;
            }
            if implementation
                .generics
                .iter()
                .any(|p| !found.contains_key(&p.name))
            {
                continue;
            }
            let application = substituted(
                implementation.trait_type.as_ref().expect("trait recipe"),
                &found,
                budget,
            )?;
            let reference = self.reference(&application, budget)?;
            let (_, declaration) = self.declaration_item(&reference);
            for member in &declaration.members {
                budget.node(member.span, 0)?;
                let ItemKind::Function(method) = &member.kind else {
                    continue;
                };
                let target = trait_instances::method_name_with(
                    &reference,
                    &method.name,
                    !method.generics.is_empty(),
                    budget,
                    span,
                )?;
                if method.name != name && target != name {
                    continue;
                }
                if selected.is_some() {
                    return Err(error(
                        span,
                        "method is ambiguous between nominal impl recipes",
                    ));
                }
                let Some(witness) =
                    self.solve(&reference, actual, &Bounds::new(), &BTreeSet::new(), budget)?
                else {
                    continue;
                };
                if !name.starts_with("$trait_")
                    && let Some(Item {
                        kind: ItemKind::Aggregate(aggregate),
                        ..
                    }) = trait_instances::head(actual).and_then(|host| self.nominals.get(host))
                {
                    for field in &aggregate.fields {
                        budget.node(field.span, 0)?;
                        budget.text(&field.name, field.span)?;
                        if field.name == name {
                            return Err(error(
                                span,
                                "method is ambiguous between a trait recipe and a callable field",
                            ));
                        }
                    }
                    for member in &aggregate.methods {
                        budget.node(member.span, 0)?;
                        if let ItemKind::Function(function) = &member.kind {
                            budget.text(&function.name, member.span)?;
                            if function.name == name {
                                return Err(error(
                                    span,
                                    "method is ambiguous between a trait recipe and an inherent method",
                                ));
                            }
                        }
                    }
                }
                if declaration.generics.is_empty() {
                    budget.item(member)?;
                }
                let mut signature =
                    trait_instances::signature(declaration, member, &reference, budget)?
                        .into_owned();
                let substitutions = BTreeMap::from([("Self".into(), actual.clone())]);
                for parameter in &mut signature.params {
                    parameter.ty = substituted(&parameter.ty, &substitutions, budget)?;
                }
                if let Some(result) = &mut signature.return_type {
                    *result = substituted(result, &substitutions, budget)?;
                }
                selected = Some((target, signature, witness));
            }
        }
        Ok(selected)
    }
}

pub(crate) fn subject_key(subject: &TypeRef, budget: &mut Budget<'_>) -> R<String> {
    let key = type_key(subject, budget)?;
    budget.text("\0subject:", subject.span)?;
    budget.text(&key, subject.span)?;
    Ok(format!("\0subject:{key}"))
}
impl Index<'_> {
    pub(crate) fn add_subject_bounds(
        &self,
        subject: &TypeRef,
        declared: &[TypeRef],
        available: &mut Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        trait_instances::known_type(subject, variables, &self.nominals, budget, 0)?;
        let key = subject_key(subject, budget)?;
        let mut bounds = Vec::new();
        for bound in declared {
            budget.ty(bound)?;
            if copy::is_bound(bound) {
                bounds.push(Bound::Copy);
            } else if stores::is_bound(bound) {
                bounds.push(Bound::StoreElement);
            } else {
                let reference = self.pattern_reference(bound, budget)?;
                self.reference_arguments(&reference, available, variables, budget)?;
                bounds.push(Bound::Nominal(reference));
            }
        }
        let bounds = self.super_bounds_type(&bounds, budget, subject)?;
        if self.has_associated {
            for bound in &bounds {
                budget.node(subject.span, 0)?;
                if let Bound::Nominal(reference) = bound {
                    self.associated_subject(subject, reference, available, budget)?;
                }
            }
        }
        available.entry(key).or_default().extend(bounds);
        Ok(())
    }
}

impl Index<'_> {
    pub(crate) fn normalize_associated(
        &self,
        ty: &TypeRef,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<TypeRef> {
        self.normalize_active(ty, available, variables, budget, &mut BTreeSet::new(), 0)
    }
    fn normalize_active(
        &self,
        ty: &TypeRef,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        active: &mut BTreeSet<(trait_instances::Key, String)>,
        depth: usize,
    ) -> R<TypeRef> {
        budget.node(ty.span, depth)?;
        budget.ty(ty)?;
        let mut result = ty.clone();
        match &mut result.kind {
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for argument in args {
                    budget.node(ty.span, depth)?;
                    if let GenericArg::Type(t) = argument {
                        *t = self.normalize_active(
                            t,
                            available,
                            variables,
                            budget,
                            active,
                            depth + 1,
                        )?;
                    }
                }
            }
            TypeKind::Reference { inner, .. }
            | TypeKind::Optional(inner)
            | TypeKind::Group(inner)
            | TypeKind::ForLifetime { inner, .. } => {
                **inner =
                    self.normalize_active(inner, available, variables, budget, active, depth + 1)?
            }
            TypeKind::Tuple(ts) | TypeKind::Union(ts) | TypeKind::Impl(ts) => {
                for t in ts {
                    *t =
                        self.normalize_active(t, available, variables, budget, active, depth + 1)?;
                }
            }
            TypeKind::Array { element, .. } => {
                **element =
                    self.normalize_active(element, available, variables, budget, active, depth + 1)?
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for t in params {
                    *t =
                        self.normalize_active(t, available, variables, budget, active, depth + 1)?;
                }
                **result =
                    self.normalize_active(result, available, variables, budget, active, depth + 1)?;
            }
            TypeKind::Error { error, result } => {
                **error =
                    self.normalize_active(error, available, variables, budget, active, depth + 1)?;
                **result =
                    self.normalize_active(result, available, variables, budget, active, depth + 1)?;
            }
            _ => {}
        }
        let Some((member, host, application)) = associated::parts(&result) else {
            return Ok(result);
        };
        let reference = self.pattern_reference(application, budget)?;
        let (_, declaration) = self.declaration_item(&reference);
        let mut declared = None;
        for candidate in &declaration.members {
            budget.node(candidate.span, depth)?;
            if let ItemKind::TypeAlias(alias) = &candidate.kind {
                budget.text(&alias.name, candidate.span)?;
                budget.text(member, ty.span)?;
                if alias.name == member {
                    declared = Some(alias);
                    break;
                }
            }
        }
        let declared = declared.ok_or_else(|| {
            error(
                ty.span,
                "projection member is not declared by its actual trait",
            )
        })?;
        budget.text("\0projection::", ty.span)?;
        budget.text(member, ty.span)?;
        let key = (
            trait_instances::copy_key(&reference.key, budget, ty.span)?,
            format!("\0projection:{}:{member}", type_key(host, budget)?),
        );
        budget.text(&key.1, ty.span)?;
        if !active.insert(key.clone()) {
            return Err(error(ty.span, "active associated type expansion cycle"));
        }
        let witness = self.solve_active(
            &reference,
            host,
            available,
            variables,
            budget,
            active,
            depth + 1,
        )?;
        if check::dependent(host, variables) {
            // Available declared evidence permits the atom, never an impl's
            // eventual equation in a generic definition.
            active.remove(&key);
            return Ok(result);
        }
        let witness = witness.ok_or_else(|| {
            error(
                ty.span,
                "concrete projection has no committed impl equation",
            )
        })?;
        let ItemKind::Impl(implementation) = &witness.item.kind else {
            unreachable!()
        };
        let mut equation = None;
        for candidate in &implementation.members {
            budget.node(candidate.span, depth)?;
            if let ItemKind::TypeAlias(alias) = &candidate.kind {
                budget.text(&alias.name, candidate.span)?;
                budget.text(member, ty.span)?;
                if alias.name == member {
                    equation = alias.value.as_ref();
                    break;
                }
            }
        }
        let from_impl = equation.is_some();
        let equation = equation
            .or(declared.value.as_ref())
            .ok_or_else(|| error(ty.span, "impl is missing its associated type equation"))?;
        let mut substitutions = if from_impl {
            witness.substitutions
        } else {
            BTreeMap::new()
        };
        budget.text("Self", ty.span)?;
        budget.ty(host)?;
        substitutions.insert("Self".into(), host.clone());
        if !from_impl {
            for (parameter, actual) in declaration.generics.iter().zip(&reference.arguments) {
                budget.text(&parameter.name, parameter.span)?;
                budget.ty(actual)?;
                substitutions.insert(parameter.name.clone(), actual.clone());
            }
        }
        let equation = substituted(equation, &substitutions, budget)?;
        let normalized =
            self.normalize_active(&equation, available, variables, budget, active, depth + 1)?;
        active.remove(&key);
        Ok(normalized)
    }
}

impl Index<'_> {
    pub(crate) fn recipe_constant(
        &self,
        actual: &TypeRef,
        name: &str,
        available: &Bounds,
        actual_variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<Expr>> {
        let mut selected = None;
        self.store_types(
            actual,
            available,
            actual_variables,
            budget,
            callable::StoreCallbacks::None,
        )?;
        for item in &self.recipes {
            budget.node(item.span, 0)?;
            let ItemKind::Impl(implementation) = &item.kind else {
                unreachable!()
            };
            let mut variables = BTreeSet::new();
            for parameter in &implementation.generics {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                variables.insert(parameter.name.clone());
            }
            let mut found = BTreeMap::new();
            if !matches_head(
                &implementation.target,
                actual,
                &variables,
                &mut found,
                span,
                budget,
            )? {
                continue;
            }
            let mut complete = true;
            for parameter in &implementation.generics {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                complete &= found.contains_key(&parameter.name);
            }
            if !complete {
                continue;
            }
            let application = substituted(
                implementation.trait_type.as_ref().expect("trait recipe"),
                &found,
                budget,
            )?;
            let reference = self.pattern_reference(&application, budget)?;
            self.reference_arguments(&reference, available, actual_variables, budget)?;
            let (_, declaration) = self.declaration_item(&reference);
            let mut declared = false;
            for member in &declaration.members {
                budget.node(member.span, 0)?;
                if let ItemKind::Const(constant) = &member.kind {
                    budget.text(&constant.name, member.span)?;
                    budget.text(name, span)?;
                    declared |= constant.name == name;
                }
            }
            if !declared {
                continue;
            }
            self.solve(&reference, actual, available, actual_variables, budget)?;
            if selected.is_some() {
                return Err(error(
                    span,
                    "associated constant is ambiguous between nominal impl instances",
                ));
            }
            selected = Some(super::constants::expression(
                actual, &reference, name, budget, span,
            )?);
        }
        Ok(selected)
    }
}

impl Index<'_> {
    pub(crate) fn check_where(
        &self,
        clauses: &[BoundClause],
        substitutions: &BTreeMap<String, TypeRef>,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let mut active = BTreeSet::new();
        for clause in clauses {
            budget.node(clause.span, 0)?;
            if clause.lifetime_bound.is_some() {
                return Err(unsupported(
                    clause.span,
                    "generic method lifetime where relation",
                ));
            }
            let BoundSubject::Type(subject) = &clause.subject else {
                return Err(unsupported(clause.span, "generic method where subject"));
            };
            let subject = substituted(subject, substitutions, budget)?;
            for bound in &clause.bounds {
                let bound = substituted(bound, substitutions, budget)?;
                self.prove_premise(
                    &subject,
                    &bound,
                    available,
                    variables,
                    budget,
                    &mut active,
                    0,
                )?;
            }
        }
        Ok(())
    }
}

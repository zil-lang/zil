//! Definition checking and ordinary static materialization of impl recipes.
use super::*;

impl<'a> Engine<'a> {
    pub(super) fn validate_impl_recipes(&mut self) -> R<()> {
        if self.traits.recipes.is_empty() {
            return Ok(());
        }
        for (recipe, item) in self.traits.recipes.iter().enumerate() {
            self.budget.item(item)?;
            let ItemKind::Impl(implementation) = &item.kind else {
                unreachable!()
            };
            if implementation.generics.is_empty()
                && implementation.where_clause.is_empty()
                && trait_bounds::plain(&implementation.target).is_some()
                && !self.traits.has_associated
            {
                continue;
            }
            let application = implementation.trait_type.as_ref().expect("nominal recipe");
            let reference = self
                .traits
                .pattern_reference(application, &mut self.budget)?;
            let (trait_item, declaration) = self.traits.declaration_item(&reference);
            let host = trait_instances::head(&implementation.target)
                .and_then(|name| self.nominals.get(name));
            let Some(host) = host else {
                return Err(error(
                    item.span,
                    "impl Self needs an outer nominal declaration",
                ));
            };
            if trait_item.span.file != item.span.file && host.span.file != item.span.file {
                return Err(error(
                    item.span,
                    "impl requires its trait or outer Self declaration in the current module",
                ));
            }
            if implementation.receiver_default.is_some() {
                return Err(unsupported(item.span, "impl group receiver default"));
            }
            let mut parameters = implementation.generics.clone();
            for clause in &implementation.where_clause {
                self.budget.node(clause.span, 0)?;
                let BoundSubject::Type(subject) = &clause.subject else {
                    return Err(unsupported(clause.span, "impl lifetime where premise"));
                };
                let Some(subject) = trait_bounds::plain(subject) else {
                    continue;
                };
                let mut position = None;
                for (index, parameter) in parameters.iter().enumerate() {
                    self.budget.node(parameter.span, 0)?;
                    self.budget.text(subject, clause.span)?;
                    self.budget.text(&parameter.name, parameter.span)?;
                    if parameter.name == subject {
                        position = Some(index);
                        break;
                    }
                }
                let Some(position) = position else {
                    continue;
                };
                let parameter = &mut parameters[position];
                let GenericParamKind::Type(bounds) = &mut parameter.kind else {
                    return Err(unsupported(
                        clause.span,
                        "impl lifetime/const where subject",
                    ));
                };
                for bound in &clause.bounds {
                    self.budget.ty(bound)?;
                    bounds.push(bound.clone());
                }
            }
            let mut bounds = self
                .traits
                .parameter_bounds(&parameters, &mut self.budget)?;
            let mut variables = BTreeSet::new();
            for parameter in &parameters {
                self.budget.text(&parameter.name, parameter.span)?;
                variables.insert(parameter.name.clone());
            }
            for clause in &implementation.where_clause {
                if clause.lifetime_bound.is_some() {
                    return Err(unsupported(clause.span, "impl lifetime where premise"));
                }
                let BoundSubject::Type(subject) = &clause.subject else {
                    unreachable!()
                };
                if trait_bounds::plain(subject).is_some_and(|name| variables.contains(name)) {
                    continue;
                }
                self.traits.add_subject_bounds(
                    subject,
                    &clause.bounds,
                    &mut bounds,
                    &variables,
                    &mut self.budget,
                )?;
            }
            trait_instances::known_type(
                &implementation.target,
                &variables,
                &self.nominals,
                &mut self.budget,
                0,
            )?;
            self.traits.store_types(
                &implementation.target,
                &bounds,
                &variables,
                &mut self.budget,
                callable::StoreCallbacks::None,
            )?;
            self.traits
                .reference_arguments(&reference, &bounds, &variables, &mut self.budget)?;
            let equations = if self.traits.has_associated {
                self.traits.add_subject_bounds(
                    &implementation.target,
                    std::slice::from_ref(application),
                    &mut bounds,
                    &variables,
                    &mut self.budget,
                )?;
                let equations = self.traits.definition_equations(
                    implementation,
                    &reference,
                    &mut self.budget,
                )?;
                for value in equations.values() {
                    trait_instances::known_type(
                        value,
                        &variables,
                        &self.nominals,
                        &mut self.budget,
                        0,
                    )?;
                    self.traits.store_types(
                        value,
                        &bounds,
                        &variables,
                        &mut self.budget,
                        callable::StoreCallbacks::None,
                    )?;
                }
                for member in &declaration.members {
                    self.budget.node(member.span, 0)?;
                    let ItemKind::TypeAlias(alias) = &member.kind else {
                        continue;
                    };
                    let projected = trait_bounds::associated::atom(
                        &implementation.target,
                        &reference,
                        &alias.name,
                        &mut self.budget,
                        member.span,
                    )?;
                    let key = trait_bounds::associated::equation_key(&projected)?;
                    self.budget.text(&key, member.span)?;
                    let value = &equations[&key];
                    let mut substitutions = BTreeMap::new();
                    self.budget.ty(&implementation.target)?;
                    substitutions.insert("Self".into(), implementation.target.clone());
                    for (parameter, actual) in declaration.generics.iter().zip(&reference.arguments)
                    {
                        self.budget.text(&parameter.name, parameter.span)?;
                        self.budget.ty(actual)?;
                        substitutions.insert(parameter.name.clone(), actual.clone());
                    }
                    for bound in &alias.bounds {
                        self.budget.substituted_type(bound, &substitutions)?;
                        let mut bound = bound.clone();
                        Substitution {
                            substitutions: &substitutions,
                        }
                        .ty(&mut bound);
                        if copy::is_bound(&bound) {
                            copy::require(
                                value,
                                &self.nominals,
                                &bounds,
                                &variables,
                                &mut self.budget,
                            )?;
                        } else if stores::is_bound(&bound) {
                            self.traits.store_element(
                                value,
                                &bounds,
                                &variables,
                                &mut self.budget,
                                callable::StoreCallbacks::None,
                            )?;
                        } else {
                            let reference =
                                self.traits.pattern_reference(&bound, &mut self.budget)?;
                            self.traits.reference_arguments(
                                &reference,
                                &bounds,
                                &variables,
                                &mut self.budget,
                            )?;
                            self.traits.solve(
                                &reference,
                                value,
                                &bounds,
                                &variables,
                                &mut self.budget,
                            )?;
                        }
                    }
                }
                equations
            } else {
                BTreeMap::new()
            };
            let declared_arity = match &host.kind {
                ItemKind::Aggregate(a) => a.generics.len(),
                ItemKind::Enum(e) => e.generics.len(),
                _ => unreachable!(),
            };
            let TypeKind::Named { args, .. } = &grouped(&implementation.target).kind else {
                unreachable!()
            };
            if args.len() != declared_arity {
                return Err(error(
                    item.span,
                    "impl Self type argument arity differs from its declaration",
                ));
            }
            if !item.attributes.is_empty() {
                return Err(unsupported(item.span, "unexecuted generic impl attribute"));
            }
            let mut seen = BTreeSet::new();
            for parameter in &parameters {
                self.budget.node(parameter.span, 0)?;
                self.budget.text(&parameter.name, parameter.span)?;
                if !seen.insert(parameter.name.as_str()) {
                    return Err(error(parameter.span, "duplicate impl binder"));
                }
                if !matches!(parameter.kind, GenericParamKind::Type(_)) {
                    return Err(unsupported(parameter.span, "impl lifetime/const binder"));
                }
                if self.nominals.contains_key(&parameter.name) {
                    self.nominal_binder_collision = true;
                    return Ok(());
                }
            }
            let mut methods = BTreeSet::new();
            for (ordinal, member) in implementation.members.iter().enumerate() {
                self.budget.item(member)?;
                if matches!(member.kind, ItemKind::TypeAlias(_)) {
                    continue;
                }
                let ItemKind::Function(method) = &member.kind else {
                    return Err(unsupported(
                        member.span,
                        "associated impl item requires its solver",
                    ));
                };
                if !member.attributes.is_empty()
                    || method.params.iter().any(|p| !p.attributes.is_empty())
                {
                    return Err(unsupported(
                        member.span,
                        "unexecuted generic impl method attribute",
                    ));
                }
                if matches!(method.body, FunctionBody::Declaration) {
                    return Err(error(member.span, "impl method requires an ordinary body"));
                }
                if !methods.insert(method.name.as_str()) {
                    return Err(error(member.span, "duplicate impl method"));
                }
                if !method.generics.is_empty() || !method.where_clause.is_empty() {
                    return Err(unsupported(
                        member.span,
                        "generic impl method requires method specialization",
                    ));
                }
                let mut required = None;
                for candidate in &declaration.members {
                    self.budget.node(candidate.span, 0)?;
                    if let ItemKind::Function(function) = &candidate.kind {
                        self.budget.text(&function.name, candidate.span)?;
                        self.budget.text(&method.name, member.span)?;
                        if function.name == method.name {
                            required = Some(candidate);
                            break;
                        }
                    }
                }
                let required = required.ok_or_else(|| {
                    error(member.span, "impl method is not declared by its trait")
                })?;
                if declaration.generics.is_empty() {
                    self.budget.item(required)?;
                }
                let mut signature = trait_instances::signature(
                    declaration,
                    required,
                    &reference,
                    &mut self.budget,
                )?
                .into_owned();
                self.budget.text("Self", member.span)?;
                self.budget.ty(&implementation.target)?;
                let substitutions =
                    BTreeMap::from([("Self".into(), implementation.target.clone())]);
                let mut function = method.clone();
                if !function.params.first().is_some_and(|p| p.name == "self")
                    && let Some(receiver) = signature.params.first().filter(|p| p.name == "self")
                {
                    self.budget.ty(&receiver.ty)?;
                    function.params.insert(0, receiver.clone());
                }
                for parameter in &mut signature.params {
                    self.budget
                        .substituted_type(&parameter.ty, &substitutions)?;
                    Substitution {
                        substitutions: &substitutions,
                    }
                    .ty(&mut parameter.ty);
                }
                if let Some(result) = &mut signature.return_type {
                    self.budget.substituted_type(result, &substitutions)?;
                    Substitution {
                        substitutions: &substitutions,
                    }
                    .ty(result);
                }
                let mut checked = Item {
                    kind: ItemKind::Function(function),
                    attributes: vec![],
                    public: member.public,
                    span: member.span,
                };
                self.budget.substituted_item(&checked, &substitutions)?;
                substitute_item(&mut checked, &substitutions)?;
                if !equations.is_empty() {
                    self.budget.substituted_item(&checked, &equations)?;
                    substitute_item(&mut checked, &equations)?;
                    for parameter in &mut signature.params {
                        self.budget.substituted_type(&parameter.ty, &equations)?;
                        Substitution {
                            substitutions: &equations,
                        }
                        .ty(&mut parameter.ty);
                    }
                    if let Some(result) = &mut signature.return_type {
                        self.budget.substituted_type(result, &equations)?;
                        Substitution {
                            substitutions: &equations,
                        }
                        .ty(result);
                    }
                }
                let ItemKind::Function(mut function) = checked.kind else {
                    unreachable!()
                };
                if function.params.len() != signature.params.len() {
                    return Err(error(
                        member.span,
                        "impl parameter count differs from trait declaration",
                    ));
                }
                for (actual, expected) in function.params.iter().zip(&signature.params) {
                    if actual.give != expected.give
                        || type_key(&actual.ty, &mut self.budget)?
                            != type_key(&expected.ty, &mut self.budget)?
                    {
                        return Err(error(
                            member.span,
                            "impl method signature differs from trait declaration",
                        ));
                    }
                }
                let unit = check::unit(member.span);
                let actual_result = function.return_type.as_ref().unwrap_or(&unit);
                let required_result = signature.return_type.as_ref().unwrap_or(&unit);
                if type_key(actual_result, &mut self.budget)?
                    != type_key(required_result, &mut self.budget)?
                {
                    return Err(error(
                        member.span,
                        "impl result differs from trait declaration",
                    ));
                }
                // Every method receives its own declared-bound checking scope.
                // Prepay the repeated binder and bound-map copies before growth.
                for parameter in &parameters {
                    self.budget.node(parameter.span, 0)?;
                    self.budget.text(&parameter.name, parameter.span)?;
                    if let GenericParamKind::Type(bounds) = &parameter.kind {
                        for bound in bounds {
                            self.budget.ty(bound)?;
                        }
                    }
                }
                for (variable, supplied) in &bounds {
                    self.budget.text(variable, member.span)?;
                    for bound in supplied {
                        self.budget.node(member.span, 0)?;
                        if let trait_bounds::Bound::Nominal(reference) = bound {
                            trait_instances::copy_key(
                                &reference.key,
                                &mut self.budget,
                                member.span,
                            )?;
                            for argument in &reference.arguments {
                                self.budget.ty(argument)?;
                            }
                        }
                    }
                }
                function.generics = parameters.clone();
                self.budget.text(&method.name, member.span)?;
                self.budget
                    .text("$impl_definition_00000000000000000000_", member.span)?;
                function.name = format!(
                    "$impl_definition_{}_{}",
                    self.impl_definitions.len(),
                    method.name
                );
                check::validate_parametric_with_bounds(
                    &function,
                    check::ParametricTables {
                        nominals: &self.nominals,
                        enums: &self.enums,
                        signatures: &self.signatures,
                        foreign_signatures: &self.foreign_signatures,
                        pattern_types: self.pattern_types,
                        traits: &self.traits,
                    },
                    &mut helper_graph::Dependencies::default(),
                    &mut self.budget,
                    &mut self.trait_calls,
                    Some(bounds.clone()),
                )?;
                self.impl_definitions
                    .insert((recipe, ordinal), function.name);
            }
            for required in &declaration.members {
                self.budget.node(required.span, 0)?;
                if let ItemKind::Function(method) = &required.kind
                    && matches!(method.body, FunctionBody::Declaration)
                    && !methods.contains(method.name.as_str())
                {
                    return Err(error(item.span, "impl is missing a required method"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn realize_witness(&mut self, witness: trait_bounds::Witness<'a>) -> R<()> {
        for dependency in witness.dependencies {
            self.realize_witness(dependency)?;
        }
        let key = (
            trait_instances::copy_key(&witness.reference.key, &mut self.budget, witness.item.span)?,
            type_key(&witness.actual, &mut self.budget)?,
        );
        if self.realized_impls.contains(&key) {
            return Ok(());
        }
        if self.realized_impls.len() >= self.limits.max_functions {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "impl instance budget exhausted",
                Some(witness.item.span),
            ));
        }
        trait_instances::copy_key(&witness.reference.key, &mut self.budget, witness.item.span)?;
        for argument in &witness.reference.arguments {
            self.budget.ty(argument)?;
        }
        self.budget.ty(&witness.actual)?;
        self.realized_references
            .push((witness.reference.clone(), witness.actual.clone()));
        if matches!(&witness.item.kind, ItemKind::Impl(i) if i.generics.is_empty() && i.where_clause.is_empty() && trait_bounds::plain(&i.target).is_some())
        {
            self.realized_impls.insert(key);
            return Ok(());
        }
        self.budget
            .substituted_item(witness.item, &witness.substitutions)?;
        let mut instance = witness.item.clone();
        let ItemKind::Impl(implementation) = &mut instance.kind else {
            unreachable!()
        };
        Substitution {
            substitutions: &witness.substitutions,
        }
        .ty(&mut implementation.target);
        if let Some(application) = &mut implementation.trait_type {
            Substitution {
                substitutions: &witness.substitutions,
            }
            .ty(application);
        }
        let (_, declaration) = self.traits.declaration_item(&witness.reference);
        let self_substitution = BTreeMap::from([("Self".into(), witness.actual)]);
        for (ordinal, member) in implementation.members.iter_mut().enumerate() {
            if let ItemKind::Function(function) = &mut member.kind {
                if !function.params.first().is_some_and(|p| p.name == "self") {
                    for candidate in &declaration.members {
                        self.budget.node(candidate.span, 0)?;
                        if let ItemKind::Function(signature) = &candidate.kind {
                            self.budget.text(&signature.name, candidate.span)?;
                            self.budget.text(&function.name, member.span)?;
                            if signature.name == function.name {
                                let signature = trait_instances::signature(
                                    declaration,
                                    candidate,
                                    &witness.reference,
                                    &mut self.budget,
                                )?;
                                if let Some(receiver) =
                                    signature.params.first().filter(|p| p.name == "self")
                                {
                                    self.budget.ty(&receiver.ty)?;
                                    self.budget.text(&receiver.name, receiver.span)?;
                                    function.params.insert(0, receiver.clone());
                                }
                                break;
                            }
                        }
                    }
                }
            }
            self.budget.substituted_item(member, &self_substitution)?;
            substitute_item(member, &self_substitution)?;
            if let Some(definition) = self.impl_definitions.get(&(witness.recipe, ordinal))
                && let Some(calls) = self.trait_calls.get(definition)
            {
                trait_instances::rewrite_calls(
                    member,
                    calls,
                    &witness.substitutions,
                    &mut self.budget,
                )?;
            }
            substitute_item(member, &witness.substitutions)?;
        }
        implementation.generics.clear();
        implementation.where_clause.clear();
        self.realized_impls.insert(key);
        self.pending.push_back(instance);
        Ok(())
    }

    pub(super) fn realized_method(
        &mut self,
        actual: &TypeRef,
        target: &str,
        span: Span,
    ) -> R<Option<Function>> {
        let key = type_key(actual, &mut self.budget)?;
        let mut selected = None;
        for (reference, host) in &self.realized_references {
            self.budget.node(span, 0)?;
            if type_key(host, &mut self.budget)? != key {
                continue;
            }
            let (_, declaration) = self.traits.declaration_item(reference);
            for member in &declaration.members {
                self.budget.node(member.span, 0)?;
                let ItemKind::Function(function) = &member.kind else {
                    continue;
                };
                if trait_instances::method_name(reference, &function.name, &mut self.budget, span)?
                    != target
                {
                    continue;
                }
                if selected.is_some() {
                    return Err(error(
                        span,
                        "committed method target has ambiguous trait identity",
                    ));
                }
                if declaration.generics.is_empty() {
                    self.budget.item(member)?;
                }
                let mut signature =
                    trait_instances::signature(declaration, member, reference, &mut self.budget)?
                        .into_owned();
                self.budget.text("Self", span)?;
                self.budget.ty(actual)?;
                let substitutions = BTreeMap::from([("Self".into(), actual.clone())]);
                for parameter in &mut signature.params {
                    self.budget
                        .substituted_type(&parameter.ty, &substitutions)?;
                    Substitution {
                        substitutions: &substitutions,
                    }
                    .ty(&mut parameter.ty);
                }
                if let Some(result) = &mut signature.return_type {
                    self.budget.substituted_type(result, &substitutions)?;
                    Substitution {
                        substitutions: &substitutions,
                    }
                    .ty(result);
                }
                selected = Some(signature);
            }
        }
        Ok(selected)
    }

    pub(super) fn realize_bounds(
        &mut self,
        parameters: &[GenericParam],
        found: &BTreeMap<String, TypeRef>,
        span: Span,
    ) -> R<()> {
        if self.traits.recipes.is_empty() {
            return Ok(());
        }
        for parameter in parameters {
            let GenericParamKind::Type(bounds) = &parameter.kind else {
                continue;
            };
            for bound in bounds {
                if copy::is_bound(bound) || stores::is_bound(bound) {
                    continue;
                }
                self.budget.substituted_type(bound, found)?;
                let mut application = bound.clone();
                Substitution {
                    substitutions: found,
                }
                .ty(&mut application);
                let reference = self
                    .traits
                    .pattern_reference(&application, &mut self.budget)?;
                let actual = found
                    .get(&parameter.name)
                    .ok_or_else(|| error(span, "missing impl type argument"))?;
                if let Some(witness) = self.traits.solve(
                    &reference,
                    actual,
                    &trait_bounds::Bounds::new(),
                    &BTreeSet::new(),
                    &mut self.budget,
                )? {
                    self.realize_witness(witness)?;
                }
            }
        }
        Ok(())
    }
}

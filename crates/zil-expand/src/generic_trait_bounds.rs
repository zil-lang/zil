//! Finite nominal obligations for ordinary generic functions. This index is
//! immutable during specialization and never treats a same-named method as an
//! implementation. All emitted calls still enter Types and independent Verify.
use super::*;
#[path = "generic_trait_super.rs"]
mod supertraits;

pub(super) use trait_instances::Bound;
pub(super) type Bounds = BTreeMap<String, Vec<Bound>>;

pub(super) struct Index<'a> {
    declarations: BTreeMap<&'a str, (&'a Item, &'a TraitDecl)>,
    implementations: BTreeSet<(trait_instances::Key, &'a str)>,
    pub(super) has_instances: bool,
    pub(super) has_defaults: bool,
    has_supertraits: bool,
    references: BTreeMap<trait_instances::Key, trait_instances::Reference>,
    instance_count: usize,
    inherent_methods: BTreeSet<(&'a str, &'a str)>,
    nominals: BTreeMap<String, &'a Item>,
}

pub(super) fn plain(ty: &TypeRef) -> Option<&str> {
    match &grouped(ty).kind {
        TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => Some(&path[0]),
        _ => None,
    }
}

impl<'a> Index<'a> {
    pub(super) fn new(module: &'a Module, budget: &mut Budget<'_>) -> R<Self> {
        Self::build(module, budget, true)
    }
    pub(super) fn declarations_only(module: &'a Module, budget: &mut Budget<'_>) -> R<Self> {
        Self::build(module, budget, false)
    }
    fn build(module: &'a Module, budget: &mut Budget<'_>, validate: bool) -> R<Self> {
        let mut declarations = BTreeMap::new();
        let implementations = BTreeSet::new();
        let mut has_instances = false;
        let mut has_defaults = false;
        let mut has_supertraits = false;
        let mut nominals = BTreeMap::new();
        for item in &module.items {
            budget.node(item.span, 0)?;
            if matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_)) {
                nominals.insert(name(item).expect("nominal name").to_owned(), item);
            }
            match &item.kind {
                ItemKind::Impl(implementation)
                    if implementation
                        .trait_type
                        .as_ref()
                        .is_some_and(stores::is_bound) =>
                {
                    return Err(error(
                        item.span,
                        "StoreElement is a compiler shape bound and cannot be implemented",
                    ));
                }
                ItemKind::Trait(declaration) => {
                    let defaults = trait_defaults::present(declaration, budget)?;
                    has_defaults |= defaults;
                    has_supertraits |= !declaration.bounds.is_empty();
                    has_instances |= !declaration.generics.is_empty()
                        || defaults
                        || !declaration.bounds.is_empty();
                    if declarations
                        .insert(declaration.name.as_str(), (item, declaration))
                        .is_some()
                    {
                        return Err(Diagnostic::error(
                            Phase::Expand,
                            "E-TRAIT-DUPLICATE",
                            Some(item.span),
                            "duplicate nominal trait name",
                        ));
                    }
                }
                _ => {}
            }
        }
        let mut index = Self {
            declarations,
            implementations,
            nominals,
            has_instances,
            has_defaults,
            has_supertraits,
            references: BTreeMap::new(),
            instance_count: 0,
            inherent_methods: BTreeSet::new(),
        };
        if has_instances && validate {
            for (item, declaration) in index.declarations.values() {
                if !declaration.generics.is_empty() || has_defaults || has_supertraits {
                    trait_instances::validate_declaration(
                        item,
                        declaration,
                        &index.nominals,
                        budget,
                    )?;
                    let bounds = index.parameter_bounds(&declaration.generics, budget)?;
                    let variables = declaration
                        .generics
                        .iter()
                        .map(|p| p.name.clone())
                        .chain(std::iter::once("Self".to_owned()))
                        .collect();
                    for member in &declaration.members {
                        if let ItemKind::Function(method) = &member.kind {
                            for parameter in &method.params {
                                index.store_types(
                                    &parameter.ty,
                                    &bounds,
                                    &variables,
                                    budget,
                                    callable::StoreCallbacks::None,
                                )?;
                            }
                            if let Some(result) = &method.return_type {
                                index.store_types(
                                    result,
                                    &bounds,
                                    &variables,
                                    budget,
                                    callable::StoreCallbacks::None,
                                )?;
                            }
                        }
                    }
                }
            }
        }
        for item in &module.items {
            if !validate {
                continue;
            }
            if has_instances
                && let ItemKind::Impl(i) = &item.kind
                && i.trait_type.is_none()
                && let Some(host) = plain(&i.target)
            {
                for member in &i.members {
                    budget.node(member.span, 0)?;
                    if let ItemKind::Function(f) = &member.kind {
                        index.inherent_methods.insert((host, f.name.as_str()));
                    }
                }
            }
            if let ItemKind::Impl(i) = &item.kind
                && i.generics.is_empty()
                && i.where_clause.is_empty()
                && let (Some(application), Some(host)) = (&i.trait_type, plain(&i.target))
                && let Some(name) = trait_instances::head(application)
                && index.declarations.contains_key(name)
            {
                let reference = index.reference(application, budget)?;
                let key = trait_instances::copy_key(&reference.key, budget, item.span)?;
                let generic_instance = !key.arguments.is_empty();
                if index.implementations.insert((key, host)) && generic_instance {
                    index.instance_count += 1;
                }
                let key = trait_instances::copy_key(&reference.key, budget, item.span)?;
                index.references.insert(key, reference);
            }
        }
        if index.instance_count() > budget.remaining_limits().max_functions {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "trait implementation-instance budget exhausted",
                Some(module.span),
            ));
        }
        if validate && index.has_supertraits {
            index.validate_supertraits(budget)?;
        }
        Ok(index)
    }
    pub(super) fn instance_count(&self) -> usize {
        self.instance_count
    }

    pub(super) fn declaration(&self, bound: &TypeRef, budget: &mut Budget<'_>) -> R<&'a TraitDecl> {
        budget.ty(bound)?;
        let name = trait_instances::head(bound).ok_or_else(|| {
            unsupported(
                bound.span,
                "generic trait bound requires a concrete nominal trait",
            )
        })?;
        let (item, declaration) = self.declarations.get(name).ok_or_else(|| {
            error(
                bound.span,
                format!("generic bound {name} is not a declared nominal trait"),
            )
        })?;
        if !item.public && item.span.file != bound.span.file {
            return Err(Diagnostic::error(
                Phase::Expand,
                "E-MODULE-PRIVATE",
                Some(bound.span),
                "generic trait bound is private to its declaring module",
            ));
        }
        if name.starts_with("$module") {
            // The same exact prefix exclusion applies to both static impls and
            // generic bounds. Charge this path as before sharing the predicate.
            budget.text(name, bound.span)?;
        }
        if super::super::static_traits::reserved_trait(name) {
            return Err(error(
                bound.span,
                "nominal bounds cannot manufacture built-in capabilities",
            ));
        }
        for member in &declaration.members {
            budget.node(member.span, 0)?;
            let ItemKind::Function(method) = &member.kind else {
                return Err(unsupported(
                    member.span,
                    "associated trait items require the full trait solver",
                ));
            };
            if !method.generics.is_empty()
                || !method.where_clause.is_empty()
                || !method.contract.is_empty()
            {
                return Err(unsupported(
                    method.span,
                    "generic or contract-bearing trait method",
                ));
            }
        }
        Ok(declaration)
    }

    pub(super) fn reference(
        &self,
        bound: &TypeRef,
        budget: &mut Budget<'_>,
    ) -> R<trait_instances::Reference> {
        let declaration = self.declaration(bound, budget)?;
        let (item, _) = self.declarations[declaration.name.as_str()];
        let reference = trait_instances::arguments(item.span, declaration, bound, budget)?;
        if !reference.arguments.is_empty() {
            for actual in &reference.arguments {
                copy::require(
                    actual,
                    &self.nominals,
                    &Bounds::new(),
                    &BTreeSet::new(),
                    budget,
                )?;
            }
        }
        Ok(reference)
    }
    pub(super) fn bounds(&self, function: &Function, budget: &mut Budget<'_>) -> R<Bounds> {
        let callable = callable::template(function)?;
        self.parameter_bounds_except(&function.generics, budget, callable.as_ref())
    }

    pub(super) fn parameter_bounds(
        &self,
        parameters: &[GenericParam],
        budget: &mut Budget<'_>,
    ) -> R<Bounds> {
        self.parameter_bounds_except(parameters, budget, None)
    }

    fn parameter_bounds_except(
        &self,
        parameters: &[GenericParam],
        budget: &mut Budget<'_>,
        retained: Option<&callable::CallableTemplates>,
    ) -> R<Bounds> {
        let mut result = Bounds::new();
        for parameter in parameters {
            if retained.is_some_and(|roles| roles.contains_generic(&parameter.name)) {
                continue;
            }
            let GenericParamKind::Type(bounds) = &parameter.kind else {
                continue;
            };
            let mut names = Vec::new();
            for bound in bounds {
                if copy::is_bound(bound) {
                    budget.ty(bound)?;
                    if parameters.iter().any(|p| {
                        matches!(p.kind, GenericParamKind::Type(_)) && p.name == copy::BOUND
                    }) {
                        return Err(error(
                            bound.span,
                            "Copy bound resolves to a shadowing type parameter",
                        ));
                    }
                    if names.iter().any(|name| name == copy::BOUND) {
                        return Err(error(bound.span, "duplicate Copy bound"));
                    }
                    names.push(Bound::Copy);
                    continue;
                }
                if stores::is_bound(bound) {
                    budget.ty(bound)?;
                    if names.iter().any(|name| name == stores::BOUND) {
                        return Err(error(bound.span, "duplicate StoreElement bound"));
                    }
                    names.push(Bound::StoreElement);
                    continue;
                }
                let declaration = self.declaration(bound, budget)?;
                if parameters.iter().any(|p| p.name == declaration.name) {
                    return Err(error(
                        bound.span,
                        "trait bound resolves to a shadowing type parameter",
                    ));
                }
                for parameter in parameters {
                    if callable::refers(bound, &parameter.name) {
                        return Err(unsupported(
                            bound.span,
                            "trait arguments require concrete types, not lexical type binders",
                        ));
                    }
                }
                let reference = self.reference(bound, budget)?;
                if names
                    .iter()
                    .any(|b| matches!(b,Bound::Nominal(r) if r.key==reference.key))
                {
                    return Err(error(bound.span, "duplicate nominal trait bound"));
                }
                names.push(Bound::Nominal(reference));
            }
            if !names.is_empty() {
                budget.text(&parameter.name, parameter.span)?;
                let names = if self.has_supertraits {
                    self.super_bounds(&names, budget, parameter.span, Some(&parameter.name))?
                } else {
                    names
                };
                result.insert(parameter.name.clone(), names);
            }
        }
        Ok(result)
    }

    pub(super) fn check_arguments(
        &self,
        parameters: &[GenericParam],
        found: &BTreeMap<String, TypeRef>,
        (available, variables, callback): (
            &Bounds,
            &BTreeSet<String>,
            callable::StoreCallbacks<'_>,
        ),
        budget: &mut Budget<'_>,
        span: Span,
        retained: Option<&callable::CallableTemplates>,
    ) -> R<()> {
        for parameter in parameters {
            if retained.is_some_and(|roles| roles.contains_generic(&parameter.name)) {
                continue;
            }
            let GenericParamKind::Type(bounds) = &parameter.kind else {
                continue;
            };
            for bound in bounds {
                if copy::is_bound(bound) {
                    let actual = found
                        .get(&parameter.name)
                        .ok_or_else(|| error(span, "Copy argument has no unique symbolic type"))?;
                    copy::require(actual, &self.nominals, available, variables, budget)?;
                    continue;
                }
                if stores::is_bound(bound) {
                    let actual = found.get(&parameter.name).ok_or_else(|| {
                        error(span, "StoreElement argument has no unique symbolic type")
                    })?;
                    self.store_element(actual, available, variables, budget, callback)?;
                    continue;
                }
                let declaration = self.declaration(bound, budget)?;
                let reference = self.reference(bound, budget)?;
                let actual = found.get(&parameter.name).ok_or_else(|| {
                    error(span, "trait-bound argument has no unique symbolic type")
                })?;
                budget.ty(actual)?;
                let host = plain(actual).ok_or_else(|| {
                    error(span, "trait bound requires an implemented nominal type")
                })?;
                let symbolic = available.get(host).is_some_and(|bounds| {
                    bounds
                        .iter()
                        .any(|b| matches!(b,Bound::Nominal(r) if r.key==reference.key))
                });
                // A lexical type variable shadows any same-spelled nominal.
                // The global impl for that nominal cannot prove its bound.
                let satisfied = symbolic
                    || !variables.contains(host)
                        && self.implementations.contains(&(reference.key, host));
                if !satisfied {
                    return Err(error(
                        span,
                        format!(
                            "type {host} does not satisfy nominal trait bound {}",
                            declaration.name
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn method(
        &self,
        bounds: &Bounds,
        variable: &str,
        name: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Cow<'a, Function>> {
        let mut selected = None;
        let expanded;
        let supplied = bounds.get(variable).map(Vec::as_slice).unwrap_or_default();
        let supplied = if self.has_supertraits {
            expanded = self.super_bounds(supplied, budget, span, Some(variable))?;
            expanded.as_slice()
        } else {
            supplied
        };
        for bound in supplied {
            budget.node(span, 0)?;
            let Bound::Nominal(reference) = bound else {
                continue;
            };
            let (_, declaration) = self.declarations[reference.key.name.as_str()];
            for member in &declaration.members {
                budget.node(member.span, 0)?;
                if let ItemKind::Function(method) = &member.kind {
                    if self.has_defaults {
                        budget.text(&method.name, member.span)?;
                        budget.text(name, span)?;
                    }
                    if method.name != name {
                        continue;
                    }
                    if selected.is_some() {
                        return Err(error(
                            span,
                            "method is ambiguous between declared trait bounds",
                        ));
                    }
                    let mut method =
                        trait_instances::signature(declaration, member, reference, budget)?;
                    if !reference.arguments.is_empty() {
                        method.to_mut().name =
                            trait_instances::method_name(reference, name, budget, span)?;
                    }
                    selected = Some(method);
                }
            }
        }
        selected.ok_or_else(|| {
            error(
                span,
                format!("method {name} is not provided by the declared trait bounds of {variable}"),
            )
        })
    }

    pub(super) fn declaration_item(
        &self,
        reference: &trait_instances::Reference,
    ) -> (&'a Item, &'a TraitDecl) {
        self.declarations[reference.key.name.as_str()]
    }
    fn actual_reference(
        &self,
        key: &trait_instances::Key,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<trait_instances::Reference> {
        let reference = &self.references[key];
        budget.text(&key.name, span)?;
        for (argument, ty) in key.arguments.iter().zip(&reference.arguments) {
            budget.text(argument, span)?;
            budget.ty(ty)?;
        }
        Ok(reference.clone())
    }
    pub(super) fn concrete_method(
        &self,
        host: &str,
        name: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<(String, Function)>> {
        if !self.has_instances {
            return Ok(None);
        }
        let mut local_collision = false;
        if !name.starts_with("$trait_") {
            for _ in 0..self
                .inherent_methods
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
            {
                budget.node(span, 0)?;
                budget.text(host, span)?;
                budget.text(name, span)?;
            }
            local_collision = self.inherent_methods.contains(&(host, name));
            if let Some(Item {
                kind: ItemKind::Aggregate(aggregate),
                ..
            }) = self.nominals.get(host)
            {
                for field in &aggregate.fields {
                    budget.node(field.span, 0)?;
                    budget.text(&field.name, field.span)?;
                    local_collision |= field.name == name;
                }
                for member in &aggregate.methods {
                    budget.node(member.span, 0)?;
                    if let ItemKind::Function(method) = &member.kind {
                        budget.text(&method.name, member.span)?;
                        local_collision |= method.name == name;
                    }
                }
            }
        }
        let mut selected = None;
        for (key, actual_host) in &self.implementations {
            budget.node(span, 0)?;
            if *actual_host != host {
                continue;
            }
            let (_, declaration) = self.declarations[key.name.as_str()];
            let reference = self.actual_reference(key, budget, span)?;
            for member in &declaration.members {
                budget.node(member.span, 0)?;
                let ItemKind::Function(method) = &member.kind else {
                    continue;
                };
                let target = trait_instances::method_name(&reference, &method.name, budget, span)?;
                if method.name != name && target != name {
                    continue;
                }
                if local_collision {
                    return Err(error(
                        span,
                        "method call is ambiguous between a trait instance and a field or inherent method",
                    ));
                }
                if selected.is_some() {
                    return Err(error(
                        span,
                        "method call is ambiguous between trait instances",
                    ));
                }
                let signature =
                    trait_instances::signature(declaration, member, &reference, budget)?;
                if declaration.generics.is_empty() {
                    budget.item(member)?;
                }
                let mut signature = signature.into_owned();
                let substitutions = BTreeMap::from([("Self".to_owned(), named(host, span))]);
                for p in &mut signature.params {
                    budget.substituted_type(&p.ty, &substitutions)?;
                    Substitution {
                        substitutions: &substitutions,
                    }
                    .ty(&mut p.ty);
                }
                if let Some(t) = &mut signature.return_type {
                    budget.substituted_type(t, &substitutions)?;
                    Substitution {
                        substitutions: &substitutions,
                    }
                    .ty(t);
                }
                selected = Some((target, signature));
            }
        }
        Ok(selected)
    }
    pub(super) fn store_element(
        &self,
        ty: &TypeRef,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        callback: callable::StoreCallbacks<'_>,
    ) -> R<()> {
        budget.ty(ty)?;
        if let Some(variable) = plain(ty).filter(|name| variables.contains(*name)) {
            if available
                .get(variable)
                .is_some_and(|bounds| bounds.iter().any(|bound| bound == stores::BOUND))
            {
                return Ok(());
            }
            return Err(error(
                ty.span,
                "Store<T> requires a declared StoreElement bound; nominal traits do not grant element ownership",
            ));
        }
        stores::concrete_element(ty, &self.nominals, budget, (available, variables, callback))
    }

    /// Validate written Store applications even in unused templates. A plain
    /// T or user nominal trait never gains its category from an eventual call.
    pub(super) fn store_types(
        &self,
        ty: &TypeRef,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        callback: callable::StoreCallbacks<'_>,
    ) -> R<()> {
        budget.ty(ty)?;
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            budget.node(ty.span, 0)?;
            match &grouped(ty).kind {
                TypeKind::Named { path, args } => {
                    if path.as_slice() == [stores::BOUND] {
                        return Err(error(ty.span, "StoreElement is a bound, not a value type"));
                    }
                    if path.as_slice() == ["Store"] {
                        let element = stores::element(ty).ok_or_else(|| {
                            error(ty.span, "Store requires exactly one element type")
                        })?;
                        self.store_element(element, available, variables, budget, callback)?;
                    }
                    if let Some(Item {
                        kind: ItemKind::Aggregate(declaration),
                        ..
                    }) = self.nominals.get(&path.join("."))
                    {
                        if declaration.generics.len() != args.len()
                            && declaration.kind == AggregateKind::Struct
                            && declaration.generics.iter().any(|parameter| {
                                matches!(&parameter.kind, GenericParamKind::Type(bounds)
                                    if bounds.iter().any(copy::is_bound))
                            })
                        {
                            return Err(error(ty.span, "Copy record type argument arity mismatch"));
                        }
                        for (parameter, argument) in declaration.generics.iter().zip(args) {
                            if declaration.kind == AggregateKind::Struct
                                && let GenericParamKind::Type(bounds) = &parameter.kind
                                && bounds.iter().any(copy::is_bound)
                            {
                                let GenericArg::Type(actual) = argument else {
                                    return Err(unsupported(
                                        ty.span,
                                        "Copy record requires type arguments",
                                    ));
                                };
                                copy::require(
                                    actual,
                                    &self.nominals,
                                    available,
                                    variables,
                                    budget,
                                )?;
                            }
                            if let GenericParamKind::Type(bounds) = &parameter.kind
                                && bounds.iter().any(stores::is_bound)
                                && let GenericArg::Type(actual) = argument
                            {
                                self.store_element(actual, available, variables, budget, callback)?;
                            }
                        }
                    }
                    if budget.has_enum_profiles() {
                        for part in path {
                            budget.text(part, ty.span)?;
                        }
                        if let Some(Item {
                            kind: ItemKind::Enum(declaration),
                            span: declaration_span,
                            ..
                        }) = self.nominals.get(&path.join("."))
                            && budget.enum_profile(*declaration_span)
                        {
                            let mut constrained = Vec::new();
                            for parameter in &declaration.generics {
                                budget.node(parameter.span, 0)?;
                                let mut has_copy = false;
                                if let GenericParamKind::Type(bounds) = &parameter.kind {
                                    for bound in bounds {
                                        budget.ty(bound)?;
                                        has_copy |= copy::is_bound(bound);
                                    }
                                }
                                constrained.push(has_copy);
                            }
                            if constrained.iter().any(|bound| *bound)
                                && declaration.generics.len() != args.len()
                            {
                                return Err(error(
                                    ty.span,
                                    "Copy enum type argument arity mismatch",
                                ));
                            }
                            for (is_copy, argument) in constrained.into_iter().zip(args) {
                                if is_copy {
                                    let GenericArg::Type(actual) = argument else {
                                        return Err(unsupported(
                                            ty.span,
                                            "Copy enum requires type arguments",
                                        ));
                                    };
                                    copy::require(
                                        actual,
                                        &self.nominals,
                                        available,
                                        variables,
                                        budget,
                                    )?;
                                }
                            }
                        }
                    }
                    for argument in args {
                        if let GenericArg::Type(ty) = argument {
                            pending.push(ty);
                        }
                    }
                }
                TypeKind::Reference { inner, .. }
                | TypeKind::Optional(inner)
                | TypeKind::ForLifetime { inner, .. } => pending.push(inner),
                TypeKind::Tuple(types) | TypeKind::Union(types) | TypeKind::Impl(types) => {
                    pending.extend(types)
                }
                TypeKind::Array { element, .. } => pending.push(element),
                TypeKind::Error { error, result } => {
                    pending.extend([error.as_ref(), result.as_ref()])
                }
                TypeKind::Function { params, result }
                | TypeKind::ForeignFunction { params, result, .. } => {
                    pending.extend(params);
                    pending.push(result);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

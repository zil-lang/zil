//! Finite nominal obligations for ordinary generic functions. This index is
//! immutable during specialization and never treats a same-named method as an
//! implementation. All emitted calls still enter Types and independent Verify.
use super::*;

pub(super) type Bounds = BTreeMap<String, Vec<String>>;

pub(super) struct Index<'a> {
    declarations: BTreeMap<&'a str, (&'a Item, &'a TraitDecl)>,
    implementations: BTreeSet<(&'a str, &'a str)>,
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
        let mut declarations = BTreeMap::new();
        let mut implementations = BTreeSet::new();
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
                ItemKind::Impl(implementation)
                    if implementation.generics.is_empty()
                        && implementation.where_clause.is_empty() =>
                {
                    if let (Some(trait_name), Some(host)) = (
                        implementation.trait_type.as_ref().and_then(plain),
                        plain(&implementation.target),
                    ) {
                        // Complete declaration conformance is checked by the
                        // ordinary static-trait collector before any Core can
                        // reach Types. This index only answers nominal presence.
                        implementations.insert((trait_name, host));
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            declarations,
            implementations,
            nominals,
        })
    }

    fn declaration(&self, bound: &TypeRef, budget: &mut Budget<'_>) -> R<&'a TraitDecl> {
        budget.ty(bound)?;
        let name = plain(bound).ok_or_else(|| {
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
        if !declaration.generics.is_empty() || !declaration.bounds.is_empty() {
            return Err(unsupported(
                bound.span,
                "generic or supertrait bound requires the full trait solver",
            ));
        }
        if matches!(
            name,
            "Copy" | "Transfer" | "Share" | "Send" | "Sync" | "Drop" | "Fn" | "FnMut" | "FnOnce"
        ) {
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
                || !matches!(method.body, FunctionBody::Declaration)
            {
                return Err(unsupported(
                    method.span,
                    "generic, default or contract-bearing trait method",
                ));
            }
        }
        Ok(declaration)
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
                    if parameters.iter().any(|p| p.name == copy::BOUND) {
                        return Err(error(
                            bound.span,
                            "Copy bound resolves to a shadowing type parameter",
                        ));
                    }
                    if names.iter().any(|name| name == copy::BOUND) {
                        return Err(error(bound.span, "duplicate Copy bound"));
                    }
                    names.push(copy::BOUND.to_owned());
                    continue;
                }
                if stores::is_bound(bound) {
                    budget.ty(bound)?;
                    if names.iter().any(|name| name == stores::BOUND) {
                        return Err(error(bound.span, "duplicate StoreElement bound"));
                    }
                    names.push(stores::BOUND.to_owned());
                    continue;
                }
                let declaration = self.declaration(bound, budget)?;
                if parameters.iter().any(|p| p.name == declaration.name) {
                    return Err(error(
                        bound.span,
                        "trait bound resolves to a shadowing type parameter",
                    ));
                }
                budget.text(&declaration.name, bound.span)?;
                if names.contains(&declaration.name) {
                    return Err(error(bound.span, "duplicate nominal trait bound"));
                }
                names.push(declaration.name.clone());
            }
            if !names.is_empty() {
                budget.text(&parameter.name, parameter.span)?;
                result.insert(parameter.name.clone(), names);
            }
        }
        Ok(result)
    }

    pub(super) fn check_arguments(
        &self,
        parameters: &[GenericParam],
        found: &BTreeMap<String, TypeRef>,
        (available, variables, callback): (&Bounds, &BTreeSet<String>, Option<&str>),
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
                let actual = found.get(&parameter.name).ok_or_else(|| {
                    error(span, "trait-bound argument has no unique symbolic type")
                })?;
                budget.ty(actual)?;
                let host = plain(actual).ok_or_else(|| {
                    error(span, "trait bound requires an implemented nominal type")
                })?;
                let symbolic = available
                    .get(host)
                    .is_some_and(|bounds| bounds.contains(&declaration.name));
                // A lexical type variable shadows any same-spelled nominal.
                // The global impl for that nominal cannot prove its bound.
                let satisfied = symbolic
                    || !variables.contains(host)
                        && self
                            .implementations
                            .contains(&(declaration.name.as_str(), host));
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
    ) -> R<&'a Function> {
        let mut selected = None;
        for trait_name in bounds.get(variable).into_iter().flatten() {
            budget.node(span, 0)?;
            if trait_name == stores::BOUND || trait_name == copy::BOUND {
                continue;
            }
            let (_, declaration) = self.declarations[trait_name.as_str()];
            for member in &declaration.members {
                budget.node(member.span, 0)?;
                if let ItemKind::Function(method) = &member.kind
                    && method.name == name
                {
                    if selected.is_some() {
                        return Err(error(
                            span,
                            "method is ambiguous between declared trait bounds",
                        ));
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

    pub(super) fn store_element(
        &self,
        ty: &TypeRef,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
        callback: Option<&str>,
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
        callback: Option<&str>,
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
                        for (parameter, argument) in declaration.generics.iter().zip(args) {
                            if let GenericParamKind::Type(bounds) = &parameter.kind
                                && bounds.iter().any(stores::is_bound)
                                && let GenericArg::Type(actual) = argument
                            {
                                self.store_element(actual, available, variables, budget, callback)?;
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

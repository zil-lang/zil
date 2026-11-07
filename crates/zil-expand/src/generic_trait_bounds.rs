//! Finite nominal obligations for ordinary generic functions. This index is
//! immutable during specialization and never treats a same-named method as an
//! implementation. All emitted calls still enter Types and independent Verify.
use super::*;
#[path = "generic_trait_associated.rs"]
pub(super) mod associated;
#[path = "generic_trait_constants.rs"]
pub(super) mod constants;
#[path = "generic_trait_dyn.rs"]
pub(crate) mod dynamic;
#[path = "generic_trait_solver.rs"]
mod solver;
#[path = "generic_trait_super.rs"]
mod supertraits;
pub(super) use solver::{Witness, subject_key};

pub(super) use trait_instances::Bound;
pub(super) type Bounds = BTreeMap<String, Vec<Bound>>;

pub(super) struct Index<'a> {
    items: &'a [Item],
    origins: Option<&'a origins::Origins>,
    declarations: BTreeMap<&'a str, (&'a Item, &'a TraitDecl)>,
    implementations: BTreeSet<(trait_instances::Key, &'a str)>,
    pub(super) has_instances: bool,
    pub(super) has_defaults: bool,
    has_supertraits: bool,
    pub(super) has_associated: bool,
    pub(super) has_constants: bool,
    pub(super) has_generic_methods: bool,
    references: BTreeMap<trait_instances::Key, trait_instances::Reference>,
    instance_count: usize,
    inherent_methods: BTreeSet<(&'a str, &'a str)>,
    nominals: BTreeMap<String, &'a Item>,
    instantiated_copy_shapes: BTreeMap<String, Item>,
    pub(super) recipes: Vec<&'a Item>,
}

pub(super) fn plain(ty: &TypeRef) -> Option<&str> {
    match &grouped(ty).kind {
        TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => Some(&path[0]),
        _ => None,
    }
}

fn mentions_any(ty: &TypeRef, variables: &BTreeSet<&str>, budget: &mut Budget<'_>) -> R<bool> {
    use super::super::ct_selection::Visitor;
    struct Mentions<'a, 'b, 'l> {
        names: &'a BTreeSet<&'a str>,
        budget: &'b mut Budget<'l>,
        found: bool,
    }
    impl Visitor for Mentions<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if let TypeKind::Named { path, .. } = &ty.kind
                && path.len() == 1
            {
                for _ in 0..self
                    .names
                    .len()
                    .checked_ilog2()
                    .map_or(1, |n| n as usize + 2)
                {
                    self.node(ty.span, depth)?;
                    self.text(&path[0], ty.span)?;
                }
                self.found |= self.names.contains(path[0].as_str());
            }
            super::super::ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut visitor = Mentions {
        names: variables,
        budget,
        found: false,
    };
    visitor.ty(ty, 0).map_err(|mut e| e.remove(0))?;
    Ok(visitor.found)
}
fn binder_names<'a>(
    names: impl Iterator<Item = &'a str>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<BTreeSet<&'a str>> {
    let mut result = BTreeSet::new();
    for name in names {
        budget.node(span, 0)?;
        budget.text(name, span)?;
        result.insert(name);
    }
    Ok(result)
}
fn prepay_shadow(
    parameters: &[GenericParam],
    name: &str,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<()> {
    for parameter in parameters {
        budget.node(parameter.span, 0)?;
        budget.text(&parameter.name, parameter.span)?;
        budget.text(name, span)?;
    }
    Ok(())
}

impl<'a> Index<'a> {
    pub(super) fn new(module: &'a Module, budget: &mut Budget<'_>) -> R<Self> {
        Self::build(module, None, budget, true)
    }
    pub(super) fn declarations_only(module: &'a Module, budget: &mut Budget<'_>) -> R<Self> {
        Self::build(module, None, budget, false)
    }
    pub(super) fn declarations_with_origins(
        module: &'a Module,
        origins: Option<&'a origins::Origins>,
        budget: &mut Budget<'_>,
    ) -> R<Self> {
        Self::build(module, origins, budget, false)
    }
    pub(super) fn with_origins(
        module: &'a Module,
        origins: Option<&'a origins::Origins>,
        budget: &mut Budget<'_>,
    ) -> R<Self> {
        Self::build(module, origins, budget, true)
    }
    fn build(
        module: &'a Module,
        origins: Option<&'a origins::Origins>,
        budget: &mut Budget<'_>,
        validate: bool,
    ) -> R<Self> {
        budget.with_origin(budget.context.origin, |budget| {
            Self::build_inner(module, origins, budget, validate)
        })
    }
    fn build_inner(
        module: &'a Module,
        origins: Option<&'a origins::Origins>,
        budget: &mut Budget<'_>,
        validate: bool,
    ) -> R<Self> {
        let mut declarations = BTreeMap::new();
        let implementations = BTreeSet::new();
        let has_dynamic = budget.has_dynamic();
        let mut has_instances = has_dynamic;
        let mut has_defaults = false;
        let mut has_supertraits = false;
        let mut has_associated = false;
        let mut has_constants = false;
        let mut has_generic_methods = false;
        let mut nominals = BTreeMap::new();
        for item in &module.items {
            budget.node(item.span, 0)?;
            if matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_)) {
                nominals.insert(name(item).expect("nominal name").to_owned(), item);
            }
            match &item.kind {
                ItemKind::Impl(implementation)
                    if implementation.trait_type.as_ref().is_some_and(|bound| {
                        stores::is_bound(bound) || callable::closed_bound(bound)
                    }) =>
                {
                    return Err(error(
                        item.span,
                        "StoreElement is a compiler shape bound and cannot be implemented",
                    ));
                }
                ItemKind::Impl(i)
                    if i.trait_type.is_some()
                        && (!i.generics.is_empty()
                            || !i.where_clause.is_empty()
                            || plain(&i.target).is_none()) =>
                {
                    has_instances = true;
                }
                ItemKind::Trait(declaration) => {
                    let (defaults, associated, methods) =
                        trait_defaults::features_with_methods(declaration, budget)?;
                    has_generic_methods |= methods;
                    has_associated |= associated;
                    if associated {
                        for member in &declaration.members {
                            budget.node(member.span, 0)?;
                            has_constants |= matches!(member.kind, ItemKind::Const(_));
                        }
                    }
                    has_defaults |= defaults;
                    has_supertraits |= !declaration.bounds.is_empty();
                    has_instances |= !declaration.generics.is_empty()
                        || defaults
                        || associated
                        || methods
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
            items: &module.items,
            origins,
            declarations,
            implementations,
            nominals,
            instantiated_copy_shapes: BTreeMap::new(),
            has_instances,
            has_defaults,
            has_supertraits,
            has_associated,
            has_constants,
            has_generic_methods,
            references: BTreeMap::new(),
            instance_count: 0,
            inherent_methods: BTreeSet::new(),
            recipes: Vec::new(),
        };
        if has_instances && validate {
            for (item, declaration) in index.declarations.values() {
                budget.context.origin = index.origin(item, budget)?;
                if !declaration.generics.is_empty()
                    || has_defaults
                    || has_supertraits
                    || has_associated
                    || has_generic_methods
                    || has_dynamic
                {
                    trait_instances::validate_declaration(
                        item,
                        declaration,
                        &index.nominals,
                        budget,
                    )?;
                    let mut bounds = index.parameter_bounds(&declaration.generics, budget)?;
                    if has_associated {
                        let mut arguments = Vec::new();
                        for parameter in &declaration.generics {
                            budget.text(&parameter.name, parameter.span)?;
                            arguments
                                .push(GenericArg::Type(named(&parameter.name, parameter.span)));
                        }
                        budget.text(&declaration.name, item.span)?;
                        let application = TypeRef {
                            kind: TypeKind::Named {
                                path: vec![declaration.name.clone()],
                                args: arguments,
                            },
                            span: item.span,
                        };
                        let reference = index.pattern_reference(&application, budget)?;
                        budget.text("Self", item.span)?;
                        let inherited = index.inherited_bounds(
                            vec![Bound::Nominal(reference)],
                            "Self",
                            budget,
                            item.span,
                        )?;
                        bounds.insert("Self".into(), inherited);
                        index.associated_bounds(&mut bounds, budget)?;
                    }
                    let variables = declaration
                        .generics
                        .iter()
                        .map(|p| p.name.clone())
                        .chain(std::iter::once("Self".to_owned()))
                        .collect();
                    if has_associated
                        && let Some(Bound::Nominal(reference)) =
                            bounds.get("Self").and_then(|b| b.first())
                    {
                        index.validate_associated_defaults(
                            declaration,
                            reference,
                            &bounds,
                            &variables,
                            budget,
                        )?;
                    }
                    for member in &declaration.members {
                        if let ItemKind::Const(constant) = &member.kind
                            && let Some(ty) = &constant.ty
                        {
                            index.store_types(
                                ty,
                                &bounds,
                                &variables,
                                budget,
                                callable::StoreCallbacks::None,
                            )?;
                        }
                        if let ItemKind::Function(method) = &member.kind {
                            let scope =
                                if method.generics.is_empty() && method.where_clause.is_empty() {
                                    None
                                } else {
                                    Some(index.method_scope(method, &bounds, &variables, budget)?)
                                };
                            let (bounds, variables) = scope
                                .as_ref()
                                .map_or((&bounds, &variables), |(bounds, variables)| {
                                    (bounds, variables)
                                });
                            for parameter in &method.params {
                                index.store_types(
                                    &parameter.ty,
                                    bounds,
                                    variables,
                                    budget,
                                    callable::StoreCallbacks::None,
                                )?;
                            }
                            if let Some(result) = &method.return_type {
                                index.store_types(
                                    result,
                                    bounds,
                                    variables,
                                    budget,
                                    callable::StoreCallbacks::None,
                                )?;
                            }
                        }
                    }
                }
            }
        }
        for (ordinal, item) in module.items.iter().enumerate() {
            budget.context.origin = origins
                .map(|origins| origins.get(ordinal, item.span))
                .transpose()?;
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
                && i.trait_type.is_some()
                && (!i.generics.is_empty()
                    || !i.where_clause.is_empty()
                    || plain(&i.target).is_none())
            {
                budget.item(item)?;
                index.declaration(i.trait_type.as_ref().expect("nominal recipe"), budget)?;
                index.has_instances = true;
                index.recipes.push(item);
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
        if validate
            && (!index.recipes.is_empty() || has_associated || has_generic_methods || has_dynamic)
        {
            for (item, declaration) in index.declarations.values() {
                budget.context.origin = index.origin(item, budget)?;
                trait_instances::validate_declaration(item, declaration, &index.nominals, budget)?;
            }
            // In mixed concrete/generic programs every source impl remains a
            // recipe until its complete premise tree has been proved.
            index.recipes.clear();
            index.implementations.clear();
            for item in &module.items {
                budget.node(item.span, 0)?;
                if let ItemKind::Impl(i) = &item.kind
                    && let Some(application) = &i.trait_type
                    && trait_instances::head(application)
                        .is_some_and(|name| index.declarations.contains_key(name))
                {
                    index.recipes.push(item);
                }
            }
        }
        if validate {
            index.coherence(module, budget)?;
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
        if validate && !index.recipes.is_empty() {
            for item in &index.recipes {
                budget.context.origin = index.origin(item, budget)?;
                let ItemKind::Impl(i) = &item.kind else {
                    unreachable!()
                };
                if i.generics.is_empty() {
                    let reference =
                        index.reference(i.trait_type.as_ref().expect("trait recipe"), budget)?;
                    index.solve(
                        &reference,
                        &i.target,
                        &Bounds::new(),
                        &BTreeSet::new(),
                        budget,
                    )?;
                }
            }
        }
        Ok(index)
    }
    pub(super) fn instance_count(&self) -> usize {
        self.instance_count
    }
    pub(super) fn inherited_bounds(
        &self,
        bounds: Vec<Bound>,
        variable: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Vec<Bound>> {
        if self.has_supertraits {
            self.super_bounds(&bounds, budget, span, Some(variable))
        } else {
            Ok(bounds)
        }
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
        let foreign = if self.origins.is_some() {
            let owner = self
                .origin(item, budget)?
                .ok_or_else(|| error(bound.span, "trait declaration owner absent"))?;
            budget
                .context
                .origin
                .is_none_or(|caller| caller.declaration.module != owner.declaration.module)
        } else {
            item.span.file != bound.span.file
        };
        if !item.public && foreign {
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
            if matches!(member.kind, ItemKind::TypeAlias(_) | ItemKind::Const(_)) {
                continue;
            }
            let ItemKind::Function(method) = &member.kind else {
                return Err(unsupported(
                    member.span,
                    "associated trait items require the full trait solver",
                ));
            };
            if !method.contract.is_empty() {
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
        self.reference_arguments(&reference, &Bounds::new(), &BTreeSet::new(), budget)?;
        Ok(reference)
    }
    pub(super) fn reference_arguments(
        &self,
        reference: &trait_instances::Reference,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let (_, declaration) = self.declaration_item(reference);
        for (parameter, actual) in declaration.generics.iter().zip(&reference.arguments) {
            let GenericParamKind::Type(bounds) = &parameter.kind else {
                return Err(unsupported(
                    parameter.span,
                    "trait non-type argument domain",
                ));
            };
            if bounds.iter().any(copy::is_bound) || !check::dependent(actual, variables) {
                copy::require_registered(
                    actual,
                    &self.nominals,
                    Some(&self.instantiated_copy_shapes),
                    available,
                    variables,
                    budget,
                )?;
            } else {
                trait_instances::known_type(actual, variables, &self.nominals, budget, 0)?;
                self.store_types(
                    actual,
                    available,
                    variables,
                    budget,
                    callable::StoreCallbacks::None,
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn bounds(&self, function: &Function, budget: &mut Budget<'_>) -> R<Bounds> {
        let callable = callable::template(function, budget)?;
        let mut bounds =
            self.parameter_bounds_except(&function.generics, budget, callable.as_ref())?;
        if !function.where_clause.is_empty() {
            let mut variables = BTreeSet::new();
            for parameter in type_parameters(function) {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                variables.insert(parameter.name.clone());
            }
            self.where_assumptions(&function.where_clause, &mut bounds, &variables, budget)?;
        }
        Ok(bounds)
    }

    pub(crate) fn where_assumptions(
        &self,
        clauses: &[BoundClause],
        bounds: &mut Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        for clause in clauses {
            budget.node(clause.span, 0)?;
            if clause.lifetime_bound.is_some() {
                return Err(unsupported(clause.span, "method lifetime where relation"));
            }
            let BoundSubject::Type(subject) = &clause.subject else {
                return Err(unsupported(clause.span, "method where subject"));
            };
            self.add_subject_bounds(subject, &clause.bounds, bounds, variables, budget)?;
            if let Some(name) = plain(subject).filter(|name| variables.contains(*name)) {
                let key = subject_key(subject, budget)?;
                if let Some(supplied) = bounds.remove(&key) {
                    budget.text(name, subject.span)?;
                    bounds.entry(name.to_owned()).or_default().extend(supplied);
                }
            }
        }
        Ok(())
    }
    fn method_scope(
        &self,
        method: &Function,
        available: &Bounds,
        variables: &BTreeSet<String>,
        budget: &mut Budget<'_>,
    ) -> R<(Bounds, BTreeSet<String>)> {
        let mut parameters = Vec::new();
        for name in variables {
            budget.node(method.span, 0)?;
            budget.text(name, method.span)?;
            let mut declared = Vec::new();
            for bound in available.get(name).into_iter().flatten() {
                budget.node(method.span, 0)?;
                declared.push(match bound {
                    Bound::Copy => {
                        budget.text(copy::BOUND, method.span)?;
                        named(copy::BOUND, method.span)
                    }
                    Bound::StoreElement => {
                        budget.text(stores::BOUND, method.span)?;
                        named(stores::BOUND, method.span)
                    }
                    Bound::Nominal(reference) => {
                        associated::application(reference, budget, method.span)?
                    }
                });
            }
            parameters.push(GenericParam {
                name: name.clone(),
                kind: GenericParamKind::Type(declared),
                span: method.span,
            });
        }
        for parameter in &method.generics {
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
        let mut bounds = self.parameter_bounds(&parameters, budget)?;
        for (name, supplied) in available {
            if !name.starts_with('\0') {
                continue;
            }
            budget.node(method.span, 0)?;
            budget.text(name, method.span)?;
            for bound in supplied {
                budget.node(method.span, 0)?;
                if let Bound::Nominal(reference) = bound {
                    trait_instances::copy_key(&reference.key, budget, method.span)?;
                    for argument in &reference.arguments {
                        budget.ty(argument)?;
                    }
                }
            }
            bounds
                .entry(name.clone())
                .or_default()
                .extend(supplied.clone());
        }
        let mut names = BTreeSet::new();
        for name in variables {
            budget.text(name, method.span)?;
            names.insert(name.clone());
        }
        for parameter in type_parameters(method) {
            budget.node(parameter.span, 0)?;
            budget.text(&parameter.name, parameter.span)?;
            names.insert(parameter.name.clone());
        }
        self.where_assumptions(&method.where_clause, &mut bounds, &names, budget)?;
        Ok((bounds, names))
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
                    prepay_shadow(parameters, copy::BOUND, bound.span, budget)?;
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
                prepay_shadow(parameters, &declaration.name, bound.span, budget)?;
                if parameters.iter().any(|p| p.name == declaration.name) {
                    return Err(error(
                        bound.span,
                        "trait bound resolves to a shadowing type parameter",
                    ));
                }
                let names_in_scope = binder_names(
                    parameters.iter().map(|p| p.name.as_str()),
                    bound.span,
                    budget,
                )?;
                let reference = if mentions_any(bound, &names_in_scope, budget)? {
                    let mut structural = Bounds::new();
                    let mut variables = BTreeSet::new();
                    for parameter in parameters {
                        budget.node(parameter.span, 0)?;
                        budget.text(&parameter.name, parameter.span)?;
                        if let GenericParamKind::Type(bounds) = &parameter.kind {
                            variables.insert(parameter.name.clone());
                            let mut supplied = Vec::new();
                            for bound in bounds {
                                budget.ty(bound)?;
                                if copy::is_bound(bound) {
                                    supplied.push(Bound::Copy);
                                }
                                if stores::is_bound(bound) {
                                    supplied.push(Bound::StoreElement);
                                }
                            }
                            structural.insert(parameter.name.clone(), supplied);
                        }
                    }
                    let reference = self.pattern_reference(bound, budget)?;
                    self.reference_arguments(&reference, &structural, &variables, budget)?;
                    reference
                } else {
                    self.reference(bound, budget)?
                };
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
        if self.has_associated {
            self.associated_bounds(&mut result, budget)?;
        }
        Ok(result)
    }

    /// Register the actual, fully substituted physical source item without
    /// interpreting the generated spelling or changing semantic life metadata.
    pub(super) fn register_copy_shape(&mut self, item: &Item, budget: &mut Budget<'_>) -> R<()> {
        let label =
            name(item).ok_or_else(|| error(item.span, "Copy shape has no actual nominal name"))?;
        budget.text(label, item.span)?;
        budget.identity_work(
            (1 + self
                .instantiated_copy_shapes
                .len()
                .saturating_add(self.nominals.len())
                .checked_ilog2()
                .unwrap_or(0) as usize)
                .saturating_mul(label.len().saturating_add(1)),
            item.span,
        )?;
        if self.nominals.contains_key(label) || self.instantiated_copy_shapes.contains_key(label) {
            return Err(error(item.span, "actual Copy shape registration collides"));
        }
        budget.substituted_item(item, &BTreeMap::new())?;
        budget.lineage_bytes(std::mem::size_of::<(String, Item)>(), item.span)?;
        self.instantiated_copy_shapes
            .insert(label.to_owned(), item.clone());
        Ok(())
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
        self.check_arguments_scoped(
            parameters,
            found,
            (available, variables, callback),
            budget,
            span,
            retained,
            None,
        )
    }

    // Bounds, lexical source, retained callable roles and callback policy are
    // independent inputs to this scoped qualification check.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_arguments_scoped(
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
        source: Option<&Function>,
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
                    copy::require_registered_scoped(
                        actual,
                        &self.nominals,
                        Some(&self.instantiated_copy_shapes),
                        available,
                        variables,
                        budget,
                        source,
                    )?;
                    continue;
                }
                if stores::is_bound(bound) {
                    let actual = found.get(&parameter.name).ok_or_else(|| {
                        error(span, "StoreElement argument has no unique symbolic type")
                    })?;
                    self.store_element(actual, available, variables, budget, callback)?;
                    continue;
                }
                let mut instantiated;
                let names_in_scope =
                    binder_names(found.keys().map(String::as_str), bound.span, budget)?;
                let bound = if mentions_any(bound, &names_in_scope, budget)? {
                    budget.substituted_type(bound, found)?;
                    instantiated = bound.clone();
                    Substitution {
                        substitutions: found,
                    }
                    .ty(&mut instantiated);
                    &instantiated
                } else {
                    bound
                };
                let declaration = self.declaration(bound, budget)?;
                let reference = self.pattern_reference(bound, budget)?;
                self.reference_arguments(&reference, available, variables, budget)?;
                let actual = found.get(&parameter.name).ok_or_else(|| {
                    error(span, "trait-bound argument has no unique symbolic type")
                })?;
                if !self.recipes.is_empty() {
                    self.solve(&reference, actual, available, variables, budget)?;
                    continue;
                }
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
        let supplied = if self.has_supertraits && !variable.starts_with("\0subject:") {
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
                    if !reference.arguments.is_empty() || !method.generics.is_empty() {
                        let generic = !method.generics.is_empty();
                        if reference.arguments.is_empty() && generic {
                            budget.item(member)?;
                        }
                        method.to_mut().name = trait_instances::method_name_with(
                            reference, name, generic, budget, span,
                        )?;
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

    pub(super) fn method_reference(
        &self,
        bounds: &Bounds,
        variable: &str,
        member: &str,
        target: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<trait_instances::Reference> {
        let mut selected = None;
        let expanded;
        let supplied = bounds.get(variable).map(Vec::as_slice).unwrap_or_default();
        let supplied = if self.has_supertraits && !variable.starts_with("\0subject:") {
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
            if trait_instances::method_name(reference, member, budget, span)? != target
                && trait_instances::method_name_with(reference, member, true, budget, span)?
                    != target
            {
                continue;
            }
            trait_instances::copy_key(&reference.key, budget, span)?;
            for argument in &reference.arguments {
                budget.ty(argument)?;
            }
            if selected.is_some() {
                return Err(error(
                    span,
                    "method target has ambiguous declared trait identity",
                ));
            }
            selected = Some(reference.clone());
        }
        selected.ok_or_else(|| error(span, "method target lost its declared trait identity"))
    }

    pub(super) fn declaration_item(
        &self,
        reference: &trait_instances::Reference,
    ) -> (&'a Item, &'a TraitDecl) {
        self.declarations[reference.key.name.as_str()]
    }
    pub(super) fn declaration_ordinal(
        &self,
        owner: &Item,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<usize> {
        for (ordinal, item) in self.items.iter().enumerate() {
            budget.node(item.span, 0)?;
            if std::ptr::eq(item, owner) {
                return Ok(ordinal);
            }
        }
        Err(error(span, "generic method lost its declaration ordinal"))
    }
    pub(super) fn function_origin(
        &self,
        function: &Function,
        budget: &mut Budget<'_>,
    ) -> R<Option<origins::Origin>> {
        let Some(origins) = self.origins else {
            return Ok(None);
        };
        for (ordinal, item) in self.items.iter().enumerate() {
            budget.node(item.span, 0)?;
            if let ItemKind::Function(candidate) = &item.kind {
                budget.text(&candidate.name, item.span)?;
                budget.text(&function.name, function.span)?;
                if candidate.name == function.name {
                    return origins.get(ordinal, item.span).map(Some);
                }
            }
        }
        Err(error(function.span, "function declaration origin absent"))
    }
    pub(super) fn origin(
        &self,
        item: &Item,
        budget: &mut Budget<'_>,
    ) -> R<Option<origins::Origin>> {
        let Some(origins) = self.origins else {
            return Ok(None);
        };
        origins
            .get(
                self.declaration_ordinal(item, budget, item.span)?,
                item.span,
            )
            .map(Some)
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
        if available
            .first_key_value()
            .is_some_and(|(key, _)| key.starts_with("\0subject:"))
        {
            let key = subject_key(ty, budget)?;
            if available
                .get(&key)
                .is_some_and(|bounds| bounds.iter().any(|bound| bound == stores::BOUND))
            {
                return Ok(());
            }
        }
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
            if associated::parts(ty).is_some() {
                self.normalize_associated(ty, available, variables, budget)?;
                continue;
            }
            match &grouped(ty).kind {
                TypeKind::Named { path, args } => {
                    if path.len() != 1 || path.as_slice() == ["$ambiguous_associated_projection"] {
                        return Err(error(
                            ty.span,
                            "associated type requires one unambiguous declared trait bound",
                        ));
                    }
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
                            && matches!(
                                declaration.kind,
                                AggregateKind::Struct | AggregateKind::Package
                            )
                            && declaration.generics.iter().any(|parameter| {
                                matches!(&parameter.kind, GenericParamKind::Type(bounds)
                                    if bounds.iter().any(copy::is_bound))
                            })
                        {
                            return Err(error(ty.span, "Copy record type argument arity mismatch"));
                        }
                        // Check written nominal obligations in signatures as
                        // well as at constructors. An unused outer template
                        // must not hide a missing bound of its field family.
                        if declaration.generics.iter().any(|parameter| {
                            matches!(&parameter.kind, GenericParamKind::Type(bounds)
                                if bounds.iter().any(|bound| !copy::is_bound(bound) && !stores::is_bound(bound)))
                        }) {
                            if declaration.generics.len() != args.len() {
                                return Err(error(ty.span, "nominal record type argument arity mismatch"));
                            }
                            let mut found = BTreeMap::new();
                            for (parameter, argument) in declaration.generics.iter().zip(args) {
                                budget.node(parameter.span, 0)?;
                                budget.text(&parameter.name, parameter.span)?;
                                if let GenericArg::Type(actual) = argument {
                                    budget.ty(actual)?;
                                    found.insert(parameter.name.clone(), actual.clone());
                                }
                            }
                            self.check_arguments(
                                &declaration.generics,
                                &found,
                                (available, variables, callback),
                                budget,
                                ty.span,
                                None,
                            )?;
                        }
                        for (parameter, argument) in declaration.generics.iter().zip(args) {
                            if matches!(
                                declaration.kind,
                                AggregateKind::Struct | AggregateKind::Package
                            ) && let GenericParamKind::Type(bounds) = &parameter.kind
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

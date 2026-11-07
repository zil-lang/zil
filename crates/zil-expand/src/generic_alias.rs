//! Transparent aliases normalize untrusted type syntax before specialization.
//! Ordinary type parameters are substituted simultaneously after protecting
//! definition identities on project and single-module APIs alike.
use super::*;

pub(super) struct Normalized {
    pub(super) module: Module,
    pub(super) pattern_types: BTreeMap<String, TypeRef>,
    pub(super) rich_aliases: Vec<TypeRef>,
}

pub(super) fn reserved_declaration_name(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u64"
            | "usize"
            | "i32"
            | "i64"
            | "f32"
            | "f64"
            | "bool"
            | "void"
            | "c_handle"
            | "c_wstr"
    )
}

pub(super) fn normalize_with_origins<'a>(
    module: &Module,
    budget: Budget<'a>,
    origins: Option<&mut super::origins::Origins>,
) -> R<(Option<Normalized>, Budget<'a>)> {
    let (normalized, mut budget) = normalize_with_budget(module, budget)?;
    if normalized.is_some()
        && let Some(origins) = origins
    {
        origins.aliases(module, &mut budget)?;
    }
    Ok((normalized, budget))
}

pub(super) fn normalize_with_budget<'a>(
    module: &Module,
    mut budget: Budget<'a>,
) -> R<(Option<Normalized>, Budget<'a>)> {
    if !module
        .items
        .iter()
        .any(|i| matches!(i.kind, ItemKind::TypeAlias(_)))
    {
        return Ok((None, budget));
    }
    let mut names = BTreeSet::new();
    for item in &module.items {
        // Bound the borrowed input before cloning any subtree or name table.
        budget.item(item)?;
        let declaration = match &item.kind {
            ItemKind::TypeAlias(a) => Some(a.name.as_str()),
            ItemKind::Import(i) => i
                .alias
                .as_deref()
                .or_else(|| i.path.last().map(String::as_str)),
            _ => name(item),
        };
        if let Some(name) = declaration
            && (!names.insert(name) || reserved_declaration_name(name))
        {
            return Err(Diagnostic::error(
                Phase::Expand,
                "E-NAME-DUPLICATE",
                Some(item.span),
                "duplicate alias or declaration name",
            ));
        }
        if let ItemKind::TypeAlias(a) = &item.kind {
            if !item.attributes.is_empty() || !a.bounds.is_empty() {
                return Err(unsupported(item.span, "bounded or attributed type alias"));
            }
            let mut parameters = BTreeSet::new();
            for parameter in &a.generics {
                if !parameters.insert(parameter.name.as_str()) {
                    return Err(error(parameter.span, "duplicate alias type parameter"));
                }
                if super::super::byte_buffer_name(&parameter.name)
                    || super::super::owned_store_name(&parameter.name)
                {
                    return Err(super::super::primitive_name_error(
                        &parameter.name,
                        parameter.span,
                    ));
                }
                if !matches!(&parameter.kind, GenericParamKind::Type(bounds) if bounds.is_empty()) {
                    return Err(unsupported(
                        parameter.span,
                        "alias bounds, lifetime and const parameters require their semantic solver",
                    ));
                }
            }
            if a.value.is_none() {
                return Err(error(item.span, "type alias requires a target"));
            }
        }
    }
    let protected = super::super::modules::protect_alias_nominals(module, &mut budget)?;
    let bound = super::super::modules::protect_alias_binders(protected.as_ref(), &mut budget)?;
    let module = bound.as_ref();
    let mut resolver = Resolver {
        life_scope: Vec::new(),
        definitions: BTreeMap::new(),
        nominals: BTreeMap::new(),
        resolved: BTreeMap::new(),
        active: BTreeSet::new(),
        budget,
    };
    for item in &module.items {
        match &item.kind {
            ItemKind::TypeAlias(a) => {
                resolver.definitions.insert(a.name.clone(), a);
            }
            ItemKind::Aggregate(a) => {
                resolver
                    .nominals
                    .insert(a.name.clone(), a.generics.as_slice());
            }
            ItemKind::Enum(e) => {
                resolver
                    .nominals
                    .insert(e.name.clone(), e.generics.as_slice());
            }
            _ => {}
        }
    }
    // All definitions are resolved, including unused aliases and forward refs.
    let definitions = resolver.definitions.keys().cloned().collect::<Vec<_>>();
    for name in definitions {
        resolver.resolve(&name, 0)?;
    }
    let mut output = module.clone();
    output
        .items
        .retain(|i| !matches!(i.kind, ItemKind::TypeAlias(_)));
    let mut visitor = Visitor {
        resolver: &mut resolver,
        pattern_types: BTreeMap::new(),
    };
    for item in &mut output.items {
        visitor.item(item, &BTreeSet::new(), 0)?;
    }
    // Check the resulting expanded shape too, before the next pass clones it.
    for item in &output.items {
        visitor.resolver.budget.item(item)?;
    }
    let pattern_types = visitor.pattern_types;
    if resolver.budget.has_enum_profiles() {
        // Aliases disappear before Engine sees ordinary type applications.
        // Validate their normalized targets while concrete phantom arguments
        // still exist. Alias parameters remain conditional type recipes, not
        // function capabilities; each actual use is checked again downstream.
        let index = trait_bounds::Index::declarations_only(&output, &mut resolver.budget)?;
        for alias in resolver.resolved.values() {
            let mut variables = BTreeSet::new();
            let mut conditional = trait_bounds::Bounds::new();
            for name in &alias.parameters {
                resolver.budget.node(alias.target.span, 0)?;
                resolver.budget.text(name, alias.target.span)?;
                resolver.budget.text(name, alias.target.span)?;
                variables.insert(name.clone());
                conditional.insert(name.clone(), vec![trait_bounds::Bound::Copy]);
            }
            index.store_types(
                &alias.target,
                &conditional,
                &variables,
                &mut resolver.budget,
                callable::StoreCallbacks::None,
            )?;
        }
    }
    let mut rich_aliases = Vec::new();
    if resolver.budget.has_borrowed_keys() {
        resolver
            .budget
            .identity_work(resolver.resolved.len(), module.span)?;
        for alias in resolver.resolved.values() {
            if rich_pointer::key(&alias.target, &mut resolver.budget)?.is_some() {
                if !alias.parameters.is_empty() {
                    return Err(unsupported(
                        alias.target.span,
                        "rich generic aliases require their declared symbolic context",
                    ));
                }
                resolver.budget.ty(&alias.target)?;
                // Outgoing inventory movement and the later preparation visit.
                resolver.budget.identity_work(2, alias.target.span)?;
                rich_aliases.push(alias.target.clone());
            } else {
                rich_pointer::reject_unhandled(&alias.target, &mut resolver.budget, &[])?;
            }
        }
    }
    Ok((
        Some(Normalized {
            module: output,
            pattern_types,
            rich_aliases,
        }),
        resolver.budget,
    ))
}

struct Alias {
    parameters: Vec<String>,
    target: TypeRef,
}

impl Alias {
    fn forward(&self) -> Option<&str> {
        let TypeKind::Named { path, args } = &grouped(&self.target).kind else {
            return None;
        };
        if path.len() != 1 || args.len() != self.parameters.len() {
            return None;
        }
        for (arg, param) in args.iter().zip(&self.parameters) {
            if !matches!(arg,GenericArg::Type(TypeRef{kind:TypeKind::Named{path,args},..}) if path.as_slice()==[param.as_str()] && args.is_empty())
            {
                return None;
            }
        }
        Some(&path[0])
    }
}
fn qualified_key(path: &[String], budget: &mut Budget<'_>, span: Span) -> R<String> {
    budget.identity_work(path.len(), span)?;
    for part in path {
        budget.text(part, span)?;
    }
    Ok(path.join("."))
}
fn namespace_path(e: &Expr, budget: &mut Budget<'_>, depth: usize) -> R<Option<(String, String)>> {
    let mut path = vec![];
    let mut node = e;
    let mut depth = depth;
    loop {
        budget.node(node.span, depth)?;
        depth = depth.saturating_add(1);
        match &node.kind {
            ExprKind::Name(name) => {
                budget.text(name, e.span)?;
                path.push(name.as_str());
                break;
            }
            ExprKind::Member {
                object,
                member: MemberName::Name(name),
            } => {
                budget.text(name, e.span)?;
                path.push(name.as_str());
                node = object;
            }
            ExprKind::Group(inner) => node = inner,
            _ => return Ok(None),
        }
    }
    path.reverse();
    budget.text(path[0], e.span)?;
    Ok(Some((path.join("."), path[0].to_owned())))
}
struct Resolver<'s, 'l> {
    life_scope: Vec<String>,
    definitions: BTreeMap<String, &'s TypeAlias>,
    nominals: BTreeMap<String, &'s [GenericParam]>,
    resolved: BTreeMap<String, Alias>,
    active: BTreeSet<String>,
    budget: Budget<'l>,
}
impl Resolver<'_, '_> {
    fn resolve(&mut self, name: &str, depth: usize) -> R<()> {
        let definition = self.definitions[name];
        let raw = definition
            .value
            .as_ref()
            .expect("validated alias definition");
        self.budget.node(raw.span, depth)?;
        if self.resolved.contains_key(name) {
            return Ok(());
        }
        if !self.active.insert(name.to_owned()) {
            return Err(error(raw.span, "cyclic transparent type alias"));
        }
        self.budget.ty_depth(raw, depth)?;
        let mut target = raw.clone();
        let parameters = definition
            .generics
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>();
        let variables = parameters.iter().cloned().collect();
        self.target(&mut target, &variables, depth + 1)?;
        // Recheck the expanded alias while it still exists. A named element
        // may have resolved to a statically forbidden slice element shape.
        self.budget.ty_depth(&target, depth + 1)?;
        self.active.remove(name);
        self.resolved
            .insert(name.to_owned(), Alias { parameters, target });
        Ok(())
    }
    fn instantiate(
        &mut self,
        name: &str,
        args: &mut [GenericArg],
        variables: &Names,
        span: Span,
        depth: usize,
    ) -> R<TypeRef> {
        self.budget.node(span, depth)?;
        // Do not erase malformed phantom arguments. Resolve every written
        // argument before a body is copied or an unused parameter disappears.
        for argument in args.iter_mut() {
            let GenericArg::Type(ty) = argument else {
                return Err(unsupported(span, "const alias type argument"));
            };
            self.target(ty, variables, depth + 1)?;
        }
        self.resolve(name, depth + 1)?;
        let alias = &self.resolved[name];
        if alias.parameters.len() != args.len() {
            return Err(error(span, "alias type argument arity mismatch"));
        }
        let mut substitutions = BTreeMap::new();
        for (parameter, argument) in alias.parameters.iter().zip(args) {
            let GenericArg::Type(ty) = argument else {
                unreachable!("arguments checked above")
            };
            self.budget.text(parameter, span)?;
            self.budget.ty_depth(ty, depth + 1)?;
            substitutions.insert(parameter.clone(), ty.clone());
        }
        // Charge both the original clone and the complete substituted shape
        // before allocating either. Substitution must not reset nesting depth.
        self.budget.ty_depth(&alias.target, depth)?;
        self.budget
            .substituted_type_depth(&alias.target, &substitutions, depth)?;
        let mut value = alias.target.clone();
        Substitution {
            substitutions: &substitutions,
        }
        .ty(&mut value);
        self.budget.ty_depth(&value, depth)?;
        super::super::foreign::callback_type_tree(&value, variables, &mut self.budget, depth)?;
        value.span = span;
        Ok(value)
    }
    fn target(&mut self, ty: &mut TypeRef, variables: &Names, depth: usize) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &mut ty.kind {
            TypeKind::Named { path, args } if path.len() == 1 => {
                if !variables.contains(&path[0]) && self.definitions.contains_key(&path[0]) {
                    *ty = self.instantiate(&path[0], args, variables, ty.span, depth)?;
                    return Ok(());
                }
                let arity = if matches!(path[0].as_str(), "Store" | "Managed") {
                    1
                } else if variables.contains(&path[0])
                    || matches!(
                        path[0].as_str(),
                        "u8" | "u16"
                            | "u64"
                            | "usize"
                            | "f32"
                            | "f64"
                            | "i32"
                            | "i64"
                            | "bool"
                            | "void"
                            | "c_handle"
                            | "c_wstr"
                            | "ByteBuffer"
                    )
                {
                    0
                } else {
                    self.nominals
                        .get(&path[0])
                        .ok_or_else(|| error(ty.span, "unknown type in alias target"))?
                        .len()
                };
                if args.len() != arity {
                    return Err(error(ty.span, "alias target type argument arity mismatch"));
                }
                let parameters = self.nominals.get(&path[0]).copied();
                for (index, arg) in args.iter_mut().enumerate() {
                    if parameters
                        .and_then(|p| p.get(index))
                        .is_some_and(|p| matches!(p.kind, GenericParamKind::Lifetime))
                        && !matches!(arg, GenericArg::Lifetime(_))
                    {
                        return Err(error(
                            ty.span,
                            "a nominal lifetime parameter requires a scoped lifetime actual",
                        ));
                    }
                    if let Some(parameter) = parameters.and_then(|parameters| parameters.get(index))
                        && let GenericParamKind::Const(domain) = &parameter.kind
                    {
                        self.budget.ty_depth(domain, depth + 1)?;
                        let mut domain = domain.clone();
                        self.target(&mut domain, variables, depth + 1)?;
                        self.budget.text(&parameter.name, parameter.span)?;
                        let parameter = GenericParam {
                            name: parameter.name.clone(),
                            kind: GenericParamKind::Const(domain),
                            span: parameter.span,
                        };
                        match arg {
                            GenericArg::Lifetime(_) => {
                                return Err(error(
                                    ty.span,
                                    "a lifetime actual cannot satisfy a const parameter",
                                ));
                            }
                            GenericArg::Type(ty) => self.budget.ty_depth(ty, depth + 1)?,
                            GenericArg::Const(value) => {
                                self.budget.const_expression_depth(value, depth + 1)?
                            }
                        }
                        let value = consts::argument(arg, Some(&parameter), &mut self.budget)?;
                        let normalized = consts::normalize(&parameter, &value, &mut self.budget)?;
                        let expression =
                            consts::expression(&normalized).expect("normalized scalar carrier");
                        self.budget.const_expression_depth(expression, depth + 1)?;
                        *arg = GenericArg::Const(expression.clone());
                    } else {
                        match arg {
                            GenericArg::Lifetime(name) => {
                                self.budget.text(name, ty.span)?;
                                for binder in &self.life_scope {
                                    self.budget.text(binder, ty.span)?;
                                    self.budget.text(name, ty.span)?;
                                }
                                if !self.life_scope.contains(name) {
                                    return Err(error(
                                        ty.span,
                                        "undeclared nominal lifetime actual",
                                    ));
                                }
                                if !parameters
                                    .and_then(|p| p.get(index))
                                    .is_some_and(|p| matches!(p.kind, GenericParamKind::Lifetime))
                                {
                                    return Err(error(
                                        ty.span,
                                        "lifetime actual supplied for a non-lifetime parameter",
                                    ));
                                }
                                // Keep the scoped actual intact. The rich signature
                                // producer resolves the actual declaration and emits
                                // its qualified class relation before lowering.
                            }
                            GenericArg::Type(t) => self.target(t, variables, depth + 1)?,
                            GenericArg::Const(value) => {
                                self.budget.const_expression_depth(value, depth + 1)?
                            }
                        }
                    }
                }
            }
            TypeKind::Group(t) => self.target(t, variables, depth + 1)?,
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if let Some(lifetime) = lifetime {
                    for _ in 0..self.life_scope.len().saturating_add(1) {
                        self.budget.text(lifetime, ty.span)?;
                    }
                    if !self.life_scope.contains(lifetime) {
                        return Err(unsupported(ty.span, "explicit alias lifetime"));
                    }
                }
                self.target(inner, variables, depth + 1)?;
            }
            TypeKind::Tuple(ts) => {
                for t in ts {
                    self.target(t, variables, depth + 1)?;
                }
            }
            TypeKind::Union(ts) => {
                for t in ts.iter_mut() {
                    self.target(t, variables, depth + 1)?;
                }
                fn flatten<'a>(t: &'a TypeRef, out: &mut Vec<&'a TypeRef>) {
                    if let TypeKind::Union(parts) = &grouped(t).kind {
                        for part in parts {
                            flatten(part, out);
                        }
                    } else {
                        out.push(t);
                    }
                }
                // All resolved targets have just passed the bounded source walk.
                // Prepay full key construction and comparisons before allocating keys.
                for t in ts.iter() {
                    for _ in 0..ts.len().saturating_add(2) {
                        self.budget.ty(t)?;
                    }
                }
                let mut members = Vec::new();
                for t in ts.iter() {
                    flatten(t, &mut members);
                }
                let mut keys = members
                    .iter()
                    .map(|t| type_key(t, &mut self.budget))
                    .collect::<R<Vec<_>>>()?;
                let work = keys
                    .iter()
                    .fold(0usize, |n, key| n.saturating_add(key.len()))
                    .saturating_mul(keys.len().saturating_add(1));
                self.budget.identity_work(work, ty.span)?;
                keys.sort();
                if keys.len() < 2 || keys.windows(2).any(|v| v[0] == v[1]) {
                    return Err(Diagnostic::error(
                        Phase::Expand,
                        "E-ANONYMOUS-SUM",
                        Some(ty.span),
                        "anonymous sum repeats a resolved member type",
                    ));
                }
            }
            TypeKind::Array { element, length } => {
                if let Some(length) = length {
                    array_length(length)?;
                }
                self.target(element, variables, depth + 1)?;
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for t in params {
                    self.target(t, variables, depth + 1)?;
                }
                self.target(result, variables, depth + 1)?;
            }
            TypeKind::ForLifetime { lifetime, inner } => {
                self.budget.text(lifetime, ty.span)?;
                self.life_scope.push(lifetime.clone());
                let result = self.target(inner, variables, depth + 1);
                self.life_scope.pop();
                result?;
            }
            TypeKind::Error { error, result } => {
                self.target(error, variables, depth + 1)?;
                self.target(result, variables, depth + 1)?;
            }
            _ => {
                return Err(unsupported(
                    ty.span,
                    "alias target type form is not implemented",
                ));
            }
        }
        super::super::foreign::callback_type_formation(ty, Some(variables))
    }
}

struct Visitor<'a, 's, 'l> {
    resolver: &'a mut Resolver<'s, 'l>,
    // Concrete generic alias patterns retain their arguments even though the
    // source pattern AST contains only a path. Ordinary checking still follows.
    pattern_types: BTreeMap<String, TypeRef>,
}
type Names = BTreeSet<String>;
impl Visitor<'_, '_, '_> {
    fn names(&mut self, names: &Names, span: Span) -> R<Names> {
        for name in names {
            self.resolver.budget.text(name, span)?;
            self.resolver.budget.node(span, 0)?;
        }
        Ok(names.clone())
    }
    fn ty(&mut self, ty: &mut TypeRef, types: &Names, depth: usize) -> R<()> {
        self.resolver.budget.node(ty.span, depth)?;
        if let TypeKind::Named { path, args } = &mut ty.kind
            && !path.is_empty()
            && !types.contains(&path[0])
        {
            // The ordinary single-name key is already present in the AST.
            // Only a qualified path needs a newly allocated joined spelling.
            let qualified;
            let key = match path.as_slice() {
                [name] => name.as_str(),
                _ => {
                    qualified = qualified_key(path, &mut self.resolver.budget, ty.span)?;
                    qualified.as_str()
                }
            };
            if self.resolver.definitions.contains_key(key) {
                *ty = self
                    .resolver
                    .instantiate(key, args, types, ty.span, depth)?;
                return Ok(());
            }
        }
        match &mut ty.kind {
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                for a in args {
                    if let GenericArg::Type(t) = a {
                        self.ty(t, types, depth + 1)?;
                    }
                }
            }
            TypeKind::Group(t)
            | TypeKind::Optional(t)
            | TypeKind::Reference { inner: t, .. }
            | TypeKind::ForLifetime { inner: t, .. } => self.ty(t, types, depth + 1)?,
            TypeKind::Tuple(ts) | TypeKind::Union(ts) | TypeKind::Impl(ts) => {
                for t in ts {
                    self.ty(t, types, depth + 1)?;
                }
            }
            TypeKind::Array { element, .. } => self.ty(element, types, depth + 1)?,
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for t in params {
                    self.ty(t, types, depth + 1)?;
                }
                self.ty(result, types, depth + 1)?;
            }
            TypeKind::Error { error, result } => {
                self.ty(error, types, depth + 1)?;
                self.ty(result, types, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn namespace(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        self.resolver.budget.node(e.span, depth)?;
        if let ExprKind::Group(inner) = &mut e.kind {
            return self.namespace(inner, types, values, depth + 1);
        }
        let namespace = match &e.kind {
            ExprKind::TypeApply { callee, .. } => callee.as_ref(),
            _ => e,
        };
        // A known lexical value/type binder, or a bare non-alias, cannot be
        // an alias namespace. Avoid constructing two unused owned names.
        if let ExprKind::Name(root) = &namespace.kind
            && (types.contains(root)
                || values.contains(root)
                || !self.resolver.definitions.contains_key(root))
        {
            return Ok(());
        }
        let Some((name, root)) = namespace_path(namespace, &mut self.resolver.budget, depth)?
        else {
            return Ok(());
        };
        if types.contains(&root)
            || values.contains(&root)
            || !self.resolver.definitions.contains_key(&name)
        {
            return Ok(());
        }
        let arguments = match &mut e.kind {
            ExprKind::TypeApply { arguments, .. } => arguments.as_mut_slice(),
            _ => &mut [],
        };
        if arguments.is_empty()
            && let Some(alias) = self.resolver.resolved.get(&name)
            && !alias.parameters.is_empty()
        {
            self.resolver.budget.ty_depth(&alias.target, depth)?;
            if let Some(target) = alias.forward() {
                self.resolver.budget.text(target, e.span)?;
                e.kind = ExprKind::Name(target.into());
                return Ok(());
            }
        }
        let alias = self
            .resolver
            .instantiate(&name, arguments, types, e.span, depth)?;
        let TypeKind::Named { path, args } = &grouped(&alias).kind else {
            return Err(error(
                e.span,
                "only a nominal alias provides a constructor or method namespace",
            ));
        };
        // The complete expanded target was bounded before this namespace copy.
        let callee = Expr {
            span: e.span,
            kind: ExprKind::Name(path.join(".")),
        };
        e.kind = if args.is_empty() {
            callee.kind
        } else {
            ExprKind::TypeApply {
                callee: Box::new(callee),
                arguments: args.clone(),
            }
        };
        Ok(())
    }
    fn pattern(
        &mut self,
        p: &mut Pattern,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        self.resolver.budget.node(p.span, depth)?;
        match &mut p.kind {
            PatternKind::Name(n) => {
                values.insert(n.clone());
            }
            PatternKind::Tuple(ps) => {
                for p in ps {
                    self.pattern(p, types, values, depth + 1)?;
                }
            }
            PatternKind::Reference { pattern, .. } | PatternKind::ErrorArm { pattern, .. } => {
                self.pattern(pattern, types, values, depth + 1)?
            }
            PatternKind::Constructor { path, arguments } => {
                if path.len() >= 2 && !types.contains(&path[0]) && !values.contains(&path[0]) {
                    let root =
                        qualified_key(&path[..path.len() - 1], &mut self.resolver.budget, p.span)?;
                    if let Some(alias) = self.resolver.resolved.get(&root)
                        && (!alias.parameters.is_empty() || path.len() > 2)
                    {
                        self.resolver.budget.ty_depth(&alias.target, depth)?;
                        if let Some(target) = alias.forward() {
                            self.resolver.budget.text(target, p.span)?;
                            let variant = path.last().unwrap().clone();
                            *path = vec![target.into(), variant];
                        } else if path.len() > 2 {
                            return Err(unsupported(
                                p.span,
                                "qualified constructor alias requires a direct nominal family binding",
                            ));
                        }
                    }
                }
                if path.len() == 1
                    && !types.contains(&path[0])
                    && let Some(alias) = self.resolver.resolved.get(&path[0])
                {
                    if !alias.parameters.is_empty() {
                        return Err(error(
                            p.span,
                            "member pattern requires a concrete type alias",
                        ));
                    }
                    self.resolver.budget.ty_depth(&alias.target, depth)?;
                    let marker = format!("$alias_pattern_{}", self.pattern_types.len());
                    self.resolver.budget.text(&marker, p.span)?;
                    self.pattern_types
                        .insert(marker.clone(), alias.target.clone());
                    path[0] = marker;
                }
                if path.len() == 2
                    && !types.contains(&path[0])
                    && let Some(alias) = self.resolver.resolved.get(&path[0])
                {
                    if !alias.parameters.is_empty() {
                        return Err(error(
                            p.span,
                            "generic alias pattern requires a concrete alias",
                        ));
                    }
                    let alias = &alias.target;
                    self.resolver.budget.ty_depth(alias, depth)?;
                    let TypeKind::Named { path: target, args } = &grouped(alias).kind else {
                        return Err(error(
                            p.span,
                            "constructor pattern alias requires a nominal enum",
                        ));
                    };
                    if !args.is_empty() {
                        let marker = format!("$alias_pattern_{}", self.pattern_types.len());
                        self.resolver.budget.text(&marker, p.span)?;
                        self.pattern_types.insert(marker.clone(), alias.clone());
                        path[0] = marker;
                    } else {
                        path[0] = target.join(".");
                    }
                }
                for p in arguments {
                    self.pattern(p, types, values, depth + 1)?;
                }
            }
            PatternKind::RecordConstructor { path, fields } => {
                if path.len() == 1
                    && !types.contains(&path[0])
                    && let Some(alias) = self.resolver.resolved.get(&path[0])
                {
                    if !alias.parameters.is_empty() {
                        return Err(error(p.span, "record pattern requires a concrete alias"));
                    }
                    self.resolver.budget.ty_depth(&alias.target, depth)?;
                    let TypeKind::Named { path: target, .. } = &grouped(&alias.target).kind else {
                        return Err(error(
                            p.span,
                            "record pattern alias requires a nominal record",
                        ));
                    };
                    let name = target.join(".");
                    self.resolver.budget.text(&name, p.span)?;
                    path[0] = name;
                }
                if path.len() == 2
                    && !types.contains(&path[0])
                    && let Some(alias) = self.resolver.resolved.get(&path[0])
                {
                    if !alias.parameters.is_empty() {
                        return Err(error(
                            p.span,
                            "generic alias pattern requires a concrete alias",
                        ));
                    }
                    let alias = &alias.target;
                    self.resolver.budget.ty_depth(alias, depth)?;
                    let TypeKind::Named { path: target, args } = &grouped(alias).kind else {
                        return Err(error(
                            p.span,
                            "record pattern alias requires a nominal enum",
                        ));
                    };
                    if !args.is_empty() {
                        let marker = format!("$alias_pattern_{}", self.pattern_types.len());
                        self.resolver.budget.text(&marker, p.span)?;
                        self.pattern_types.insert(marker.clone(), alias.clone());
                        path[0] = marker;
                    } else {
                        path[0] = target.join(".");
                    }
                }
                for field in fields {
                    self.resolver.budget.node(field.span, depth + 1)?;
                    self.resolver.budget.text(&field.name, field.span)?;
                    self.pattern(&mut field.pattern, types, values, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn item(&mut self, item: &mut Item, outer: &Names, depth: usize) -> R<()> {
        self.resolver.budget.node(item.span, depth)?;
        let mut types = self.names(outer, item.span)?;
        let generics = match &item.kind {
            ItemKind::Impl(i) => i.generics.as_slice(),
            ItemKind::Trait(t) => t.generics.as_slice(),
            ItemKind::Function(function) | ItemKind::Extern { function, .. } => {
                function.generics.as_slice()
            }
            _ => params(item),
        };
        let life_depth = self.resolver.life_scope.len();
        for g in generics {
            if matches!(g.kind, GenericParamKind::Type(_)) {
                types.insert(g.name.clone());
            } else if matches!(g.kind, GenericParamKind::Lifetime) {
                // The alias target was checked in its own definition scope.
                // Its actual argument here belongs to this real enclosing item,
                // not an empty module scope or the target's local class0.
                self.resolver.budget.node(g.span, depth)?;
                self.resolver.budget.text(&g.name, g.span)?;
                self.resolver.life_scope.push(g.name.clone());
            }
        }
        match &mut item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => {
                for parameter in &mut f.generics {
                    match &mut parameter.kind {
                        GenericParamKind::Type(bounds) => {
                            for bound in bounds {
                                self.ty(bound, &types, depth + 1)?;
                            }
                        }
                        GenericParamKind::Const(ty) => self.ty(ty, &types, depth + 1)?,
                        GenericParamKind::Lifetime => {}
                    }
                }
                let mut values = Names::new();
                for p in &mut f.params {
                    self.ty(&mut p.ty, &types, depth + 1)?;
                    values.insert(p.name.clone());
                }
                if let Some(t) = &mut f.return_type {
                    self.ty(t, &types, depth + 1)?;
                }
                match &mut f.body {
                    FunctionBody::Block(b) => self.block(b, &types, &values, depth + 1)?,
                    FunctionBody::Expression(e) => self.expr(e, &types, &values, depth + 1)?,
                    FunctionBody::Declaration => {}
                }
            }
            ItemKind::Aggregate(a) => {
                // The receiver/type stage resolves this lexical nominal name.
                types.insert("Self".into());
                for f in &mut a.fields {
                    self.ty(&mut f.ty, &types, depth + 1)?;
                    if let Some(e) = &mut f.default {
                        self.expr(e, &types, &Names::new(), depth + 1)?;
                    }
                }
                for m in &mut a.methods {
                    self.item(m, &types, depth + 1)?;
                }
            }
            ItemKind::Enum(e) => {
                for v in &mut e.variants {
                    match &mut v.fields {
                        VariantFields::Tuple(ts) => {
                            for t in ts {
                                self.ty(t, &types, depth + 1)?;
                            }
                        }
                        VariantFields::Record(fs) => {
                            for f in fs {
                                self.ty(&mut f.ty, &types, depth + 1)?;
                            }
                        }
                        VariantFields::Unit => {}
                    }
                }
            }
            ItemKind::TypeAlias(alias) => {
                for bound in &mut alias.bounds {
                    self.ty(bound, &types, depth + 1)?;
                }
                if let Some(value) = &mut alias.value {
                    self.ty(value, &types, depth + 1)?;
                }
            }
            ItemKind::Trait(t) => {
                types.insert("Self".into());
                for m in &mut t.members {
                    self.item(m, &types, depth + 1)?;
                }
            }
            ItemKind::Impl(i) => {
                self.ty(&mut i.target, &types, depth + 1)?;
                types.insert("Self".into());
                if let Some(t) = &mut i.trait_type {
                    self.ty(t, &types, depth + 1)?;
                }
                if let Some(p) = &mut i.receiver_default {
                    self.ty(&mut p.ty, &types, depth + 1)?;
                }
                for m in &mut i.members {
                    self.item(m, &types, depth + 1)?;
                }
            }
            _ => {}
        }
        self.resolver.life_scope.truncate(life_depth);
        Ok(())
    }
    fn block(&mut self, b: &mut Block, types: &Names, values: &Names, depth: usize) -> R<()> {
        self.resolver.budget.node(b.span, depth)?;
        let mut scope = self.names(values, b.span)?;
        for s in &mut b.statements {
            self.statement(s, types, &mut scope, depth + 1)?;
        }
        Ok(())
    }
    fn condition(
        &mut self,
        c: &mut Condition,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        match c {
            Condition::Expression(e) => self.expr(e, types, values, depth)?,
            Condition::Let { pattern, value } => {
                self.expr(value, types, values, depth)?;
                self.pattern(pattern, types, values, depth)?;
            }
        }
        Ok(())
    }
    fn statement(
        &mut self,
        s: &mut Stmt,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        self.resolver.budget.node(s.span, depth)?;
        match &mut s.kind {
            StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                if let Some(t) = ty {
                    self.ty(t, types, depth + 1)?;
                }
                self.expr(value, types, values, depth + 1)?;
                self.pattern(pattern, types, values, depth + 1)?;
            }
            StmtKind::Return(Some(e)) | StmtKind::Expression(e) | StmtKind::Yield(e) => {
                self.expr(e, types, values, depth + 1)?
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, types, values, depth + 1)?;
                self.expr(value, types, values, depth + 1)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let mut success = self.names(values, s.span)?;
                self.condition(condition, types, &mut success, depth + 1)?;
                self.block(then_body, types, &success, depth + 1)?;
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b, types, values, depth + 1)?,
                    Some(ElseBranch::If(s)) => {
                        let mut scope = self.names(values, s.span)?;
                        self.statement(s, types, &mut scope, depth + 1)?;
                    }
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                let mut success = self.names(values, s.span)?;
                self.condition(condition, types, &mut success, depth + 1)?;
                self.block(body, types, &success, depth + 1)?;
            }
            StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.expr(iterable, types, values, depth + 1)?;
                let mut success = self.names(values, s.span)?;
                self.pattern(pattern, types, &mut success, depth + 1)?;
                self.block(body, types, &success, depth + 1)?;
            }
            StmtKind::Loop(b) | StmtKind::Defer(b) | StmtKind::Unsafe(b) => {
                self.block(b, types, values, depth + 1)?
            }
            StmtKind::With { name, value, body } => {
                self.expr(value, types, values, depth + 1)?;
                let mut scope = self.names(values, s.span)?;
                scope.insert(name.clone());
                self.block(body, types, &scope, depth + 1)?;
            }
            StmtKind::Match { value, arms } => {
                self.expr(value, types, values, depth + 1)?;
                for arm in arms {
                    let mut scope = self.names(values, arm.span)?;
                    self.pattern(&mut arm.pattern, types, &mut scope, depth + 1)?;
                    self.arrow(&mut arm.body, types, &scope, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn arrow(
        &mut self,
        body: &mut ArrowBody,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        match body {
            ArrowBody::Expression(e) => self.expr(e, types, values, depth),
            ArrowBody::Block(b) => self.block(b, types, values, depth),
        }
    }
    fn expr(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        self.resolver.budget.node(e.span, depth)?;
        match &mut e.kind {
            ExprKind::Ascribed { ty, value } => {
                self.ty(ty, types, depth + 1)?;
                self.expr(value, types, values, depth + 1)?;
            }
            ExprKind::Group(x)
            | ExprKind::Try(x)
            | ExprKind::MetaEval(x)
            | ExprKind::Unary { operand: x, .. } => self.expr(x, types, values, depth + 1)?,
            ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
                for x in xs {
                    self.expr(x, types, values, depth + 1)?;
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left, types, values, depth + 1)?;
                self.expr(right, types, values, depth + 1)?;
            }
            ExprKind::Pipeline { input, target } => {
                self.expr(input, types, values, depth + 1)?;
                self.expr(target, types, values, depth + 1)?;
            }
            ExprKind::Member { object, member } => {
                if let MemberName::Constrained { owner, .. } = member {
                    self.ty(owner, types, depth + 1)?;
                }
                self.namespace(object, types, values, depth + 1)?;
                self.expr(object, types, values, depth + 1)?;
            }
            ExprKind::TypeApply { callee, arguments } => {
                let binding = namespace_path(callee, &mut self.resolver.budget, depth + 1)?;
                if binding.is_some_and(|(name, root)| {
                    self.resolver.definitions.contains_key(&name)
                        && !types.contains(&root)
                        && !values.contains(&root)
                }) {
                    self.namespace(e, types, values, depth + 1)?;
                } else {
                    self.expr(callee, types, values, depth + 1)?;
                    for arg in arguments {
                        match arg {
                            GenericArg::Lifetime(name) => {
                                self.resolver.budget.text(name, e.span)?
                            }
                            GenericArg::Type(t) => self.ty(t, types, depth + 1)?,
                            GenericArg::Const(x) => self.expr(x, types, values, depth + 1)?,
                        }
                    }
                }
            }
            ExprKind::Call { callee, arguments } => {
                self.expr(callee, types, values, depth + 1)?;
                for arg in arguments {
                    self.expr(&mut arg.value, types, values, depth + 1)?;
                }
            }
            ExprKind::Index { object, index } => {
                self.expr(object, types, values, depth + 1)?;
                self.expr(index, types, values, depth + 1)?;
            }
            ExprKind::Slice { object, start, end } => {
                self.expr(object, types, values, depth + 1)?;
                for x in [start, end].into_iter().flatten() {
                    self.expr(x, types, values, depth + 1)?;
                }
            }
            ExprKind::Record { ty, base, fields } => {
                if let Some(t) = ty {
                    self.namespace(t, types, values, depth + 1)?;
                    self.expr(t, types, values, depth + 1)?;
                }
                if let Some(b) = base {
                    self.expr(b, types, values, depth + 1)?;
                }
                for f in fields {
                    if let Some(x) = &mut f.value {
                        self.expr(x, types, values, depth + 1)?;
                    }
                }
            }
            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                let mut scope = self.names(values, e.span)?;
                for p in params {
                    if let Some(t) = &mut p.ty {
                        self.ty(t, types, depth + 1)?;
                    }
                    scope.insert(p.name.clone());
                }
                if let Some(t) = return_type {
                    self.ty(t, types, depth + 1)?;
                }
                self.arrow(body, types, &scope, depth + 1)?;
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, types, values, depth + 1)?;
                self.expr(then_value, types, values, depth + 1)?;
                self.expr(else_value, types, values, depth + 1)?;
            }
            ExprKind::MetaBlock(b) => self.block(b, types, values, depth + 1)?,
            _ => {}
        }
        Ok(())
    }
}

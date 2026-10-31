//! Transparent aliases normalize untrusted type syntax before specialization.
//! Ordinary type parameters are substituted simultaneously after protecting
//! definition identities on project and single-module APIs alike.
use super::*;

pub(super) struct Normalized {
    pub(super) module: Module,
    pub(super) pattern_types: BTreeMap<String, TypeRef>,
}

pub(super) fn reserved_declaration_name(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16" | "i32" | "i64" | "bool" | "void" | "c_handle" | "c_wstr"
    )
}

pub(super) fn normalize(module: &Module, limits: &Limits) -> R<Option<Normalized>> {
    if !module
        .items
        .iter()
        .any(|i| matches!(i.kind, ItemKind::TypeAlias(_)))
    {
        return Ok(None);
    }
    let mut budget = Budget::new(limits);
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
            && (!names.insert(name)
                || reserved_declaration_name(name))
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
                resolver.nominals.insert(a.name.clone(), a.generics.len());
            }
            ItemKind::Enum(e) => {
                resolver.nominals.insert(e.name.clone(), e.generics.len());
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
    Ok(Some(Normalized {
        module: output,
        pattern_types: visitor.pattern_types,
    }))
}

struct Alias {
    parameters: Vec<String>,
    target: TypeRef,
}
struct Resolver<'s, 'l> {
    definitions: BTreeMap<String, &'s TypeAlias>,
    nominals: BTreeMap<String, usize>,
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
                let arity = if path[0] == "Store" {
                    1
                } else if variables.contains(&path[0])
                    || matches!(
                        path[0].as_str(),
                        "u8" | "u16"
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
                    *self
                        .nominals
                        .get(&path[0])
                        .ok_or_else(|| error(ty.span, "unknown type in alias target"))?
                };
                if args.len() != arity {
                    return Err(error(ty.span, "alias target type argument arity mismatch"));
                }
                for arg in args {
                    let GenericArg::Type(t) = arg else {
                        return Err(unsupported(ty.span, "const alias type argument"));
                    };
                    self.target(t, variables, depth + 1)?;
                }
            }
            TypeKind::Group(t) => self.target(t, variables, depth + 1)?,
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if lifetime.is_some() {
                    return Err(unsupported(ty.span, "explicit alias lifetime"));
                }
                self.target(inner, variables, depth + 1)?;
            }
            TypeKind::Tuple(ts) => {
                for t in ts {
                    self.target(t, variables, depth + 1)?;
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
            && path.len() == 1
            && !types.contains(&path[0])
            && self.resolver.definitions.contains_key(&path[0])
        {
            *ty = self
                .resolver
                .instantiate(&path[0], args, types, ty.span, depth)?;
            return Ok(());
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
        let (name, arguments): (&str, &mut [GenericArg]) = match &mut e.kind {
            ExprKind::Name(name) => (name, &mut []),
            ExprKind::TypeApply { callee, arguments } => {
                let Some(name) = super::super::foreign::bare_name(callee) else {
                    return Ok(());
                };
                (name, arguments)
            }
            _ => return Ok(()),
        };
        if types.contains(name)
            || values.contains(name)
            || !self.resolver.definitions.contains_key(name)
        {
            return Ok(());
        }
        let alias = self
            .resolver
            .instantiate(name, arguments, types, e.span, depth)?;
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
            PatternKind::Reference { pattern, .. } => {
                self.pattern(pattern, types, values, depth + 1)?
            }
            PatternKind::Constructor { path, arguments } => {
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
            _ => {}
        }
        Ok(())
    }
    fn item(&mut self, item: &mut Item, outer: &Names, depth: usize) -> R<()> {
        self.resolver.budget.node(item.span, depth)?;
        let mut types = self.names(outer, item.span)?;
        let generics = match &item.kind {
            ItemKind::Impl(i) => i.generics.as_slice(),
            ItemKind::Extern { function, .. } => function.generics.as_slice(),
            _ => params(item),
        };
        for g in generics {
            types.insert(g.name.clone());
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
            ExprKind::Member { object, .. } => {
                self.namespace(object, types, values, depth + 1)?;
                self.expr(object, types, values, depth + 1)?;
            }
            ExprKind::TypeApply { callee, arguments } => {
                if super::super::foreign::bare_name(callee).is_some_and(|name| {
                    self.resolver.definitions.contains_key(name)
                        && !types.contains(name)
                        && !values.contains(name)
                }) {
                    self.namespace(e, types, values, depth + 1)?;
                } else {
                    self.expr(callee, types, values, depth + 1)?;
                    for arg in arguments {
                        match arg {
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

//! Project syntax normalization. Hygienic names are not type certificates: the
//! resulting declarations still enter ordinary shape, type and safety checks.
use super::*;
use generics::Budget;
use std::borrow::Cow;
use zil_base::{FileId, ModuleId};
use zil_syntax::ast::*;

type R<T> = Result<T, Diagnostic>;
type Names = BTreeSet<String>;
// The flag distinguishes source declarations from actual specialization output;
// a spelling prefix alone is not evidence of an instantiated function.
type Functions = BTreeMap<String, (ModuleId, String, bool)>;

#[derive(Clone)]
struct Symbol {
    canonical: String,
    public: bool,
    function: bool,
    generic: bool,
}
struct Index {
    symbols: BTreeMap<ModuleId, BTreeMap<String, Symbol>>,
    imports: BTreeMap<ModuleId, BTreeMap<String, ModuleId>>,
}

/// A direct single-module API does not run project normalization. Protect any
/// nominal declaration that a later alias/function substitution could capture.
/// The caller has bounded and validated the complete borrowed syntax first.
pub(super) fn protect_alias_nominals<'a>(
    module: &'a Module,
    budget: &mut Budget<'_>,
) -> R<Cow<'a, Module>> {
    if !module
        .items
        .iter()
        .any(|item| matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_)))
    {
        return Ok(Cow::Borrowed(module));
    }
    fn binders<'a>(
        item: &'a Item,
        names: &mut BTreeSet<&'a str>,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        budget.node(item.span, depth)?;
        let (parameters, members): (&[GenericParam], &[Item]) = match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => (&f.generics, &[]),
            ItemKind::Aggregate(a) => (&a.generics, &a.methods),
            ItemKind::Enum(e) => (&e.generics, &[]),
            ItemKind::TypeAlias(a) => (&a.generics, &[]),
            ItemKind::Impl(i) => (&i.generics, &i.members),
            ItemKind::Trait(t) => (&t.generics, &t.members),
            _ => (&[], &[]),
        };
        for parameter in parameters {
            budget.node(parameter.span, depth + 1)?;
            budget.text(&parameter.name, parameter.span)?;
            names.insert(&parameter.name);
        }
        for member in members {
            binders(member, names, budget, depth + 1)?;
        }
        Ok(())
    }
    let mut parameters = BTreeSet::new();
    let mut occupied = BTreeSet::new();
    for item in &module.items {
        binders(item, &mut parameters, budget, 0)?;
        if let Some((name, _, _)) = declaration(item) {
            occupied.insert(name);
        }
    }
    let mut symbols = BTreeMap::new();
    for item in &module.items {
        let (name, generic) = match &item.kind {
            ItemKind::Aggregate(a) => (&a.name, !a.generics.is_empty()),
            ItemKind::Enum(e) => (&e.name, !e.generics.is_empty()),
            _ => continue,
        };
        if !parameters.contains(name.as_str()) {
            continue;
        }
        let mut suffix = 0usize;
        let canonical = loop {
            budget.node(item.span, 0)?;
            let candidate = format!("$alias_nominal{}_{}_{}", module.id.0, suffix, name);
            budget.text(&candidate, item.span)?;
            if !parameters.contains(candidate.as_str()) && !occupied.contains(candidate.as_str()) {
                break candidate;
            }
            suffix = suffix.checked_add(1).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "alias nominal identity budget exhausted",
                    Some(item.span),
                )
            })?;
        };
        budget.text(name, item.span)?;
        symbols.insert(
            name.clone(),
            Symbol {
                canonical,
                public: item.public,
                function: false,
                generic,
            },
        );
    }
    if symbols.is_empty() {
        return Ok(Cow::Borrowed(module));
    }
    let index = Index {
        symbols: BTreeMap::from([(module.id, symbols)]),
        imports: BTreeMap::from([(module.id, BTreeMap::new())]),
    };
    // The borrowed preflight above precedes this full syntax clone. The same
    // lexical visitor as project normalization preserves local shadowing.
    let mut output = module.clone();
    let mut visitor = Visitor {
        default_nominal: None,
        mode: Mode::Normalize(&index),
        module: module.id,
        budget,
    };
    for item in &mut output.items {
        visitor.item(item, &Names::new(), true, 0)?;
    }
    Ok(Cow::Owned(output))
}

/// Give each lexical type binder an identity separate from source spellings,
/// including builtin names. Definitions and all scoped references are rewritten
/// together before substitution; no previously legal binder becomes reserved.
pub(super) fn protect_alias_binders<'a>(
    module: &'a Module,
    budget: &mut Budget<'_>,
) -> R<Cow<'a, Module>> {
    fn collect(
        item: &Item,
        occupied: &mut Names,
        found: &mut bool,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        budget.node(item.span, depth)?;
        if let Some((name, _, _)) = declaration(item) {
            budget.text(name, item.span)?;
            occupied.insert(name.to_owned());
        }
        let (params, members): (&[GenericParam], &[Item]) = match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => (&f.generics, &[]),
            ItemKind::Aggregate(a) => (&a.generics, &a.methods),
            ItemKind::Enum(e) => (&e.generics, &[]),
            ItemKind::TypeAlias(a) => (&a.generics, &[]),
            ItemKind::Impl(i) => (&i.generics, &i.members),
            ItemKind::Trait(t) => (&t.generics, &t.members),
            _ => (&[], &[]),
        };
        for p in params {
            budget.node(p.span, depth + 1)?;
            budget.text(&p.name, p.span)?;
            occupied.insert(p.name.clone());
            *found |= matches!(p.kind, GenericParamKind::Type(_));
        }
        for member in members {
            collect(member, occupied, found, budget, depth + 1)?;
        }
        Ok(())
    }
    let mut occupied = Names::new();
    let mut found = false;
    for item in &module.items {
        collect(item, &mut occupied, &mut found, budget, 0)?;
    }
    if !found {
        return Ok(Cow::Borrowed(module));
    }
    let mut output = module.clone();
    let mut visitor = Visitor {
        default_nominal: None,
        mode: Mode::Binders {
            bindings: BTreeMap::new(),
            occupied,
            next: 0,
        },
        module: module.id,
        budget,
    };
    for item in &mut output.items {
        visitor.item(item, &Names::new(), true, 0)?;
    }
    Ok(Cow::Owned(output))
}
fn error(code: &str, span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, code, Some(span), message)
}
fn declaration(item: &Item) -> Option<(&str, bool, bool)> {
    match &item.kind {
        ItemKind::Function(f) => Some((&f.name, true, !f.generics.is_empty())),
        ItemKind::Aggregate(a) => Some((&a.name, false, !a.generics.is_empty())),
        ItemKind::Enum(e) => Some((&e.name, false, !e.generics.is_empty())),
        ItemKind::TypeAlias(a) => Some((&a.name, false, !a.generics.is_empty())),
        ItemKind::Trait(t) => Some((&t.name, false, !t.generics.is_empty())),
        ItemKind::Const(c) => Some((&c.name, false, false)),
        ItemKind::Extern { function, .. } => Some((&function.name, true, false)),
        _ => None,
    }
}

pub(super) fn specialize(
    sources: &[SourceUnit],
    modules: &[&Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
) -> CompileResult<Vec<(Module, Vec<CoreImport>)>> {
    normalize(sources, modules, imports, limits).map_err(|e| vec![e])
}
fn normalize(
    sources: &[SourceUnit],
    modules: &[&Module],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
) -> R<Vec<(Module, Vec<CoreImport>)>> {
    let mut budget = Budget::new(limits);
    if sources.len() != modules.len() || modules.len() != imports.len() || modules.is_empty() {
        return Err(error(
            "E-MODULE-ID",
            Span::default(),
            "inconsistent project syntax inputs",
        ));
    }
    // Bound borrowed inputs before cloning any syntax or symbol table.
    for ((source, module), edges) in sources.iter().zip(modules).zip(imports) {
        budget.node(module.span, 0)?;
        if source.module_id != module.id || !module.span.is_valid_for(source) {
            return Err(error(
                "E-MODULE-ID",
                module.span,
                "source and syntax module identities differ",
            ));
        }
        for item in &module.items {
            budget.item(item)?;
        }
        for edge in edges {
            budget.node(edge.span, 0)?;
            budget.text(&edge.alias, edge.span)?;
        }
    }
    let mut index = Index {
        symbols: BTreeMap::new(),
        imports: BTreeMap::new(),
    };
    let mut files = BTreeMap::<FileId, usize>::new();
    let mut functions = Functions::new();
    for (position, ((source, module), edges)) in
        sources.iter().zip(modules).zip(imports).enumerate()
    {
        if files.insert(source.file_id, position).is_some()
            || index.symbols.contains_key(&module.id)
        {
            return Err(error(
                "E-MODULE-ID",
                module.span,
                "duplicate project module or file identity",
            ));
        }
        let mut symbols = BTreeMap::new();
        for item in &module.items {
            if let Some((name, function, generic)) = declaration(item) {
                if symbols.contains_key(name)
                    || super::primitive_name(name)
                    || matches!(name, "bool" | "void" | "c_handle")
                {
                    return Err(error(
                        "E-NAME-DUPLICATE",
                        item.span,
                        "duplicate declaration name",
                    ));
                }
                let canonical = format!("$module{}_{}", module.id.0, name);
                budget.text(&canonical, item.span)?;
                if function {
                    functions.insert(canonical.clone(), (module.id, name.to_owned(), true));
                }
                symbols.insert(
                    name.to_owned(),
                    Symbol {
                        canonical,
                        public: item.public,
                        function,
                        generic,
                    },
                );
            }
        }
        let mut aliases = BTreeMap::new();
        for edge in edges {
            if symbols.contains_key(&edge.alias)
                || aliases.insert(edge.alias.clone(), edge.module).is_some()
            {
                return Err(error(
                    "E-MODULE-ALIAS",
                    edge.span,
                    "module alias conflicts with another declaration",
                ));
            }
        }
        index.symbols.insert(module.id, symbols);
        index.imports.insert(module.id, aliases);
    }
    let mut combined = Module {
        id: modules[0].id,
        span: modules[0].span,
        items: Vec::new(),
    };
    for module in modules {
        let mut visitor = Visitor {
            default_nominal: None,
            mode: Mode::Normalize(&index),
            module: module.id,
            budget: &mut budget,
        };
        for source_item in &module.items {
            if matches!(source_item.kind, ItemKind::Import(_)) {
                continue;
            }
            let mut item = source_item.clone();
            visitor.item(&mut item, &Names::new(), true, 0)?;
            combined.items.push(item);
        }
    }
    let specialized = generics::specialize(&combined, limits).map_err(|mut e| e.remove(0))?;
    let mut output = modules
        .iter()
        .zip(imports)
        .map(|(m, i)| {
            (
                Module {
                    id: m.id,
                    span: m.span,
                    items: Vec::new(),
                },
                i.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut has_project_trait_impl = false;
    for item in &specialized.items {
        budget.item(item)?;
        let position = *files.get(&item.span.file).ok_or_else(|| {
            error(
                "E-MODULE-ID",
                item.span,
                "instantiated declaration lost its definition source",
            )
        })?;
        has_project_trait_impl |= project_traits::is_external_impl(item, modules[position].id);
        if let ItemKind::Function(f) = &item.kind {
            // An instance stays in its template's defining module. Publicness
            // is inherited from that declaration, never from the consumer.
            functions.entry(f.name.clone()).or_insert((
                modules[position].id,
                f.name.clone(),
                false,
            ));
        }
        output[position].0.items.push(item.clone());
    }
    let method_owners = if has_project_trait_impl {
        project_traits::method_owners(&specialized, modules, &files, &mut budget)?
    } else {
        BTreeMap::new()
    };
    for (module, edges) in &mut output {
        if !method_owners.is_empty() {
            for item in &module.items {
                budget.node(item.span, 0)?;
                let ItemKind::Function(function) = &item.kind else {
                    continue;
                };
                budget.text(&function.name, item.span)?;
                if functions
                    .get(&function.name)
                    .is_some_and(|(_, _, original)| !original)
                {
                    project_traits::instance_dependencies(
                        item,
                        &method_owners,
                        module.id,
                        edges,
                        &mut budget,
                    )?;
                }
            }
        }
        let mut visitor = Visitor {
            default_nominal: None,
            mode: Mode::Restore {
                functions: &functions,
                edges,
            },
            module: module.id,
            budget: &mut budget,
        };
        for item in &mut module.items {
            visitor.item(item, &Names::new(), true, 0)?;
        }
    }
    Ok(output)
}

enum Mode<'a> {
    Normalize(&'a Index),
    Binders {
        bindings: BTreeMap<String, String>,
        occupied: Names,
        next: usize,
    },
    Restore {
        functions: &'a Functions,
        edges: &'a mut Vec<CoreImport>,
    },
}
struct Visitor<'a, 'b, 'l> {
    default_nominal: Option<String>,
    mode: Mode<'a>,
    module: ModuleId,
    budget: &'b mut Budget<'l>,
}
impl Visitor<'_, '_, '_> {
    fn names(&mut self, input: &Names, span: Span) -> R<Names> {
        for name in input {
            self.budget.node(span, 0)?;
            self.budget.text(name, span)?;
        }
        Ok(input.clone())
    }
    fn symbol(
        &mut self,
        path: &[String],
        types: &Names,
        values: &Names,
        span: Span,
        type_position: bool,
    ) -> R<Option<String>> {
        if let Mode::Binders { bindings, .. } = &self.mode {
            let [name] = path else { return Ok(None) };
            if type_position
                && types.contains(name)
                && let Some(canonical) = bindings.get(name)
            {
                self.budget.text(canonical, span)?;
                return Ok(Some(canonical.clone()));
            }
            return Ok(None);
        }
        let Mode::Normalize(index) = &self.mode else {
            return Ok(None);
        };
        let Some(first) = path.first() else {
            return Ok(None);
        };
        if types.contains(first) || (!type_position && values.contains(first)) {
            return Ok(None);
        }
        let (owner, name, external) = match path {
            [name] => (self.module, name, false),
            [alias, name] => {
                let Some(owner) = index.imports[&self.module].get(alias) else {
                    return Ok(None);
                };
                (*owner, name, true)
            }
            _ => return Ok(None),
        };
        let Some(symbol) = index.symbols.get(&owner).and_then(|s| s.get(name)) else {
            if external && type_position {
                return Err(error(
                    "E-MODULE-MEMBER",
                    span,
                    "imported module has no such type declaration",
                ));
            }
            return Ok(None);
        };
        if external && !symbol.public {
            // Ordinary function calls retain the established CT/types diagnostic
            // path. A private template or type cannot enter specialization.
            if symbol.function && !symbol.generic && !type_position {
                return Ok(None);
            }
            return Err(error(
                "E-MODULE-PRIVATE",
                span,
                "imported declaration is private",
            ));
        }
        self.budget.text(&symbol.canonical, span)?;
        Ok(Some(symbol.canonical.clone()))
    }
    fn binder_namespace(
        &mut self,
        expression: &mut Expr,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        if !matches!(self.mode, Mode::Binders { .. }) {
            return Ok(());
        }
        self.budget.node(expression.span, depth)?;
        match &mut expression.kind {
            ExprKind::Name(name) if !values.contains(name) => {
                if let Some(canonical) = self.symbol(
                    std::slice::from_ref(name),
                    types,
                    values,
                    expression.span,
                    true,
                )? {
                    *name = canonical;
                }
            }
            ExprKind::Group(inner) | ExprKind::TypeApply { callee: inner, .. } => {
                self.binder_namespace(inner, types, values, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn declaration_name(&mut self, name: &mut String, top: bool, span: Span) -> R<()> {
        if !top {
            return Ok(());
        }
        match &self.mode {
            Mode::Binders { .. } => {}
            Mode::Normalize(index) => {
                if let Some(symbol) = index.symbols[&self.module].get(name) {
                    self.budget.text(&symbol.canonical, span)?;
                    *name = symbol.canonical.clone();
                }
            }
            Mode::Restore { functions, .. } => {
                if let Some((_, original, _)) = functions.get(name) {
                    self.budget.text(original, span)?;
                    *name = original.clone();
                }
            }
        }
        Ok(())
    }
    fn ty(&mut self, ty: &mut TypeRef, types: &Names, depth: usize) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &mut ty.kind {
            TypeKind::Named { path, args } | TypeKind::Dyn { path, args } => {
                if let Some(name) = self.symbol(path, types, &Names::new(), ty.span, true)? {
                    *path = vec![name];
                }
                for arg in args {
                    self.arg(arg, types, &Names::new(), depth + 1)?;
                }
            }
            TypeKind::Reference { inner, .. }
            | TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::ForLifetime { inner, .. } => self.ty(inner, types, depth + 1)?,
            TypeKind::Tuple(ts) | TypeKind::Union(ts) | TypeKind::Impl(ts) => {
                for t in ts {
                    self.ty(t, types, depth + 1)?;
                }
            }
            TypeKind::Array { element, length } => {
                self.ty(element, types, depth + 1)?;
                if let Some(e) = length {
                    self.expr(e, types, &Names::new(), depth + 1)?;
                }
            }
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
            TypeKind::MetaEval(e) => self.expr(e, types, &Names::new(), depth + 1)?,
            TypeKind::MetaBlock(b) => self.block(b, types, &Names::new(), depth + 1)?,
        }
        Ok(())
    }
    fn arg(&mut self, arg: &mut GenericArg, types: &Names, values: &Names, depth: usize) -> R<()> {
        match arg {
            GenericArg::Type(t) => self.ty(t, types, depth),
            GenericArg::Const(e) => self.expr(e, types, values, depth),
        }
    }
    fn item(&mut self, item: &mut Item, outer: &Names, top: bool, depth: usize) -> R<()> {
        self.budget.node(item.span, depth)?;
        let mut types = self.names(outer, item.span)?;
        let params = match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => f.generics.as_slice(),
            ItemKind::Aggregate(a) => a.generics.as_slice(),
            ItemKind::Enum(e) => e.generics.as_slice(),
            ItemKind::TypeAlias(a) => a.generics.as_slice(),
            ItemKind::Impl(i) => i.generics.as_slice(),
            ItemKind::Trait(t) => t.generics.as_slice(),
            _ => &[],
        };
        for p in params {
            types.insert(p.name.clone());
        }
        let previous = if let Mode::Binders {
            bindings,
            occupied,
            next,
        } = &mut self.mode
        {
            for (source, canonical) in bindings.iter() {
                self.budget.node(item.span, depth)?;
                self.budget.text(source, item.span)?;
                self.budget.text(canonical, item.span)?;
            }
            let previous = bindings.clone();
            let mut seen = BTreeSet::new();
            for p in params {
                if !matches!(p.kind, GenericParamKind::Type(_)) || !seen.insert(&p.name) {
                    continue;
                }
                self.budget.text(&p.name, p.span)?;
                let canonical = loop {
                    self.budget.node(p.span, depth + 1)?;
                    let name = format!("$alias_type{}_{}", self.module.0, *next);
                    *next = next.checked_add(1).ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Expand,
                            "alias binder identity budget exhausted",
                            Some(p.span),
                        )
                    })?;
                    self.budget.text(&name, p.span)?;
                    if !occupied.contains(&name) {
                        break name;
                    }
                };
                bindings.insert(p.name.clone(), canonical);
            }
            Some(previous)
        } else {
            None
        };
        match &mut item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => {
                self.declaration_name(&mut f.name, top, item.span)?;
                let mut values = Names::new();
                for parameter in &mut f.generics {
                    if let GenericParamKind::Type(bounds) = &mut parameter.kind {
                        for bound in bounds {
                            self.ty(bound, &types, depth + 1)?;
                        }
                    }
                }
                // A where bound has the same lexical/module identity as an
                // inline bound. Resolve it before combining modules; leaving
                // source spellings here would lose privacy and nominal identity.
                for clause in &mut f.where_clause {
                    self.budget.node(clause.span, depth + 1)?;
                    match &mut clause.subject {
                        BoundSubject::Type(subject) => self.ty(subject, &types, depth + 1)?,
                        BoundSubject::Lifetime(name) => self.budget.text(name, clause.span)?,
                    }
                    for bound in &mut clause.bounds {
                        self.ty(bound, &types, depth + 1)?;
                    }
                    if let Some(name) = &clause.lifetime_bound {
                        self.budget.text(name, clause.span)?;
                    }
                }
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
                self.declaration_name(&mut a.name, top, item.span)?;
                let previous_nominal = self.default_nominal.take();
                if a.fields.iter().any(|field| field.default.is_some()) {
                    self.budget.text(&a.name, item.span)?;
                    self.default_nominal = Some(a.name.clone());
                }
                for f in &mut a.fields {
                    self.ty(&mut f.ty, &types, depth + 1)?;
                    if let Some(e) = &mut f.default {
                        self.expr(e, &types, &Names::new(), depth + 1)?;
                    }
                }
                self.default_nominal = previous_nominal;
                for m in &mut a.methods {
                    self.item(m, &types, false, depth + 1)?;
                }
            }
            ItemKind::Enum(e) => {
                self.declaration_name(&mut e.name, top, item.span)?;
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
            ItemKind::TypeAlias(a) => {
                self.declaration_name(&mut a.name, top, item.span)?;
                if let Some(t) = &mut a.value {
                    self.ty(t, &types, depth + 1)?;
                }
            }
            ItemKind::Impl(i) => {
                self.ty(&mut i.target, &types, depth + 1)?;
                if let Some(t) = &mut i.trait_type {
                    self.ty(t, &types, depth + 1)?;
                }
                if let Some(p) = &mut i.receiver_default {
                    self.ty(&mut p.ty, &types, depth + 1)?;
                }
                for m in &mut i.members {
                    self.item(m, &types, false, depth + 1)?;
                }
            }
            ItemKind::Trait(t) => {
                self.declaration_name(&mut t.name, top, item.span)?;
                for bound in &mut t.bounds {
                    self.ty(bound, &types, depth + 1)?;
                }
                for m in &mut t.members {
                    self.item(m, &types, false, depth + 1)?;
                }
            }
            _ => {}
        }
        if let (Some(previous), Mode::Binders { bindings, .. }) = (previous, &mut self.mode) {
            let params = match &mut item.kind {
                ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => &mut f.generics,
                ItemKind::Aggregate(a) => &mut a.generics,
                ItemKind::Enum(e) => &mut e.generics,
                ItemKind::TypeAlias(a) => &mut a.generics,
                ItemKind::Impl(i) => &mut i.generics,
                ItemKind::Trait(t) => &mut t.generics,
                _ => {
                    *bindings = previous;
                    return Ok(());
                }
            };
            for p in params {
                if matches!(p.kind, GenericParamKind::Type(_)) {
                    let canonical = &bindings[&p.name];
                    self.budget.text(canonical, p.span)?;
                    p.name = canonical.clone();
                }
            }
            *bindings = previous;
        }
        Ok(())
    }
    fn pattern(
        &mut self,
        p: &mut Pattern,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        self.budget.node(p.span, depth)?;
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
                if path.len() >= 2 {
                    let count = path.len();
                    if let Some(name) =
                        self.symbol(&path[..count - 1], types, &Names::new(), p.span, true)?
                    {
                        *path = vec![name, path[count - 1].clone()];
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
    fn block(&mut self, b: &mut Block, types: &Names, values: &Names, depth: usize) -> R<()> {
        self.budget.node(b.span, depth)?;
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
            Condition::Expression(e) => self.expr(e, types, values, depth),
            Condition::Let { pattern, value } => {
                self.expr(value, types, values, depth)?;
                self.pattern(pattern, types, values, depth)
            }
        }
    }
    fn statement(
        &mut self,
        s: &mut Stmt,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        self.budget.node(s.span, depth)?;
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
            StmtKind::Return(Some(e)) | StmtKind::Yield(e) | StmtKind::Expression(e) => {
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
                let mut scope = self.names(values, s.span)?;
                self.pattern(pattern, types, &mut scope, depth + 1)?;
                self.block(body, types, &scope, depth + 1)?;
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
    fn arrow(&mut self, b: &mut ArrowBody, types: &Names, values: &Names, depth: usize) -> R<()> {
        match b {
            ArrowBody::Expression(e) => self.expr(e, types, values, depth),
            ArrowBody::Block(b) => self.block(b, types, values, depth),
        }
    }
    fn expr(&mut self, e: &mut Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        self.budget.node(e.span, depth)?;
        // Self in a default denotes its defining nominal, unless a lexical
        // value binder shadows that expression name. Reuse this visitor's
        // existing scoped name sets; do not rewrite arbitrary Self.x values.
        if matches!(&e.kind, ExprKind::Name(name) if name == "Self")
            && !values.contains("Self")
            && let Some(nominal) = &self.default_nominal
        {
            self.budget.text(nominal, e.span)?;
            e.kind = ExprKind::Name(nominal.clone());
            return Ok(());
        }
        let written = match &e.kind {
            ExprKind::Name(n) => Some(vec![n.clone()]),
            ExprKind::Member {
                object,
                member: MemberName::Name(n),
            } => match &object.kind {
                ExprKind::Name(alias) => Some(vec![alias.clone(), n.clone()]),
                _ => None,
            },
            _ => None,
        };
        if let Some(path) = written
            && let Some(name) = self.symbol(&path, types, values, e.span, false)?
        {
            e.kind = ExprKind::Name(name);
            return Ok(());
        }
        if let (Mode::Restore { functions, edges }, ExprKind::Name(name)) =
            (&mut self.mode, &e.kind)
            && let Some((owner, local, _)) = functions.get(name)
        {
            self.budget.text(local, e.span)?;
            e.kind = if *owner == self.module {
                ExprKind::Name(local.clone())
            } else {
                let alias = if let Some(edge) = edges.iter().find(|edge| edge.module == *owner) {
                    edge.alias.clone()
                } else {
                    let alias = format!("$module_import_{}", owner.0);
                    self.budget.node(e.span, depth)?;
                    self.budget.text(&alias, e.span)?;
                    edges.push(CoreImport {
                        alias: alias.clone(),
                        module: *owner,
                        span: e.span,
                    });
                    alias
                };
                ExprKind::Member {
                    object: Box::new(Expr {
                        kind: ExprKind::Name(alias),
                        span: e.span,
                    }),
                    member: MemberName::Name(local.clone()),
                }
            };
            return Ok(());
        }
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
                self.binder_namespace(object, types, values, depth + 1)?;
                self.expr(object, types, values, depth + 1)?;
            }
            ExprKind::TypeApply { callee, arguments } => {
                self.expr(callee, types, values, depth + 1)?;
                for arg in arguments {
                    self.arg(arg, types, values, depth + 1)?;
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
                    self.binder_namespace(t, types, values, depth + 1)?;
                    self.expr(t, types, values, depth + 1)?;
                }
                if let Some(b) = base {
                    self.expr(b, types, values, depth + 1)?;
                }
                for f in fields {
                    if let Some(x) = &mut f.value {
                        self.expr(x, types, values, depth + 1)?;
                    } else if let Some(name) =
                        self.symbol(std::slice::from_ref(&f.name), types, values, f.span, false)?
                    {
                        f.value = Some(Expr {
                            kind: ExprKind::Name(name),
                            span: f.span,
                        });
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

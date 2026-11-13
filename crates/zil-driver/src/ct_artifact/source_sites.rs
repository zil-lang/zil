//! Request-local structural addresses in the immutable source AST. Producer
//! pointers select an original node only while walking that AST; no pointer,
//! span, generated name or cloned TypeRef becomes source authority.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceTreeId {
    Original,
    Generated(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceSite {
    pub tree: SourceTreeId,
    pub module: zil_base::ModuleId,
    pub ordinal: u32,
}
#[derive(Clone, Copy)]
pub(crate) enum SourceNode<'a> {
    Expr(&'a s::Expr),
    Type(&'a s::TypeRef),
}
impl SourceNode<'_> {
    pub(crate) fn address(self) -> usize {
        match self {
            Self::Expr(node) => node as *const s::Expr as usize,
            Self::Type(node) => node as *const s::TypeRef as usize,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct LocatedNode<'a> {
    pub site: SourceSite,
    pub node: SourceNode<'a>,
    pub function: Option<&'a s::Function>,
}
#[derive(Clone, Copy)]
pub(crate) enum Selection {
    All,
    Expr(usize),
    Type(usize),
    Arguments(usize),
    Site(SourceSite),
}

pub(crate) fn select<'a>(
    module: &'a s::Module,
    selection: Selection,
    max_depth: usize,
    pay: impl FnMut(usize) -> R<()>,
) -> R<Option<LocatedNode<'a>>> {
    select_in_tree(module, SourceTreeId::Original, selection, max_depth, pay)
}
/// The caller selects a frozen catalog tree before walking. A catalog row is
/// only a request-local address; its emitter chain remains independently checked.
pub(crate) fn select_in_tree<'a>(
    module: &'a s::Module,
    tree: SourceTreeId,
    selection: Selection,
    max_depth: usize,
    pay: impl FnMut(usize) -> R<()>,
) -> R<Option<LocatedNode<'a>>> {
    select_from_node(module, tree, selection, None, max_depth, pay)
}
pub(crate) fn contains_node(
    module: &s::Module,
    root: SourceNode<'_>,
    candidate: SourceNode<'_>,
    max_depth: usize,
    pay: impl FnMut(usize) -> R<()>,
) -> R<bool> {
    let selection = match candidate {
        SourceNode::Expr(node) => Selection::Expr(node as *const s::Expr as usize),
        SourceNode::Type(node) => Selection::Type(node as *const s::TypeRef as usize),
    };
    Ok(select_from_node(
        module,
        SourceTreeId::Original,
        selection,
        Some(root),
        max_depth,
        pay,
    )?
    .is_some())
}
pub(crate) fn relative_node<'a>(
    module: &'a s::Module,
    root: SourceNode<'a>,
    selection: Selection,
    max_depth: usize,
    pay: impl FnMut(usize) -> R<()>,
) -> R<Option<LocatedNode<'a>>> {
    select_from_node(
        module,
        SourceTreeId::Original,
        selection,
        Some(root),
        max_depth,
        pay,
    )
}
#[derive(Clone, Copy)]
pub(crate) enum SiteEvent<'a> {
    Work(usize),
    Node(LocatedNode<'a>),
}
pub(crate) fn observe<'a>(
    module: &'a s::Module,
    tree: SourceTreeId,
    max_depth: usize,
    observer: impl FnMut(SiteEvent<'a>) -> R<()>,
) -> R<()> {
    walk_source_nodes(
        module.id,
        Some(module),
        tree,
        Selection::All,
        None,
        None,
        max_depth,
        observer,
    )
    .map(|_| ())
}
/// Item-local ordinals support a synchronous clone bridge. The caller maps
/// them to the frozen module's canonical sites before supplying authority.
pub(crate) fn observe_item<'a>(
    item: &'a s::Item,
    module: zil_base::ModuleId,
    tree: SourceTreeId,
    max_depth: usize,
    observer: impl FnMut(SiteEvent<'a>) -> R<()>,
) -> R<()> {
    walk_source_nodes(
        module,
        None,
        tree,
        Selection::All,
        None,
        Some(item),
        max_depth,
        observer,
    )
    .map(|_| ())
}
fn select_from_node<'a>(
    module: &'a s::Module,
    tree: SourceTreeId,
    selection: Selection,
    root: Option<SourceNode<'a>>,
    max_depth: usize,
    mut pay: impl FnMut(usize) -> R<()>,
) -> R<Option<LocatedNode<'a>>> {
    walk_source_nodes(
        module.id,
        Some(module),
        tree,
        selection,
        root,
        None,
        max_depth,
        |event| match event {
            SiteEvent::Work(work) => pay(work),
            SiteEvent::Node(_) => Ok(()),
        },
    )
}
// Keep the source, candidate, scope and request meter explicit at this proof boundary.
#[allow(clippy::too_many_arguments)]
fn walk_source_nodes<'a>(
    module_id: zil_base::ModuleId,
    module: Option<&'a s::Module>,
    tree: SourceTreeId,
    selection: Selection,
    root: Option<SourceNode<'a>>,
    root_item: Option<&'a s::Item>,
    max_depth: usize,
    pay: impl FnMut(SiteEvent<'a>) -> R<()>,
) -> R<Option<LocatedNode<'a>>> {
    struct Walk<'a, F> {
        module: zil_base::ModuleId,
        tree: SourceTreeId,
        next: u32,
        selection: Selection,
        function: Option<&'a s::Function>,
        found: Option<LocatedNode<'a>>,
        max_depth: usize,
        pay: F,
    }
    impl<'a, F: FnMut(SiteEvent<'a>) -> R<()>> Walk<'a, F> {
        fn step(&mut self, depth: usize) -> R<bool> {
            (self.pay)(SiteEvent::Work(1))?;
            if self.found.is_some() {
                return Ok(false);
            }
            if depth > self.max_depth {
                return Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT original source site depth exceeded",
                    None,
                )]);
            }
            Ok(true)
        }
        fn node(&mut self, node: SourceNode<'a>) -> R<()> {
            let site = SourceSite {
                module: self.module,
                tree: self.tree,
                ordinal: self.next,
            };
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| error(Span::default(), "CT source site ordinal overflow"))?;
            (self.pay)(SiteEvent::Node(LocatedNode {
                site,
                node,
                function: self.function,
            }))?;
            let matched = match (&self.selection, node) {
                (Selection::Site(target), _) => *target == site,
                (Selection::Expr(target), SourceNode::Expr(_))
                | (Selection::Type(target), SourceNode::Type(_)) => *target == node.address(),
                (
                    Selection::Arguments(target),
                    SourceNode::Type(s::TypeRef {
                        kind: s::TypeKind::Named { args, .. },
                        ..
                    }),
                ) => *target == args.as_ptr() as usize,
                (
                    Selection::Arguments(target),
                    SourceNode::Expr(s::Expr {
                        kind: s::ExprKind::TypeApply { arguments, .. },
                        ..
                    }),
                ) => *target == arguments.as_ptr() as usize,
                _ => false,
            };
            if matched {
                self.found = Some(LocatedNode {
                    site,
                    node,
                    function: self.function,
                });
            }
            Ok(())
        }
        fn binding(&mut self, binding: &'a s::DynBinding, depth: usize) -> R<()> {
            if !self.step(depth)? {
                return Ok(());
            }
            match &binding.kind {
                s::DynBindingKind::Type { value, .. } => self.ty(value, depth + 1)?,
                s::DynBindingKind::Constant { ty, value, .. } => {
                    self.ty(ty, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                s::DynBindingKind::Method { arguments, .. } => self.args(arguments, depth + 1)?,
            }
            Ok(())
        }
        fn args(&mut self, args: &'a [s::GenericArg], depth: usize) -> R<()> {
            for arg in args {
                if !self.step(depth)? {
                    break;
                }
                match arg {
                    s::GenericArg::Type(t) => self.ty(t, depth + 1)?,
                    s::GenericArg::Const(e) => self.expr(e, depth + 1)?,
                    s::GenericArg::Lifetime(_) => {}
                    s::GenericArg::AssociatedBinding(binding) => {
                        self.binding(binding, depth + 1)?
                    }
                }
            }
            Ok(())
        }
        fn ty(&mut self, ty: &'a s::TypeRef, depth: usize) -> R<()> {
            if !self.step(depth)? {
                return Ok(());
            }
            self.node(SourceNode::Type(ty))?;
            use s::TypeKind as T;
            match &ty.kind {
                T::Named { args, .. } => self.args(args, depth + 1)?,
                T::Reference { inner, .. }
                | T::Group(inner)
                | T::Optional(inner)
                | T::ForLifetime { inner, .. } => self.ty(inner, depth + 1)?,
                T::Tuple(parts) | T::Union(parts) | T::Impl(parts) => {
                    for part in parts {
                        self.ty(part, depth + 1)?;
                    }
                }
                T::Array { element, length } => {
                    self.ty(element, depth + 1)?;
                    if let Some(e) = length {
                        self.expr(e, depth + 1)?;
                    }
                }
                T::Function { params, result } | T::ForeignFunction { params, result, .. } => {
                    for p in params {
                        self.ty(p, depth + 1)?;
                    }
                    self.ty(result, depth + 1)?;
                }
                T::Error { error, result } => {
                    self.ty(error, depth + 1)?;
                    self.ty(result, depth + 1)?;
                }
                T::Dyn { args, bindings, .. } => {
                    self.args(args, depth + 1)?;
                    for binding in bindings {
                        if !self.step(depth + 1)? {
                            break;
                        }
                        match &binding.kind {
                            s::DynBindingKind::Type { value, .. } => self.ty(value, depth + 1)?,
                            s::DynBindingKind::Constant { ty, value, .. } => {
                                self.ty(ty, depth + 1)?;
                                self.expr(value, depth + 1)?;
                            }
                            s::DynBindingKind::Method { arguments, .. } => {
                                self.args(arguments, depth + 1)?
                            }
                        }
                    }
                }
                T::MetaEval(e) | T::MetaSplice(e) => self.expr(e, depth + 1)?,
                T::MetaBlock(b) => self.block(b, depth + 1)?,
            }
            Ok(())
        }
        fn arrow(&mut self, body: &'a s::ArrowBody, depth: usize) -> R<()> {
            match body {
                s::ArrowBody::Expression(e) => self.expr(e, depth)?,
                s::ArrowBody::Block(b) => self.block(b, depth)?,
            }
            Ok(())
        }
        fn expr(&mut self, e: &'a s::Expr, depth: usize) -> R<()> {
            if !self.step(depth)? {
                return Ok(());
            }
            self.node(SourceNode::Expr(e))?;
            use s::ExprKind as E;
            match &e.kind {
                E::Group(e)
                | E::Try(e)
                | E::MetaEval(e)
                | E::MetaSplice(e)
                | E::Unary { operand: e, .. } => self.expr(e, depth + 1)?,
                E::Ascribed { ty, value } => {
                    self.ty(ty, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                E::Tuple(parts) | E::Array(parts) => {
                    for p in parts {
                        self.expr(p, depth + 1)?;
                    }
                }
                E::Binary { left, right, .. }
                | E::Pipeline {
                    input: left,
                    target: right,
                }
                | E::Index {
                    object: left,
                    index: right,
                } => {
                    self.expr(left, depth + 1)?;
                    self.expr(right, depth + 1)?;
                }
                E::Member { object, member } => {
                    self.expr(object, depth + 1)?;
                    if let s::MemberName::Constrained { owner, .. } = member {
                        self.ty(owner, depth + 1)?;
                    }
                }
                E::TypeApply { callee, arguments } => {
                    self.expr(callee, depth + 1)?;
                    self.args(arguments, depth + 1)?;
                }
                E::Call { callee, arguments } => {
                    self.expr(callee, depth + 1)?;
                    for arg in arguments {
                        self.expr(&arg.value, depth + 1)?;
                    }
                }
                E::Slice { object, start, end } => {
                    self.expr(object, depth + 1)?;
                    if let Some(e) = start {
                        self.expr(e, depth + 1)?;
                    }
                    if let Some(e) = end {
                        self.expr(e, depth + 1)?;
                    }
                }
                E::Record { ty, base, fields } => {
                    if let Some(e) = ty {
                        self.expr(e, depth + 1)?;
                    }
                    if let Some(e) = base {
                        self.expr(e, depth + 1)?;
                    }
                    for field in fields {
                        if let Some(e) = &field.value {
                            self.expr(e, depth + 1)?;
                        }
                    }
                }
                E::Closure {
                    params,
                    return_type,
                    body,
                    ..
                } => {
                    for p in params {
                        if let Some(t) = &p.ty {
                            self.ty(t, depth + 1)?;
                        }
                    }
                    if let Some(t) = return_type {
                        self.ty(t, depth + 1)?;
                    }
                    self.arrow(body, depth + 1)?;
                }
                E::If {
                    condition,
                    then_value,
                    else_value,
                } => {
                    self.expr(condition, depth + 1)?;
                    self.expr(then_value, depth + 1)?;
                    self.expr(else_value, depth + 1)?;
                }
                E::Quote { template, .. } => {
                    for hole in &template.holes {
                        self.expr(&hole.value, depth + 1)?;
                    }
                }
                E::MetaBlock(b) => self.block(b, depth + 1)?,
                E::Literal(_) | E::Name(_) | E::ReceiverMember(_) | E::SyntaxIsland { .. } => {}
            }
            Ok(())
        }
        fn condition(&mut self, c: &'a s::Condition, depth: usize) -> R<()> {
            match c {
                s::Condition::Expression(e) | s::Condition::Let { value: e, .. } => {
                    self.expr(e, depth)
                }
            }
        }
        fn block(&mut self, b: &'a s::Block, depth: usize) -> R<()> {
            if !self.step(depth)? {
                return Ok(());
            }
            for stmt in &b.statements {
                self.stmt(stmt, depth + 1)?;
            }
            Ok(())
        }
        fn stmt(&mut self, stmt: &'a s::Stmt, depth: usize) -> R<()> {
            if !self.step(depth)? {
                return Ok(());
            }
            use s::StmtKind as S;
            match &stmt.kind {
                S::Binding { ty, value, .. } => {
                    if let Some(t) = ty {
                        self.ty(t, depth + 1)?;
                    }
                    self.expr(value, depth + 1)?;
                }
                S::Return(e) => {
                    if let Some(e) = e {
                        self.expr(e, depth + 1)?;
                    }
                }
                S::Yield(e) | S::Expression(e) => self.expr(e, depth + 1)?,
                S::Defer(b) | S::Loop(b) | S::Unsafe(b) => self.block(b, depth + 1)?,
                S::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.condition(condition, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(b) = else_body {
                        match b {
                            s::ElseBranch::Block(b) => self.block(b, depth + 1)?,
                            s::ElseBranch::If(stmt) => self.stmt(stmt, depth + 1)?,
                        }
                    }
                }
                S::While { condition, body } => {
                    self.condition(condition, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                S::For { iterable, body, .. }
                | S::With {
                    value: iterable,
                    body,
                    ..
                } => {
                    self.expr(iterable, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                S::Match { value, arms } => {
                    self.expr(value, depth + 1)?;
                    for arm in arms {
                        self.arrow(&arm.body, depth + 1)?;
                    }
                }
                S::Assign { target, value, .. } => {
                    self.expr(target, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                S::Meta(meta) => match meta {
                    s::MetaStatement::If {
                        condition,
                        then_body,
                        else_body,
                    } => {
                        self.expr(condition, depth + 1)?;
                        self.block(then_body, depth + 1)?;
                        if let Some(b) = else_body {
                            self.block(b, depth + 1)?;
                        }
                    }
                    s::MetaStatement::For { iterable, body, .. } => {
                        self.expr(iterable, depth + 1)?;
                        self.block(body, depth + 1)?;
                    }
                    s::MetaStatement::Emit(e) => self.expr(e, depth + 1)?,
                },
                S::Break | S::Continue => {}
            }
            Ok(())
        }
        fn generics(&mut self, params: &'a [s::GenericParam], depth: usize) -> R<()> {
            for p in params {
                if !self.step(depth)? {
                    break;
                }
                match &p.kind {
                    s::GenericParamKind::Type(bounds) => {
                        for t in bounds {
                            self.ty(t, depth + 1)?;
                        }
                    }
                    s::GenericParamKind::Const(t) => self.ty(t, depth + 1)?,
                    s::GenericParamKind::Lifetime => {}
                }
            }
            Ok(())
        }
        fn attrs(&mut self, attrs: &'a [s::Attribute], depth: usize) -> R<()> {
            for a in attrs {
                if !self.step(depth)? {
                    break;
                }
                for e in &a.arguments {
                    self.expr(e, depth + 1)?;
                }
            }
            Ok(())
        }
        fn bounds(&mut self, bounds: &'a [s::BoundClause], depth: usize) -> R<()> {
            for b in bounds {
                if !self.step(depth)? {
                    break;
                }
                if let s::BoundSubject::Type(t) = &b.subject {
                    self.ty(t, depth + 1)?;
                }
                for t in &b.bounds {
                    self.ty(t, depth + 1)?;
                }
            }
            Ok(())
        }
        fn field(&mut self, f: &'a s::Field, depth: usize) -> R<()> {
            if !self.step(depth)? {
                return Ok(());
            }
            self.attrs(&f.attributes, depth + 1)?;
            self.ty(&f.ty, depth + 1)?;
            if let Some(e) = &f.default {
                self.expr(e, depth + 1)?;
            }
            Ok(())
        }
        fn term(&mut self, t: &'a s::ContractTerm, depth: usize) -> R<()> {
            if !self.step(depth)? {
                return Ok(());
            }
            self.args(&t.type_args, depth + 1)?;
            for t in &t.arguments {
                self.term(t, depth + 1)?;
            }
            if let Some(t) = &t.index {
                self.term(t, depth + 1)?;
            }
            Ok(())
        }
        fn function(&mut self, f: &'a s::Function, depth: usize) -> R<()> {
            let old = self.function.replace(f);
            self.generics(&f.generics, depth + 1)?;
            for p in &f.params {
                self.attrs(&p.attributes, depth + 1)?;
                self.ty(&p.ty, depth + 1)?;
            }
            if let Some(t) = &f.return_type {
                self.ty(t, depth + 1)?;
            }
            self.bounds(&f.where_clause, depth + 1)?;
            for c in &f.contract {
                if !self.step(depth + 1)? {
                    break;
                }
                for t in &c.terms {
                    self.term(t, depth + 1)?;
                }
            }
            match &f.body {
                s::FunctionBody::Block(b) => self.block(b, depth + 1)?,
                s::FunctionBody::Expression(e) => self.expr(e, depth + 1)?,
                s::FunctionBody::Declaration => {}
            }
            self.function = old;
            Ok(())
        }
        fn item(&mut self, item: &'a s::Item, depth: usize) -> R<()> {
            if !self.step(depth)? {
                return Ok(());
            }
            self.attrs(&item.attributes, depth + 1)?;
            match &item.kind {
                s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => {
                    self.function(f, depth + 1)?
                }
                s::ItemKind::Aggregate(a) => {
                    self.generics(&a.generics, depth + 1)?;
                    for f in &a.fields {
                        self.field(f, depth + 1)?;
                    }
                    for m in &a.methods {
                        self.item(m, depth + 1)?;
                    }
                }
                s::ItemKind::Enum(e) => {
                    self.generics(&e.generics, depth + 1)?;
                    for v in &e.variants {
                        if !self.step(depth + 1)? {
                            break;
                        }
                        self.attrs(&v.attributes, depth + 1)?;
                        match &v.fields {
                            s::VariantFields::Unit => {}
                            s::VariantFields::Tuple(fields) => {
                                for f in fields {
                                    self.ty(&f.ty, depth + 1)?;
                                }
                            }
                            s::VariantFields::Record(fields) => {
                                for f in fields {
                                    self.field(f, depth + 1)?;
                                }
                            }
                        }
                    }
                }
                s::ItemKind::Trait(t) => {
                    self.generics(&t.generics, depth + 1)?;
                    for b in &t.bounds {
                        self.ty(b, depth + 1)?;
                    }
                    for m in &t.members {
                        self.item(m, depth + 1)?;
                    }
                }
                s::ItemKind::Impl(i) => {
                    self.generics(&i.generics, depth + 1)?;
                    if let Some(t) = &i.trait_type {
                        self.ty(t, depth + 1)?;
                    }
                    self.ty(&i.target, depth + 1)?;
                    if let Some(p) = &i.receiver_default {
                        self.ty(&p.ty, depth + 1)?;
                    }
                    self.bounds(&i.where_clause, depth + 1)?;
                    for m in &i.members {
                        self.item(m, depth + 1)?;
                    }
                }
                s::ItemKind::TypeAlias(a) => {
                    self.generics(&a.generics, depth + 1)?;
                    for t in &a.bounds {
                        self.ty(t, depth + 1)?;
                    }
                    if let Some(t) = &a.value {
                        self.ty(t, depth + 1)?;
                    }
                }
                s::ItemKind::Const(c) => {
                    if let Some(t) = &c.ty {
                        self.ty(t, depth + 1)?;
                    }
                    if let Some(e) = &c.value {
                        self.expr(e, depth + 1)?;
                    }
                }
                s::ItemKind::Meta(m) => match m {
                    s::MetaItem::If {
                        condition,
                        then_items,
                        else_items,
                    } => {
                        self.expr(condition, depth + 1)?;
                        for i in then_items.iter().chain(else_items) {
                            self.item(i, depth + 1)?;
                        }
                    }
                    s::MetaItem::For {
                        iterable, items, ..
                    } => {
                        self.expr(iterable, depth + 1)?;
                        for i in items {
                            self.item(i, depth + 1)?;
                        }
                    }
                    s::MetaItem::Emit(e) => self.expr(e, depth + 1)?,
                },
                s::ItemKind::Import(_) => {}
            }
            Ok(())
        }
    }
    let mut walk = Walk {
        module: module_id,
        tree,
        next: 0,
        selection,
        function: None,
        found: None,
        max_depth,
        pay,
    };
    match root {
        Some(SourceNode::Expr(node)) => walk.expr(node, 0)?,
        Some(SourceNode::Type(node)) => walk.ty(node, 0)?,
        None => {
            if let Some(item) = root_item {
                walk.item(item, 0)?;
            } else if let Some(module) = module {
                for item in &module.items {
                    walk.item(item, 0)?;
                    if walk.found.is_some() {
                        break;
                    }
                }
            }
        }
    }
    Ok(walk.found)
}

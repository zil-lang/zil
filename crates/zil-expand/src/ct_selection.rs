//! Borrowed, bounded syntax traversal and an untrusted condition bridge.
//! None of these interfaces decides purity, selection or safety. The driver
//! must obtain an actual independently verified CT result before materializing.
use super::*;

pub trait Visitor {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()>;
    fn text(&mut self, value: &str, span: Span) -> CompileResult<()>;
    fn item(&mut self, item: &s::Item, depth: usize) -> CompileResult<()> {
        walk_item(self, item, depth)
    }
    fn block(&mut self, block: &s::Block, depth: usize) -> CompileResult<()> {
        walk_block(self, block, depth)
    }
    fn statement(&mut self, statement: &s::Stmt, depth: usize) -> CompileResult<()> {
        walk_statement(self, statement, depth)
    }
    fn expression(&mut self, expression: &s::Expr, depth: usize) -> CompileResult<()> {
        walk_expression(self, expression, depth)
    }
    fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
        walk_type(self, ty, depth)
    }
}

fn path<V: Visitor + ?Sized>(
    v: &mut V,
    names: &[String],
    span: Span,
    depth: usize,
) -> CompileResult<()> {
    for name in names {
        v.node(span, depth)?;
        v.text(name, span)?;
    }
    Ok(())
}
fn attributes<V: Visitor + ?Sized>(
    v: &mut V,
    values: &[s::Attribute],
    depth: usize,
) -> CompileResult<()> {
    for a in values {
        v.node(a.span, depth)?;
        path(v, &a.path, a.span, depth + 1)?;
        if let Some(text) = &a.string_value {
            v.text(text, a.span)?;
        }
        for e in &a.arguments {
            v.expression(e, depth + 1)?;
        }
    }
    Ok(())
}
fn argument<V: Visitor + ?Sized>(v: &mut V, a: &s::GenericArg, depth: usize) -> CompileResult<()> {
    match a {
        s::GenericArg::Type(t) => v.ty(t, depth),
        s::GenericArg::Const(e) => v.expression(e, depth),
    }
}
fn generics<V: Visitor + ?Sized>(
    v: &mut V,
    gs: &[s::GenericParam],
    depth: usize,
) -> CompileResult<()> {
    for g in gs {
        v.node(g.span, depth)?;
        v.text(&g.name, g.span)?;
        match &g.kind {
            s::GenericParamKind::Type(ts) => {
                for t in ts {
                    v.ty(t, depth + 1)?;
                }
            }
            s::GenericParamKind::Const(t) => v.ty(t, depth + 1)?,
            s::GenericParamKind::Lifetime => {}
        }
    }
    Ok(())
}
fn bounds<V: Visitor + ?Sized>(
    v: &mut V,
    bs: &[s::BoundClause],
    depth: usize,
) -> CompileResult<()> {
    for b in bs {
        v.node(b.span, depth)?;
        match &b.subject {
            s::BoundSubject::Type(t) => v.ty(t, depth + 1)?,
            s::BoundSubject::Lifetime(n) => v.text(n, b.span)?,
        }
        for t in &b.bounds {
            v.ty(t, depth + 1)?;
        }
        if let Some(n) = &b.lifetime_bound {
            v.text(n, b.span)?;
        }
    }
    Ok(())
}
fn term<V: Visitor + ?Sized>(v: &mut V, t: &s::ContractTerm, depth: usize) -> CompileResult<()> {
    v.node(t.span, depth)?;
    path(v, &t.path, t.span, depth + 1)?;
    for a in &t.type_args {
        argument(v, a, depth + 1)?;
    }
    for a in &t.arguments {
        term(v, a, depth + 1)?;
    }
    if let Some(i) = &t.index {
        term(v, i, depth + 1)?;
    }
    Ok(())
}
fn parameter<V: Visitor + ?Sized>(v: &mut V, p: &s::Param, depth: usize) -> CompileResult<()> {
    v.node(p.span, depth)?;
    v.text(&p.name, p.span)?;
    attributes(v, &p.attributes, depth + 1)?;
    v.ty(&p.ty, depth + 1)
}
fn function<V: Visitor + ?Sized>(v: &mut V, f: &s::Function, depth: usize) -> CompileResult<()> {
    v.node(f.span, depth)?;
    v.text(&f.name, f.span)?;
    generics(v, &f.generics, depth + 1)?;
    for p in &f.params {
        parameter(v, p, depth + 1)?;
    }
    if let Some(t) = &f.return_type {
        v.ty(t, depth + 1)?;
    }
    bounds(v, &f.where_clause, depth + 1)?;
    for c in &f.contract {
        v.node(c.span, depth + 1)?;
        v.text(&c.name, c.span)?;
        for t in &c.terms {
            term(v, t, depth + 2)?;
        }
    }
    match &f.body {
        s::FunctionBody::Block(b) => v.block(b, depth + 1),
        s::FunctionBody::Expression(e) => v.expression(e, depth + 1),
        s::FunctionBody::Declaration => Ok(()),
    }
}
fn field<V: Visitor + ?Sized>(v: &mut V, f: &s::Field, depth: usize) -> CompileResult<()> {
    v.node(f.span, depth)?;
    v.text(&f.name, f.span)?;
    attributes(v, &f.attributes, depth + 1)?;
    v.ty(&f.ty, depth + 1)?;
    if let Some(e) = &f.default {
        v.expression(e, depth + 1)?;
    }
    Ok(())
}
pub fn walk_item<V: Visitor + ?Sized>(
    v: &mut V,
    item: &s::Item,
    depth: usize,
) -> CompileResult<()> {
    v.node(item.span, depth)?;
    attributes(v, &item.attributes, depth + 1)?;
    match &item.kind {
        s::ItemKind::Import(i) => {
            path(v, &i.path, item.span, depth + 1)?;
            if let Some(n) = &i.alias {
                v.text(n, item.span)?;
            }
        }
        s::ItemKind::Function(f) => function(v, f, depth + 1)?,
        s::ItemKind::Extern { abi, function: f } => {
            v.text(abi, item.span)?;
            function(v, f, depth + 1)?;
        }
        s::ItemKind::Aggregate(a) => {
            v.text(&a.name, item.span)?;
            generics(v, &a.generics, depth + 1)?;
            for f in &a.fields {
                field(v, f, depth + 1)?;
            }
            for m in &a.methods {
                v.item(m, depth + 1)?;
            }
        }
        s::ItemKind::Enum(e) => {
            v.text(&e.name, item.span)?;
            generics(v, &e.generics, depth + 1)?;
            for a in &e.variants {
                v.node(a.span, depth + 1)?;
                v.text(&a.name, a.span)?;
                attributes(v, &a.attributes, depth + 2)?;
                match &a.fields {
                    s::VariantFields::Unit => {}
                    s::VariantFields::Tuple(ts) => {
                        for t in ts {
                            v.ty(t, depth + 2)?;
                        }
                    }
                    s::VariantFields::Record(fs) => {
                        for f in fs {
                            field(v, f, depth + 2)?;
                        }
                    }
                }
            }
        }
        s::ItemKind::Trait(t) => {
            v.text(&t.name, item.span)?;
            generics(v, &t.generics, depth + 1)?;
            for b in &t.bounds {
                v.ty(b, depth + 1)?;
            }
            for m in &t.members {
                v.item(m, depth + 1)?;
            }
        }
        s::ItemKind::Impl(i) => {
            generics(v, &i.generics, depth + 1)?;
            v.ty(&i.target, depth + 1)?;
            if let Some(t) = &i.trait_type {
                v.ty(t, depth + 1)?;
            }
            if let Some(p) = &i.receiver_default {
                parameter(v, p, depth + 1)?;
            }
            bounds(v, &i.where_clause, depth + 1)?;
            for m in &i.members {
                v.item(m, depth + 1)?;
            }
        }
        s::ItemKind::TypeAlias(a) => {
            v.text(&a.name, item.span)?;
            generics(v, &a.generics, depth + 1)?;
            for t in &a.bounds {
                v.ty(t, depth + 1)?;
            }
            if let Some(t) = &a.value {
                v.ty(t, depth + 1)?;
            }
        }
        s::ItemKind::Const(c) => {
            v.text(&c.name, item.span)?;
            if let Some(t) = &c.ty {
                v.ty(t, depth + 1)?;
            }
            if let Some(e) = &c.value {
                v.expression(e, depth + 1)?;
            }
        }
        s::ItemKind::Meta(s::MetaItem::If {
            condition,
            then_items,
            else_items,
        }) => {
            v.expression(condition, depth + 1)?;
            for i in then_items.iter().chain(else_items) {
                v.item(i, depth + 1)?;
            }
        }
        s::ItemKind::Meta(s::MetaItem::For {
            binding,
            iterable,
            items,
        }) => {
            v.text(binding, item.span)?;
            v.expression(iterable, depth + 1)?;
            for i in items {
                v.item(i, depth + 1)?;
            }
        }
        s::ItemKind::Meta(s::MetaItem::Emit(e)) => v.expression(e, depth + 1)?,
    }
    Ok(())
}
pub fn walk_type<V: Visitor + ?Sized>(
    v: &mut V,
    t: &s::TypeRef,
    depth: usize,
) -> CompileResult<()> {
    v.node(t.span, depth)?;
    match &t.kind {
        s::TypeKind::Named { path: names, args } | s::TypeKind::Dyn { path: names, args } => {
            path(v, names, t.span, depth + 1)?;
            for a in args {
                argument(v, a, depth + 1)?;
            }
        }
        s::TypeKind::Reference {
            lifetime, inner, ..
        } => {
            if let Some(n) = lifetime {
                v.text(n, t.span)?;
            }
            v.ty(inner, depth + 1)?;
        }
        s::TypeKind::Group(t) | s::TypeKind::Optional(t) => v.ty(t, depth + 1)?,
        s::TypeKind::Tuple(ts) | s::TypeKind::Union(ts) | s::TypeKind::Impl(ts) => {
            for t in ts {
                v.ty(t, depth + 1)?;
            }
        }
        s::TypeKind::Array { element, length } => {
            v.ty(element, depth + 1)?;
            if let Some(e) = length {
                v.expression(e, depth + 1)?;
            }
        }
        s::TypeKind::Function { params, result }
        | s::TypeKind::ForeignFunction { params, result, .. } => {
            if let s::TypeKind::ForeignFunction { abi, .. } = &t.kind {
                v.text(abi, t.span)?;
            }
            for t in params {
                v.ty(t, depth + 1)?;
            }
            v.ty(result, depth + 1)?;
        }
        s::TypeKind::Error { error, result } => {
            v.ty(error, depth + 1)?;
            v.ty(result, depth + 1)?;
        }
        s::TypeKind::ForLifetime { lifetime, inner } => {
            v.text(lifetime, t.span)?;
            v.ty(inner, depth + 1)?;
        }
        s::TypeKind::MetaEval(e) => v.expression(e, depth + 1)?,
        s::TypeKind::MetaBlock(b) => v.block(b, depth + 1)?,
    }
    Ok(())
}
pub fn walk_pattern<V: Visitor + ?Sized>(
    v: &mut V,
    p: &s::Pattern,
    depth: usize,
) -> CompileResult<()> {
    v.node(p.span, depth)?;
    match &p.kind {
        s::PatternKind::Name(n) => v.text(n, p.span)?,
        s::PatternKind::Wildcard => {}
        s::PatternKind::Literal(l) => literal(v, l, p.span)?,
        s::PatternKind::Reference { pattern, .. } => walk_pattern(v, pattern, depth + 1)?,
        s::PatternKind::Tuple(ps) => {
            for p in ps {
                walk_pattern(v, p, depth + 1)?;
            }
        }
        s::PatternKind::Constructor {
            path: names,
            arguments,
        } => {
            path(v, names, p.span, depth + 1)?;
            for p in arguments {
                walk_pattern(v, p, depth + 1)?;
            }
        }
    }
    Ok(())
}
fn condition<V: Visitor + ?Sized>(v: &mut V, c: &s::Condition, depth: usize) -> CompileResult<()> {
    match c {
        s::Condition::Expression(e) => v.expression(e, depth),
        s::Condition::Let { pattern, value } => {
            walk_pattern(v, pattern, depth)?;
            v.expression(value, depth)
        }
    }
}
pub fn walk_block<V: Visitor + ?Sized>(v: &mut V, b: &s::Block, depth: usize) -> CompileResult<()> {
    v.node(b.span, depth)?;
    for s in &b.statements {
        v.statement(s, depth + 1)?;
    }
    Ok(())
}
pub fn walk_statement<V: Visitor + ?Sized>(
    v: &mut V,
    s: &s::Stmt,
    depth: usize,
) -> CompileResult<()> {
    v.node(s.span, depth)?;
    match &s.kind {
        s::StmtKind::Binding {
            pattern, ty, value, ..
        } => {
            walk_pattern(v, pattern, depth + 1)?;
            if let Some(t) = ty {
                v.ty(t, depth + 1)?;
            }
            v.expression(value, depth + 1)?;
        }
        s::StmtKind::Return(e) => {
            if let Some(e) = e {
                v.expression(e, depth + 1)?;
            }
        }
        s::StmtKind::Yield(e) | s::StmtKind::Expression(e) => v.expression(e, depth + 1)?,
        s::StmtKind::Break | s::StmtKind::Continue => {}
        s::StmtKind::Defer(b) | s::StmtKind::Loop(b) | s::StmtKind::Unsafe(b) => {
            v.block(b, depth + 1)?
        }
        s::StmtKind::If {
            condition: c,
            then_body,
            else_body,
        } => {
            condition(v, c, depth + 1)?;
            v.block(then_body, depth + 1)?;
            match else_body {
                Some(s::ElseBranch::Block(b)) => v.block(b, depth + 1)?,
                Some(s::ElseBranch::If(s)) => v.statement(s, depth + 1)?,
                None => {}
            }
        }
        s::StmtKind::While { condition: c, body } => {
            condition(v, c, depth + 1)?;
            v.block(body, depth + 1)?;
        }
        s::StmtKind::For {
            pattern,
            iterable,
            body,
        } => {
            walk_pattern(v, pattern, depth + 1)?;
            v.expression(iterable, depth + 1)?;
            v.block(body, depth + 1)?;
        }
        s::StmtKind::Match { value, arms } => {
            v.expression(value, depth + 1)?;
            for a in arms {
                v.node(a.span, depth + 1)?;
                walk_pattern(v, &a.pattern, depth + 2)?;
                arrow(v, &a.body, depth + 2)?;
            }
        }
        s::StmtKind::With { name, value, body } => {
            v.text(name, s.span)?;
            v.expression(value, depth + 1)?;
            v.block(body, depth + 1)?;
        }
        s::StmtKind::Assign { target, value, .. } => {
            v.expression(target, depth + 1)?;
            v.expression(value, depth + 1)?;
        }
        s::StmtKind::Meta(s::MetaStatement::If {
            condition,
            then_body,
            else_body,
        }) => {
            v.expression(condition, depth + 1)?;
            v.block(then_body, depth + 1)?;
            if let Some(b) = else_body {
                v.block(b, depth + 1)?;
            }
        }
        s::StmtKind::Meta(s::MetaStatement::For {
            binding,
            iterable,
            body,
        }) => {
            v.text(binding, s.span)?;
            v.expression(iterable, depth + 1)?;
            v.block(body, depth + 1)?;
        }
        s::StmtKind::Meta(s::MetaStatement::Emit(e)) => v.expression(e, depth + 1)?,
    }
    Ok(())
}
pub fn arrow<V: Visitor + ?Sized>(v: &mut V, a: &s::ArrowBody, depth: usize) -> CompileResult<()> {
    match a {
        s::ArrowBody::Expression(e) => v.expression(e, depth),
        s::ArrowBody::Block(b) => v.block(b, depth),
    }
}
fn literal<V: Visitor + ?Sized>(v: &mut V, l: &s::Literal, span: Span) -> CompileResult<()> {
    match l {
        s::Literal::Integer(n)
        | s::Literal::Float(n)
        | s::Literal::String(n)
        | s::Literal::Char(n) => v.text(n, span),
        s::Literal::Bool(_) => Ok(()),
    }
}
fn tree<V: Visitor + ?Sized>(v: &mut V, t: &s::TokenTree, depth: usize) -> CompileResult<()> {
    v.node(t.span, depth)?;
    for e in &t.children {
        match e {
            s::TokenTreeEntry::Token { text, span } => {
                v.node(*span, depth + 1)?;
                v.text(text, *span)?;
            }
            s::TokenTreeEntry::Tree(t) => tree(v, t, depth + 1)?,
        }
    }
    Ok(())
}
pub fn walk_expression<V: Visitor + ?Sized>(
    v: &mut V,
    e: &s::Expr,
    depth: usize,
) -> CompileResult<()> {
    v.node(e.span, depth)?;
    match &e.kind {
        s::ExprKind::Literal(l) => literal(v, l, e.span)?,
        s::ExprKind::Name(n) | s::ExprKind::ReceiverMember(n) => v.text(n, e.span)?,
        s::ExprKind::Group(e)
        | s::ExprKind::Try(e)
        | s::ExprKind::MetaEval(e)
        | s::ExprKind::Unary { operand: e, .. } => v.expression(e, depth + 1)?,
        s::ExprKind::Tuple(es) | s::ExprKind::Array(es) => {
            for e in es {
                v.expression(e, depth + 1)?;
            }
        }
        s::ExprKind::Binary { left, right, .. }
        | s::ExprKind::Pipeline {
            input: left,
            target: right,
        } => {
            v.expression(left, depth + 1)?;
            v.expression(right, depth + 1)?;
        }
        s::ExprKind::Member { object, member } => {
            v.expression(object, depth + 1)?;
            if let s::MemberName::Name(n) = member {
                v.text(n, e.span)?;
            }
        }
        s::ExprKind::TypeApply { callee, arguments } => {
            v.expression(callee, depth + 1)?;
            for a in arguments {
                argument(v, a, depth + 1)?;
            }
        }
        s::ExprKind::Call { callee, arguments } => {
            v.expression(callee, depth + 1)?;
            for a in arguments {
                v.node(a.span, depth + 1)?;
                v.expression(&a.value, depth + 2)?;
            }
        }
        s::ExprKind::Index { object, index } => {
            v.expression(object, depth + 1)?;
            v.expression(index, depth + 1)?;
        }
        s::ExprKind::Slice { object, start, end } => {
            v.expression(object, depth + 1)?;
            for e in [start, end].into_iter().flatten() {
                v.expression(e, depth + 1)?;
            }
        }
        s::ExprKind::Record { ty, base, fields } => {
            for e in [ty, base].into_iter().flatten() {
                v.expression(e, depth + 1)?;
            }
            for f in fields {
                v.node(f.span, depth + 1)?;
                v.text(&f.name, f.span)?;
                if let Some(e) = &f.value {
                    v.expression(e, depth + 2)?;
                }
            }
        }
        s::ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            for p in params {
                v.node(p.span, depth + 1)?;
                v.text(&p.name, p.span)?;
                if let Some(t) = &p.ty {
                    v.ty(t, depth + 2)?;
                }
            }
            if let Some(t) = return_type {
                v.ty(t, depth + 1)?;
            }
            arrow(v, body, depth + 1)?;
        }
        s::ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            v.expression(condition, depth + 1)?;
            v.expression(then_value, depth + 1)?;
            v.expression(else_value, depth + 1)?;
        }
        s::ExprKind::MetaBlock(b) => v.block(b, depth + 1)?,
        s::ExprKind::SyntaxIsland {
            path: names,
            tree: t,
        } => {
            path(v, names, e.span, depth + 1)?;
            tree(v, t, depth + 1)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SyntaxCost {
    pub nodes: usize,
    pub work: usize,
    pub generation: bool,
}
struct Counter<'a> {
    limits: &'a Limits,
    cost: SyntaxCost,
}
impl Visitor for Counter<'_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        if depth > self.limits.max_call_depth || self.cost.nodes >= self.limits.max_nodes {
            return Err(vec![Diagnostic::limit(
                Phase::Expand,
                "selection syntax node/depth budget exceeded",
                Some(span),
            )]);
        }
        self.cost.nodes += 1;
        self.text("x", span)
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.cost.work = self
            .cost
            .work
            .checked_add(text.len())
            .filter(|n| *n <= self.limits.max_analysis_steps)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Expand,
                    "selection syntax work budget exceeded",
                    Some(span),
                )]
            })?;
        Ok(())
    }
    fn item(&mut self, i: &s::Item, depth: usize) -> CompileResult<()> {
        self.cost.generation |= matches!(i.kind, s::ItemKind::Meta(_));
        walk_item(self, i, depth)
    }
    fn statement(&mut self, s: &s::Stmt, depth: usize) -> CompileResult<()> {
        self.cost.generation |= matches!(s.kind, s::StmtKind::Meta(_));
        walk_statement(self, s, depth)
    }
    fn expression(&mut self, e: &s::Expr, depth: usize) -> CompileResult<()> {
        self.cost.generation |= matches!(
            e.kind,
            s::ExprKind::MetaBlock(_) | s::ExprKind::SyntaxIsland { .. }
        );
        walk_expression(self, e, depth)
    }
    fn ty(&mut self, t: &s::TypeRef, depth: usize) -> CompileResult<()> {
        self.cost.generation |=
            matches!(t.kind, s::TypeKind::MetaBlock(_) | s::TypeKind::MetaEval(_));
        walk_type(self, t, depth)
    }
}
pub fn item_cost(item: &s::Item, limits: &Limits) -> CompileResult<SyntaxCost> {
    let mut c = Counter {
        limits,
        cost: SyntaxCost::default(),
    };
    c.item(item, 0)?;
    Ok(c.cost)
}
pub fn expression_cost(expression: &s::Expr, limits: &Limits) -> CompileResult<SyntaxCost> {
    let mut c = Counter {
        limits,
        cost: SyntaxCost::default(),
    };
    c.expression(expression, 0)?;
    Ok(c.cost)
}
pub fn block_cost(block: &s::Block, limits: &Limits) -> CompileResult<SyntaxCost> {
    let mut counter = Counter {
        limits,
        cost: SyntaxCost::default(),
    };
    counter.block(block, 0)?;
    Ok(counter.cost)
}
pub fn module_cost(module: &s::Module, limits: &Limits) -> CompileResult<SyntaxCost> {
    let mut c = Counter {
        limits,
        cost: SyntaxCost::default(),
    };
    c.node(module.span, 0)?;
    for item in &module.items {
        c.item(item, 1)?;
    }
    Ok(c.cost)
}

/// Reuses ordinary source lowering after bounding all borrowed nodes/strings.
/// The result remains raw Core and never establishes a bool, purity or safety.
pub fn expression(
    module: zil_base::ModuleId,
    expression: &s::Expr,
    limits: &Limits,
) -> CompileResult<CoreExpr> {
    expression_cost(expression, limits)?;
    let declarations = BTreeMap::new();
    let mut e = Expander::new(module, true, limits, None, &declarations);
    let result = e.expr(expression);
    if e.diagnostics.is_empty() {
        Ok(result)
    } else {
        Err(e.diagnostics)
    }
}

/// Untrusted expression plus the next unused raw identity, so generation query
/// wrappers leave a real bounded identity range for aggregate CT reification.
pub fn expression_with_next_id(
    module: zil_base::ModuleId,
    expression: &s::Expr,
    limits: &Limits,
) -> CompileResult<(CoreExpr, NodeId)> {
    expression_cost(expression, limits)?;
    let declarations = BTreeMap::new();
    let mut expander = Expander::new(module, true, limits, None, &declarations);
    let result = expander.expr(expression);
    if expander.diagnostics.is_empty() {
        Ok((result, NodeId(expander.next)))
    } else {
        Err(expander.diagnostics)
    }
}

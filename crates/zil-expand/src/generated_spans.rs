//! Call-site spans for final generated ordinary syntax, never a hygiene proof.
use super::*;
use zil_syntax::GeneratedSyntax;

/// Prepay both traversals before changing any span. Names and source-owned
/// quote/island token metadata stay intact; the driver retains their source.
/// This is for ordinary materialized products after CT binding, not a way to
/// authenticate a generated provider's quote against the insertion source.
pub fn remap_generated_spans(
    syntax: &mut GeneratedSyntax,
    insertion: Span,
    limits: &Limits,
) -> CompileResult<usize> {
    let mut pass = Mapper {
        insertion,
        limits,
        work: 0,
        nodes: 0,
        apply: false,
    };
    pass.syntax(syntax)?;
    let work = pass.work;
    pass.apply = true;
    pass.syntax(syntax)?;
    Ok(work)
}
struct Mapper<'a> {
    insertion: Span,
    limits: &'a Limits,
    work: usize,
    nodes: usize,
    apply: bool,
}
impl Mapper<'_> {
    fn syntax(&mut self, syntax: &mut GeneratedSyntax) -> CompileResult<()> {
        match syntax {
            GeneratedSyntax::Expr(e) => self.expression(e, 0),
            GeneratedSyntax::Type(t) => self.ty(t, 0),
            GeneratedSyntax::StmtList(ss) => {
                for s in ss {
                    self.statement(s, 0)?;
                }
                Ok(())
            }
            GeneratedSyntax::ItemList(ii) => {
                for i in ii {
                    self.item(i, 0)?;
                }
                Ok(())
            }
        }
    }
    fn charge(&mut self, amount: usize, span: Span) -> CompileResult<()> {
        if self.apply {
            return Ok(());
        }
        self.work = self
            .work
            .checked_add(amount)
            .filter(|w| *w <= self.limits.max_analysis_steps)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Expand,
                    "generated span remap work budget exceeded",
                    Some(span),
                )]
            })?;
        Ok(())
    }
}
impl Visitor for Mapper<'_> {
    fn node(&mut self, span: &mut Span, depth: usize) -> CompileResult<()> {
        if self.apply {
            *span = self.insertion;
            return Ok(());
        }
        if depth > self.limits.max_call_depth || self.nodes >= self.limits.max_nodes {
            return Err(vec![Diagnostic::limit(
                Phase::Expand,
                "generated span remap node/depth budget exceeded",
                Some(*span),
            )]);
        }
        self.nodes += 1;
        self.charge(2, *span)
    }
    fn text(&mut self, _value: &str, span: Span) -> CompileResult<()> {
        self.charge(2, span)
    }
}

trait Visitor {
    fn node(&mut self, span: &mut Span, depth: usize) -> CompileResult<()>;
    fn text(&mut self, value: &str, span: Span) -> CompileResult<()>;
    fn item(&mut self, item: &mut s::Item, depth: usize) -> CompileResult<()> {
        walk_item(self, item, depth)
    }
    fn block(&mut self, block: &mut s::Block, depth: usize) -> CompileResult<()> {
        walk_block(self, block, depth)
    }
    fn statement(&mut self, statement: &mut s::Stmt, depth: usize) -> CompileResult<()> {
        walk_statement(self, statement, depth)
    }
    fn expression(&mut self, expression: &mut s::Expr, depth: usize) -> CompileResult<()> {
        walk_expression(self, expression, depth)
    }
    fn ty(&mut self, ty: &mut s::TypeRef, depth: usize) -> CompileResult<()> {
        walk_type(self, ty, depth)
    }
}

fn path<V: Visitor + ?Sized>(
    v: &mut V,
    names: &[String],
    mut span: Span,
    depth: usize,
) -> CompileResult<()> {
    for name in names {
        v.node(&mut span, depth)?;
        v.text(name, span)?;
    }
    Ok(())
}
fn attributes<V: Visitor + ?Sized>(
    v: &mut V,
    values: &mut [s::Attribute],
    depth: usize,
) -> CompileResult<()> {
    for a in values {
        v.node(&mut a.span, depth)?;
        path(v, &a.path, a.span, depth + 1)?;
        if let Some(text) = &a.string_value {
            v.text(text, a.span)?;
        }
        for e in &mut a.arguments {
            v.expression(e, depth + 1)?;
        }
    }
    Ok(())
}
fn argument<V: Visitor + ?Sized>(
    v: &mut V,
    a: &mut s::GenericArg,
    depth: usize,
) -> CompileResult<()> {
    match a {
        s::GenericArg::Type(t) => v.ty(t, depth),
        s::GenericArg::Const(e) => v.expression(e, depth),
    }
}
fn generics<V: Visitor + ?Sized>(
    v: &mut V,
    gs: &mut [s::GenericParam],
    depth: usize,
) -> CompileResult<()> {
    for g in gs {
        v.node(&mut g.span, depth)?;
        v.text(&g.name, g.span)?;
        match &mut g.kind {
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
    bs: &mut [s::BoundClause],
    depth: usize,
) -> CompileResult<()> {
    for b in bs {
        v.node(&mut b.span, depth)?;
        match &mut b.subject {
            s::BoundSubject::Type(t) => v.ty(t, depth + 1)?,
            s::BoundSubject::Lifetime(n) => v.text(n, b.span)?,
        }
        for t in &mut b.bounds {
            v.ty(t, depth + 1)?;
        }
        if let Some(n) = &b.lifetime_bound {
            v.text(n, b.span)?;
        }
    }
    Ok(())
}
fn term<V: Visitor + ?Sized>(
    v: &mut V,
    t: &mut s::ContractTerm,
    depth: usize,
) -> CompileResult<()> {
    v.node(&mut t.span, depth)?;
    path(v, &t.path, t.span, depth + 1)?;
    for a in &mut t.type_args {
        argument(v, a, depth + 1)?;
    }
    for a in &mut t.arguments {
        term(v, a, depth + 1)?;
    }
    if let Some(i) = &mut t.index {
        term(v, i, depth + 1)?;
    }
    Ok(())
}
fn parameter<V: Visitor + ?Sized>(v: &mut V, p: &mut s::Param, depth: usize) -> CompileResult<()> {
    v.node(&mut p.span, depth)?;
    v.text(&p.name, p.span)?;
    attributes(v, &mut p.attributes, depth + 1)?;
    v.ty(&mut p.ty, depth + 1)
}
fn function<V: Visitor + ?Sized>(
    v: &mut V,
    f: &mut s::Function,
    depth: usize,
) -> CompileResult<()> {
    v.node(&mut f.span, depth)?;
    v.text(&f.name, f.span)?;
    generics(v, &mut f.generics, depth + 1)?;
    for p in &mut f.params {
        parameter(v, p, depth + 1)?;
    }
    if let Some(t) = &mut f.return_type {
        v.ty(t, depth + 1)?;
    }
    bounds(v, &mut f.where_clause, depth + 1)?;
    for c in &mut f.contract {
        v.node(&mut c.span, depth + 1)?;
        v.text(&c.name, c.span)?;
        for t in &mut c.terms {
            term(v, t, depth + 2)?;
        }
    }
    match &mut f.body {
        s::FunctionBody::Block(b) => v.block(b, depth + 1),
        s::FunctionBody::Expression(e) => v.expression(e, depth + 1),
        s::FunctionBody::Declaration => Ok(()),
    }
}
fn field<V: Visitor + ?Sized>(v: &mut V, f: &mut s::Field, depth: usize) -> CompileResult<()> {
    v.node(&mut f.span, depth)?;
    v.text(&f.name, f.span)?;
    attributes(v, &mut f.attributes, depth + 1)?;
    v.ty(&mut f.ty, depth + 1)?;
    if let Some(e) = &mut f.default {
        v.expression(e, depth + 1)?;
    }
    Ok(())
}
fn walk_item<V: Visitor + ?Sized>(
    v: &mut V,
    item: &mut s::Item,
    depth: usize,
) -> CompileResult<()> {
    v.node(&mut item.span, depth)?;
    attributes(v, &mut item.attributes, depth + 1)?;
    match &mut item.kind {
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
            generics(v, &mut a.generics, depth + 1)?;
            for f in &mut a.fields {
                field(v, f, depth + 1)?;
            }
            for m in &mut a.methods {
                v.item(m, depth + 1)?;
            }
        }
        s::ItemKind::Enum(e) => {
            v.text(&e.name, item.span)?;
            generics(v, &mut e.generics, depth + 1)?;
            for a in &mut e.variants {
                v.node(&mut a.span, depth + 1)?;
                v.text(&a.name, a.span)?;
                attributes(v, &mut a.attributes, depth + 2)?;
                match &mut a.fields {
                    s::VariantFields::Unit => {}
                    s::VariantFields::Tuple(ts) => {
                        for t in ts {
                            v.node(&mut t.span, depth + 2)?;
                            v.ty(&mut t.ty, depth + 2)?;
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
            generics(v, &mut t.generics, depth + 1)?;
            for b in &mut t.bounds {
                v.ty(b, depth + 1)?;
            }
            for m in &mut t.members {
                v.item(m, depth + 1)?;
            }
        }
        s::ItemKind::Impl(i) => {
            generics(v, &mut i.generics, depth + 1)?;
            v.ty(&mut i.target, depth + 1)?;
            if let Some(t) = &mut i.trait_type {
                v.ty(t, depth + 1)?;
            }
            if let Some(p) = &mut i.receiver_default {
                parameter(v, p, depth + 1)?;
            }
            bounds(v, &mut i.where_clause, depth + 1)?;
            for m in &mut i.members {
                v.item(m, depth + 1)?;
            }
        }
        s::ItemKind::TypeAlias(a) => {
            v.text(&a.name, item.span)?;
            generics(v, &mut a.generics, depth + 1)?;
            for t in &mut a.bounds {
                v.ty(t, depth + 1)?;
            }
            if let Some(t) = &mut a.value {
                v.ty(t, depth + 1)?;
            }
        }
        s::ItemKind::Const(c) => {
            v.text(&c.name, item.span)?;
            if let Some(t) = &mut c.ty {
                v.ty(t, depth + 1)?;
            }
            if let Some(e) = &mut c.value {
                v.expression(e, depth + 1)?;
            }
        }
        s::ItemKind::Meta(s::MetaItem::If {
            condition,
            then_items,
            else_items,
        }) => {
            v.expression(condition, depth + 1)?;
            for i in then_items.iter_mut().chain(else_items) {
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
fn walk_type<V: Visitor + ?Sized>(
    v: &mut V,
    t: &mut s::TypeRef,
    depth: usize,
) -> CompileResult<()> {
    v.node(&mut t.span, depth)?;
    match &mut t.kind {
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
        s::TypeKind::MetaEval(e) | s::TypeKind::MetaSplice(e) => v.expression(e, depth + 1)?,
        s::TypeKind::MetaBlock(b) => v.block(b, depth + 1)?,
    }
    Ok(())
}
fn walk_pattern<V: Visitor + ?Sized>(
    v: &mut V,
    p: &mut s::Pattern,
    depth: usize,
) -> CompileResult<()> {
    v.node(&mut p.span, depth)?;
    match &mut p.kind {
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
fn condition<V: Visitor + ?Sized>(
    v: &mut V,
    c: &mut s::Condition,
    depth: usize,
) -> CompileResult<()> {
    match c {
        s::Condition::Expression(e) => v.expression(e, depth),
        s::Condition::Let { pattern, value } => {
            walk_pattern(v, pattern, depth)?;
            v.expression(value, depth)
        }
    }
}
fn walk_block<V: Visitor + ?Sized>(v: &mut V, b: &mut s::Block, depth: usize) -> CompileResult<()> {
    v.node(&mut b.span, depth)?;
    for s in &mut b.statements {
        v.statement(s, depth + 1)?;
    }
    Ok(())
}
fn walk_statement<V: Visitor + ?Sized>(
    v: &mut V,
    s: &mut s::Stmt,
    depth: usize,
) -> CompileResult<()> {
    v.node(&mut s.span, depth)?;
    match &mut s.kind {
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
                v.node(&mut a.span, depth + 1)?;
                walk_pattern(v, &mut a.pattern, depth + 2)?;
                arrow(v, &mut a.body, depth + 2)?;
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
fn arrow<V: Visitor + ?Sized>(v: &mut V, a: &mut s::ArrowBody, depth: usize) -> CompileResult<()> {
    match a {
        s::ArrowBody::Expression(e) => v.expression(e, depth),
        s::ArrowBody::Block(b) => v.block(b, depth),
    }
}
fn literal<V: Visitor + ?Sized>(v: &mut V, l: &mut s::Literal, span: Span) -> CompileResult<()> {
    match l {
        s::Literal::Integer(n)
        | s::Literal::Float(n)
        | s::Literal::String(n)
        | s::Literal::Char(n) => v.text(n, span),
        s::Literal::Bool(_) => Ok(()),
    }
}
fn walk_expression<V: Visitor + ?Sized>(
    v: &mut V,
    e: &mut s::Expr,
    depth: usize,
) -> CompileResult<()> {
    v.node(&mut e.span, depth)?;
    match &mut e.kind {
        s::ExprKind::Literal(l) => literal(v, l, e.span)?,
        s::ExprKind::Name(n) | s::ExprKind::ReceiverMember(n) => v.text(n, e.span)?,
        s::ExprKind::Group(e)
        | s::ExprKind::Try(e)
        | s::ExprKind::MetaEval(e)
        | s::ExprKind::MetaSplice(e)
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
                v.node(&mut a.span, depth + 1)?;
                v.expression(&mut a.value, depth + 2)?;
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
                v.node(&mut f.span, depth + 1)?;
                v.text(&f.name, f.span)?;
                if let Some(e) = &mut f.value {
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
                v.node(&mut p.span, depth + 1)?;
                v.text(&p.name, p.span)?;
                if let Some(t) = &mut p.ty {
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
        s::ExprKind::Quote { .. } | s::ExprKind::SyntaxIsland { .. } => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId};
    use zil_syntax::{
        GeneratedParseContext, GeneratedTokenContext, ast::SyntaxCategory, token::TokenKind,
    };
    fn fragment(category: SyntaxCategory, text: &str) -> GeneratedSyntax {
        let source = SourceUnit::new(FileId(8), ModuleId(3), "generated.zil", text);
        let context = GeneratedParseContext {
            tokens: zil_syntax::lexer::lex(&source)
                .tokens
                .into_iter()
                .filter(|t| t.kind != TokenKind::Eof)
                .map(|t| GeneratedTokenContext {
                    span: t.span,
                    origin: 7,
                    name_context: None,
                })
                .collect(),
        };
        zil_syntax::parse_generated(&source, category, &context, &Limits::default())
            .into_result()
            .unwrap()
            .syntax
            .unwrap()
    }
    struct Check(Span);
    impl ct_selection::Visitor for Check {
        fn node(&mut self, span: Span, _: usize) -> CompileResult<()> {
            assert_eq!(span, self.0);
            Ok(())
        }
        fn text(&mut self, _: &str, _: Span) -> CompileResult<()> {
            Ok(())
        }
    }
    #[test]
    fn all_generated_categories_remap_nested_spans_and_keep_spelling() {
        use ct_selection::Visitor;
        let insertion = Span::new(FileId(0), 10, 20);
        for (category, text) in [
            (
                SyntaxCategory::Expr,
                "move(x:i32)->i32=>{let y:[i32;2]=[x,2];return y[0]}",
            ),
            (SyntaxCategory::Type, "fn([i32;2])->i32"),
            (
                SyntaxCategory::StmtList,
                "if let (a,b)=(1,2){defer{work(a)};work(b)}else{loop{break}}",
            ),
            (
                SyntaxCategory::ItemList,
                "enum E{A(i32),B{x:i32}}\nfn f<T>(x:T)->T{return x}",
            ),
        ] {
            let mut syntax = fragment(category, text);
            assert!(remap_generated_spans(&mut syntax, insertion, &Limits::default()).unwrap() > 0);
            let mut check = Check(insertion);
            match &syntax {
                GeneratedSyntax::Expr(e) => check.expression(e, 0).unwrap(),
                GeneratedSyntax::Type(t) => check.ty(t, 0).unwrap(),
                GeneratedSyntax::StmtList(ss) => {
                    for s in ss {
                        check.statement(s, 0).unwrap()
                    }
                }
                GeneratedSyntax::ItemList(ii) => {
                    for i in ii {
                        check.item(i, 0).unwrap()
                    }
                    let s::ItemKind::Enum(e) = &ii[0].kind else {
                        panic!()
                    };
                    let s::VariantFields::Tuple(ts) = &e.variants[0].fields else {
                        panic!()
                    };
                    assert_eq!(ts[0].span, insertion);
                    assert_eq!(e.name, "E");
                }
            }
        }
    }
    #[test]
    fn full_preflight_is_atomic_at_exact_work_node_and_depth_limits() {
        let original = fragment(SyntaxCategory::Expr, "f(a+1,b[0])");
        let insertion = Span::new(FileId(0), 10, 20);
        let n =
            remap_generated_spans(&mut original.clone(), insertion, &Limits::default()).unwrap();
        let exact = Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let mut copy = original.clone();
        assert_eq!(
            remap_generated_spans(&mut copy, insertion, &exact).unwrap(),
            n
        );
        for limits in [
            Limits {
                max_analysis_steps: n - 1,
                ..Limits::default()
            },
            Limits {
                max_nodes: 1,
                ..Limits::default()
            },
            Limits {
                max_call_depth: 1,
                ..Limits::default()
            },
        ] {
            let mut copy = original.clone();
            let errors = remap_generated_spans(&mut copy, insertion, &limits).unwrap_err();
            assert_eq!(errors[0].code, "E-RESOURCE-LIMIT");
            assert_eq!(copy, original);
        }
        assert_eq!(
            remap_generated_spans(&mut original.clone(), insertion, &exact).unwrap(),
            n
        );
    }
    #[test]
    fn quote_and_island_metadata_keep_their_registered_source() {
        let insertion = Span::new(FileId(0), 10, 20);
        for text in ["quote<Expr>{name+$(value)}", "provider!(name)"] {
            let mut syntax = fragment(SyntaxCategory::Expr, text);
            let GeneratedSyntax::Expr(original) = syntax.clone() else {
                panic!()
            };
            remap_generated_spans(&mut syntax, insertion, &Limits::default()).unwrap();
            let GeneratedSyntax::Expr(mapped) = syntax else {
                panic!()
            };
            assert_eq!(mapped.kind, original.kind);
            assert_eq!(mapped.span, insertion);
        }
    }
}

use super::*;
use cfg::{Builder, Value};
fn category(c: s::SyntaxCategory) -> CtSyntaxCategory {
    match c {
        s::SyntaxCategory::Expr => CtSyntaxCategory::Expr,
        s::SyntaxCategory::Type => CtSyntaxCategory::Type,
        s::SyntaxCategory::StmtList => CtSyntaxCategory::StmtList,
        s::SyntaxCategory::ItemList => CtSyntaxCategory::ItemList,
    }
}
fn namespace(n: s::QuoteNameNamespace) -> CtNameNamespace {
    match n {
        s::QuoteNameNamespace::Value => CtNameNamespace::Value,
        s::QuoteNameNamespace::Type => CtNameNamespace::Type,
        s::QuoteNameNamespace::Member => CtNameNamespace::Member,
        s::QuoteNameNamespace::Module => CtNameNamespace::Module,
        s::QuoteNameNamespace::Label => CtNameNamespace::Label,
    }
}
fn delimiter(d: s::Delimiter) -> CtDelimiter {
    match d {
        s::Delimiter::Brace => CtDelimiter::Brace,
        s::Delimiter::Bracket => CtDelimiter::Bracket,
        s::Delimiter::Parenthesis => CtDelimiter::Parenthesis,
    }
}
fn class(text: &str) -> CtTokenClass {
    match text.chars().next() {
        Some('"' | '\'') => CtTokenClass::String,
        Some('0'..='9') => CtTokenClass::Integer,
        Some(c) if c == '_' || zil_syntax::unicode::is_xid_start(c) => CtTokenClass::Identifier,
        _ => CtTokenClass::Punctuation,
    }
}
struct TemplateMeter<'a, 'b> {
    budget: &'a mut Budget<'b>,
    depth: usize,
}
impl zil_expand::ct_selection::Visitor for TemplateMeter<'_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> R<()> {
        self.budget.depth(self.depth.saturating_add(depth), span)?;
        self.budget.work(1, span)
    }
    fn text(&mut self, text: &str, span: Span) -> R<()> {
        self.budget.work(text.len(), span)
    }
}
impl Builder<'_, '_> {
    fn generated_template(
        &mut self,
        c: s::SyntaxCategory,
        template: &s::QuoteTemplate,
        depth: usize,
        span: Span,
    ) -> R<bool> {
        if self.definition.generation_path.is_none() {
            return Ok(false);
        }
        let site = self.compiler.current_expression_site(span)?;
        if !matches!(site.tree, source_sites::SourceTreeId::Generated(_)) {
            return Err(error(
                span,
                "generated quote has no actual generated source site",
            ));
        }
        let (tree, module) = self.compiler.frozen_site_module(site, span)?;
        let original = source_sites::select_in_tree(
            module,
            tree,
            source_sites::Selection::Site(site),
            self.compiler.budget.limits.max_call_depth,
            |work| self.compiler.budget.work(work, span),
        )?
        .ok_or_else(|| error(span, "generated quote source node is absent"))?;
        let source_sites::SourceNode::Expr(s::Expr {
            kind:
                s::ExprKind::Quote {
                    category,
                    template: actual,
                },
            ..
        }) = original.node
        else {
            return Err(error(
                span,
                "generated quote source node has a different actual kind",
            ));
        };
        // Output AST spans can name the emission rather than the original file
        // token bytes. Authenticate the complete template through the immutable
        // generated tree and real structural SourceSite instead of a span.
        let mut meter = TemplateMeter {
            budget: &mut self.compiler.budget,
            depth,
        };
        zil_expand::ct_selection::walk_quote_template(&mut meter, template, 0)?;
        zil_expand::ct_selection::walk_quote_template(&mut meter, actual, 0)?;
        if c != *category || template != actual {
            return Err(error(
                span,
                "generated quote differs from its complete frozen source template",
            ));
        }
        Ok(true)
    }
    pub(super) fn quote(
        &mut self,
        c: s::SyntaxCategory,
        template: &s::QuoteTemplate,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        let generated = self.generated_template(c, template, depth, span)?;
        let mut kinds = Vec::new();
        let mut holes = Vec::new();
        for hole in &template.holes {
            self.compiler.budget.node(hole.span)?;
            let (kind, ty) = match hole.kind {
                s::QuoteHoleKind::Expr => (CtHoleKind::Expr, CtTy::Syntax(CtSyntaxCategory::Expr)),
                s::QuoteHoleKind::Type => (CtHoleKind::Type, CtTy::Syntax(CtSyntaxCategory::Type)),
                s::QuoteHoleKind::StmtList => (
                    CtHoleKind::StmtList,
                    CtTy::Syntax(CtSyntaxCategory::StmtList),
                ),
                s::QuoteHoleKind::ItemList => (
                    CtHoleKind::ItemList,
                    CtTy::Syntax(CtSyntaxCategory::ItemList),
                ),
                s::QuoteHoleKind::Ident { namespace: n, .. } => {
                    (CtHoleKind::Ident(namespace(n)), CtTy::Ident)
                }
            };
            let value = self.expr(&hole.value, None, depth + 1)?;
            self.compiler.budget.ty(&value.ty, 0, hole.span)?;
            if value.ty != ty {
                return Err(category_error(
                    hole.span,
                    "quote hole value has the wrong actual Syntax/Ident category",
                ));
            }
            kinds.push(kind);
            holes.push(value.id);
        }
        let mut syntax = CtSyntaxTree {
            category: category(c),
            nodes: Vec::new(),
            roots: Vec::new(),
            origins: Vec::new(),
        };
        for entry in &template.tree.children {
            let node = self.quote_node(entry, &mut syntax, kinds.len(), generated, depth + 1)?;
            self.compiler.budget.node(span)?;
            syntax.roots.push(node);
        }
        self.compiler.budget.node(span)?;
        let id = CtTemplateId(self.compiler.templates.len() as u32);
        self.compiler.templates.push(CtTemplate {
            definition: self.definition.clone(),
            syntax,
            holes: kinds,
        });
        self.emit(
            CtInstructionKind::Quote {
                template: id,
                holes,
            },
            CtTy::Syntax(category(c)),
            span,
        )
    }
    fn quote_node(
        &mut self,
        entry: &s::QuoteTokenTreeEntry,
        tree: &mut CtSyntaxTree,
        holes: usize,
        generated: bool,
        depth: usize,
    ) -> R<CtSyntaxNodeId> {
        let span = match entry {
            s::QuoteTokenTreeEntry::Token { span, .. }
            | s::QuoteTokenTreeEntry::Hole { span, .. } => *span,
            s::QuoteTokenTreeEntry::Tree(t) => t.span,
        };
        self.compiler.budget.depth(depth, span)?;
        self.compiler.budget.node(span)?;
        let origin = CtOriginId(tree.origins.len() as u32);
        tree.origins.push(CtOrigin {
            span,
            parent: None,
            role: CtOriginRole::QuoteDefinition,
        });
        let node = match entry {
            s::QuoteTokenTreeEntry::Token { text, span } => {
                self.compiler.budget.text(text, *span)?;
                let source = self.compiler.modules[self.module].source;
                if !generated
                    && (!span.is_valid_for(source)
                        || source.text.get(span.start..span.end) != Some(text.as_str()))
                {
                    return Err(error(
                        *span,
                        "quote token does not match its actual source range",
                    ));
                }
                CtSyntaxNode::Token(CtToken {
                    class: class(text),
                    text: text.clone(),
                    origin,
                    name: None,
                })
            }
            s::QuoteTokenTreeEntry::Hole { index, .. } => {
                if *index as usize >= holes {
                    return Err(error(
                        span,
                        "quote hole index is outside its actual hole table",
                    ));
                }
                CtSyntaxNode::Hole {
                    index: *index,
                    origin,
                }
            }
            s::QuoteTokenTreeEntry::Tree(group) => {
                let mut children = Vec::new();
                for entry in &group.children {
                    let child = self.quote_node(entry, tree, holes, generated, depth + 1)?;
                    self.compiler.budget.node(span)?;
                    children.push(child);
                }
                CtSyntaxNode::Group {
                    delimiter: delimiter(group.delimiter),
                    children,
                    origin,
                }
            }
        };
        self.compiler.budget.node(span)?;
        let id = CtSyntaxNodeId(tree.nodes.len() as u32);
        tree.nodes.push(node);
        Ok(id)
    }
}

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
impl Builder<'_, '_> {
    pub(super) fn quote(
        &mut self,
        c: s::SyntaxCategory,
        template: &s::QuoteTemplate,
        depth: usize,
        span: Span,
    ) -> R<Value> {
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
            let value = self.expr(&hole.value, Some(&ty), depth + 1)?;
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
            let node = self.quote_node(entry, &mut syntax, kinds.len(), depth + 1)?;
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
                if !span.is_valid_for(source)
                    || source.text.get(span.start..span.end) != Some(text.as_str())
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
                    let child = self.quote_node(entry, tree, holes, depth + 1)?;
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

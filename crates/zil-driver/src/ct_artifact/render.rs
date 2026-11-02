//! Lossless, bounded generated text for the ordinary parser and preview. Token
//! origin/name handles are retained separately; text alone is never authority.
use zil_base::{CompileResult, Diagnostic, FileId, Phase, SourceUnit, Span};
use zil_ct::CtExecutionBudget;
use zil_ir::ct_artifact::{CtDelimiter, CtIdent, CtOriginId, CtSyntaxNode, CtSyntaxNodeId, CtSyntaxTree};

#[derive(Clone, Debug)]
pub(super) struct RenderedToken {
    pub span: Span,
    pub origin: CtOriginId,
    pub name: Option<CtIdent>,
}

#[derive(Clone, Debug)]
pub(super) struct RenderedSyntax {
    pub source: SourceUnit,
    pub tokens: Vec<RenderedToken>,
}

fn error(message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-CATEGORY", None, message)]
}

struct Renderer<'a> {
    tree: &'a CtSyntaxTree,
    file: FileId,
    text: String,
    tokens: Vec<RenderedToken>,
    remaining: usize,
}
impl Renderer<'_> {
    fn write(&mut self, text: &str, origin: CtOriginId, name: Option<CtIdent>) -> CompileResult<()> {
        self.remaining = self.remaining.checked_sub(text.len().saturating_add(1)).ok_or_else(|| error("generated syntax text exceeds reserved size"))?;
        let start = self.text.len();
        self.text.push_str(text);
        let end = self.text.len();
        self.text.push(' ');
        if !text.is_empty() { self.tokens.push(RenderedToken { span: Span::new(self.file, start, end), origin, name }); }
        Ok(())
    }
    fn node(&mut self, id: CtSyntaxNodeId, depth: usize) -> CompileResult<()> {
        if depth > self.tree.nodes.len() { return Err(error("generated syntax node cycle")); }
        match self.tree.nodes.get(id.0 as usize).ok_or_else(|| error("generated syntax node is out of range"))? {
            CtSyntaxNode::Token(token) => {
                if token.origin.0 as usize >= self.tree.origins.len() { return Err(error("generated token origin is out of range")); }
                self.write(&token.text, token.origin, token.name.clone())
            }
            CtSyntaxNode::Group { delimiter, children, origin } => {
                if origin.0 as usize >= self.tree.origins.len() { return Err(error("generated group origin is out of range")); }
                let (open, close) = match delimiter {
                    CtDelimiter::Parenthesis => ("(", ")"),
                    CtDelimiter::Bracket => ("[", "]"),
                    CtDelimiter::Brace => ("{", "}"),
                };
                self.write(open, *origin, None)?;
                for child in children { self.node(*child, depth + 1)?; }
                self.write(close, *origin, None)
            }
            CtSyntaxNode::Hole { .. } => Err(error("evaluated syntax still contains a quote hole")),
        }
    }
}

pub(super) fn render(tree: &CtSyntaxTree, file: FileId, module: zil_base::ModuleId, budget: &mut CtExecutionBudget) -> CompileResult<RenderedSyntax> {
    let mut bytes = 0usize;
    for node in &tree.nodes {
        bytes = bytes.checked_add(match node {
            CtSyntaxNode::Token(token) => token.text.len().saturating_add(1),
            CtSyntaxNode::Group { .. } => 4,
            CtSyntaxNode::Hole { .. } => return Err(error("evaluated syntax still contains a quote hole")),
        }).ok_or_else(|| error("generated syntax text size overflow"))?;
    }
    budget.reserve_analysis_work(tree.nodes.len() as u64)?;
    budget.reserve_temporary_bytes(bytes as u64)?;
    let mut renderer = Renderer { tree, file, text: String::with_capacity(bytes), tokens: Vec::new(), remaining: bytes };
    for id in &tree.roots { renderer.node(*id, 0)?; }
    Ok(RenderedSyntax { source: SourceUnit::new(file, module, "<ct-generated>", renderer.text), tokens: renderer.tokens })
}

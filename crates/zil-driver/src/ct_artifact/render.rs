//! Lossless, bounded generated text for the ordinary parser and preview. Token
//! origin/name handles are retained separately; text alone is never authority.
use zil_base::{CompileResult, Diagnostic, FileId, Limits, Phase, SourceUnit, Span};
use zil_ct::CtExecutionBudget;
use zil_ir::ct_artifact::{CtDelimiter, CtIdent, CtOriginId, CtSyntaxNode, CtSyntaxNodeId, CtSyntaxTree};
use zil_syntax::{GeneratedParseContext, GeneratedParseResult, GeneratedTokenContext, ast::SyntaxCategory};

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

pub(super) struct ParsedSyntax {
    pub rendered: RenderedSyntax,
    pub parsed: GeneratedParseResult,
    pub names: Vec<CtIdent>,
}

fn category(value: zil_ir::ct_artifact::CtSyntaxCategory) -> SyntaxCategory {
    match value {
        zil_ir::ct_artifact::CtSyntaxCategory::Expr => SyntaxCategory::Expr,
        zil_ir::ct_artifact::CtSyntaxCategory::Type => SyntaxCategory::Type,
        zil_ir::ct_artifact::CtSyntaxCategory::StmtList => SyntaxCategory::StmtList,
        zil_ir::ct_artifact::CtSyntaxCategory::ItemList => SyntaxCategory::ItemList,
    }
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
    max_depth: usize,
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
        if depth > self.max_depth || depth > self.tree.nodes.len() { return Err(error("generated syntax nesting exceeds budget")); }
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
    let mut renderer = Renderer { tree, file, text: String::with_capacity(bytes), tokens: Vec::new(), remaining: bytes,
        max_depth: budget.remaining().max_call_depth as usize };
    for id in &tree.roots { renderer.node(*id, 0)?; }
    Ok(RenderedSyntax { source: SourceUnit::new(file, module, "<ct-generated>", renderer.text), tokens: renderer.tokens })
}

pub(super) fn parse(tree: &CtSyntaxTree, file: FileId, module: zil_base::ModuleId, limits: &Limits, budget: &mut CtExecutionBudget) -> CompileResult<ParsedSyntax> {
    let rendered = render(tree, file, module, budget)?;
    let mut names = Vec::new();
    let mut context = GeneratedParseContext { tokens: Vec::with_capacity(rendered.tokens.len()) };
    for token in &rendered.tokens {
        let name_context = match &token.name {
            Some(name) => {
                let index = u32::try_from(names.len()).map_err(|_| error("generated name context index overflow"))?;
                names.push(name.clone());
                Some(index)
            }
            None => None,
        };
        context.tokens.push(GeneratedTokenContext { span: token.span, origin: token.origin.0, name_context });
    }
    // Reserve before lexing/parsing. The syntactic pass is linear in this
    // bounded token tape plus depth and is charged from the same CT balance.
    let work = rendered.source.text.len().saturating_mul(16).saturating_add(context.tokens.len().saturating_mul(16));
    budget.reserve_analysis_work(work as u64)?;
    let parsed = zil_syntax::parse_generated(&rendered.source, category(tree.category), &context, limits).into_result()?;
    Ok(ParsedSyntax { rendered, parsed, names })
}

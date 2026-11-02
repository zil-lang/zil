//! Lossless, bounded generated text for the ordinary parser and preview. Token
//! origin/name handles are retained separately; text alone is never authority.
use std::collections::BTreeMap;
use zil_base::{CompileResult, Diagnostic, FileId, Limits, ModuleId, Phase, SourceUnit, Span};
use zil_ct::CtExecutionBudget;
use zil_ir::ct_artifact::{
    CtDelimiter, CtIdent, CtNameNamespace, CtNameSource, CtOriginId, CtSyntaxNode, CtSyntaxNodeId,
    CtSyntaxTree,
};
use zil_syntax::{
    GeneratedParseContext, GeneratedParseResult, GeneratedTokenContext, ast::SyntaxCategory,
};

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
    module: ModuleId,
    aliases: &'a BTreeMap<ModuleId, String>,
}
impl Renderer<'_> {
    fn spelling(token: &zil_ir::ct_artifact::CtToken) -> String {
        if let Some(CtIdent {
            source: CtNameSource::Fresh { invocation, binder },
            ..
        }) = &token.name
        {
            return format!("__zil_ct_f{}_{}", &invocation.to_hex()[..16], binder);
        }
        token.text.clone()
    }
    fn alias_for(&self, name: Option<&CtIdent>) -> CompileResult<Option<String>> {
        let Some(CtIdent {
            source: CtNameSource::Definition(symbol),
            namespace: CtNameNamespace::Type | CtNameNamespace::Value,
            ..
        }) = name
        else {
            return Ok(None);
        };
        if symbol.module == self.module {
            return Ok(None);
        }
        self.aliases
            .get(&symbol.module)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                error("definition-site name has no sealed import in the generated caller module")
            })
    }
    fn write(
        &mut self,
        text: &str,
        origin: CtOriginId,
        name: Option<CtIdent>,
    ) -> CompileResult<()> {
        self.remaining = self
            .remaining
            .checked_sub(text.len().saturating_add(1))
            .ok_or_else(|| error("generated syntax text exceeds reserved size"))?;
        let start = self.text.len();
        self.text.push_str(text);
        let end = self.text.len();
        self.text.push(' ');
        // The renderer inserts separating spaces. Adjacent source whitespace
        // then lexes as one trivia token, so it has no independent token span.
        // Newlines remain semantic tokens and retain their exact provenance.
        if !text.is_empty() && (!text.trim().is_empty() || text.contains('\n')) {
            self.tokens.push(RenderedToken {
                span: Span::new(self.file, start, end),
                origin,
                name,
            });
        }
        Ok(())
    }
    fn node(&mut self, id: CtSyntaxNodeId, depth: usize) -> CompileResult<()> {
        if depth > self.max_depth || depth > self.tree.nodes.len() {
            return Err(error("generated syntax nesting exceeds budget"));
        }
        match self
            .tree
            .nodes
            .get(id.0 as usize)
            .ok_or_else(|| error("generated syntax node is out of range"))?
        {
            CtSyntaxNode::Token(token) => {
                if token.origin.0 as usize >= self.tree.origins.len() {
                    return Err(error("generated token origin is out of range"));
                }
                if let Some(alias) = self.alias_for(token.name.as_ref())? {
                    self.write(&alias, token.origin, None)?;
                    self.write(".", token.origin, None)?;
                }
                let spelling = Self::spelling(token);
                let mut name = token.name.clone();
                if let Some(name) = &mut name {
                    name.text = spelling.clone();
                }
                self.write(&spelling, token.origin, name)
            }
            CtSyntaxNode::Group {
                delimiter,
                children,
                origin,
            } => {
                if origin.0 as usize >= self.tree.origins.len() {
                    return Err(error("generated group origin is out of range"));
                }
                let (open, close) = match delimiter {
                    CtDelimiter::Parenthesis => ("(", ")"),
                    CtDelimiter::Bracket => ("[", "]"),
                    CtDelimiter::Brace => ("{", "}"),
                };
                self.write(open, *origin, None)?;
                for child in children {
                    self.node(*child, depth + 1)?;
                }
                self.write(close, *origin, None)
            }
            CtSyntaxNode::Hole { .. } => Err(error("evaluated syntax still contains a quote hole")),
        }
    }
}

pub(super) fn render(
    tree: &CtSyntaxTree,
    file: FileId,
    module: ModuleId,
    aliases: &BTreeMap<ModuleId, String>,
    budget: &mut CtExecutionBudget,
) -> CompileResult<RenderedSyntax> {
    let mut bytes = 0usize;
    for node in &tree.nodes {
        bytes = bytes.checked_add(match node {
            CtSyntaxNode::Token(token) => {
                let extra = if let Some(CtIdent { source: CtNameSource::Definition(symbol), namespace: CtNameNamespace::Type | CtNameNamespace::Value, .. }) = &token.name {
                    if symbol.module != module { aliases.get(&symbol.module).ok_or_else(|| error("definition-site name has no sealed import in the generated caller module"))?.len().saturating_add(3) } else { 0 }
                } else { 0 };
                Renderer::spelling(token).len().saturating_add(1).saturating_add(extra)
            },
            CtSyntaxNode::Group { .. } => 4,
            CtSyntaxNode::Hole { .. } => return Err(error("evaluated syntax still contains a quote hole")),
        }).ok_or_else(|| error("generated syntax text size overflow"))?;
    }
    budget.reserve_analysis_work(tree.nodes.len() as u64)?;
    budget.reserve_temporary_bytes(bytes as u64)?;
    let mut renderer = Renderer {
        tree,
        file,
        text: String::with_capacity(bytes),
        tokens: Vec::new(),
        remaining: bytes,
        max_depth: budget.remaining().max_call_depth as usize,
        module,
        aliases,
    };
    for id in &tree.roots {
        renderer.node(*id, 0)?;
    }
    Ok(RenderedSyntax {
        source: SourceUnit::new(file, module, "<ct-generated>", renderer.text),
        tokens: renderer.tokens,
    })
}

pub(super) fn parse(
    tree: &CtSyntaxTree,
    file: FileId,
    module: ModuleId,
    aliases: &BTreeMap<ModuleId, String>,
    limits: &Limits,
    budget: &mut CtExecutionBudget,
) -> CompileResult<ParsedSyntax> {
    let rendered = render(tree, file, module, aliases, budget)?;
    let mut name_count = 0usize;
    let mut context = GeneratedParseContext {
        tokens: Vec::with_capacity(rendered.tokens.len()),
    };
    for token in &rendered.tokens {
        let name_context = match &token.name {
            Some(_) => {
                let index = u32::try_from(name_count)
                    .map_err(|_| error("generated name context index overflow"))?;
                name_count = name_count
                    .checked_add(1)
                    .ok_or_else(|| error("generated name context index overflow"))?;
                Some(index)
            }
            None => None,
        };
        context.tokens.push(GeneratedTokenContext {
            span: token.span,
            origin: token.origin.0,
            name_context,
        });
    }
    // Reserve before lexing/parsing. The syntactic pass is linear in this
    // bounded token tape plus depth and is charged from the same CT balance.
    let work = rendered
        .source
        .text
        .len()
        .saturating_mul(16)
        .saturating_add(context.tokens.len().saturating_mul(16));
    budget.reserve_analysis_work(work as u64)?;
    let parsed =
        zil_syntax::parse_generated(&rendered.source, category(tree.category), &context, limits)
            .into_result()?;
    Ok(ParsedSyntax { rendered, parsed })
}

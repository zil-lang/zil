//! Authenticate provider input against the current lossless parsed source.
//! The tree includes its outer envelope; parse APIs remove that layer once.
use zil_base::{CompileResult, Diagnostic, Fingerprint, Limits, Phase, SourceUnit, Span};
use zil_ir::ct_artifact::*;
use zil_syntax::{ast as s, token::{Token, TokenKind}};

fn error(message: impl Into<String>, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-INPUT", Some(span), message)]
}

fn delimiter(value: s::Delimiter) -> CtDelimiter {
    match value {
        s::Delimiter::Parenthesis => CtDelimiter::Parenthesis,
        s::Delimiter::Bracket => CtDelimiter::Bracket,
        s::Delimiter::Brace => CtDelimiter::Brace,
    }
}

fn keyword(text: &str) -> bool {
    matches!(text, "let" | "var" | "fn" | "pub" | "struct" | "enum" | "resource" | "package" | "impl" | "trait" | "type" | "return" | "if" | "else" | "for" | "in" | "while" | "loop" | "match" | "break" | "continue" | "defer" | "unsafe" | "true" | "false" | "import" | "syntax" | "use")
}

fn class(kind: TokenKind, text: &str) -> CtTokenClass {
    match kind {
        TokenKind::Identifier if keyword(text) => CtTokenClass::Keyword,
        TokenKind::Identifier => CtTokenClass::Identifier,
        TokenKind::Integer | TokenKind::Float | TokenKind::TupleIndex => CtTokenClass::Integer,
        TokenKind::String | TokenKind::Char => CtTokenClass::String,
        _ => CtTokenClass::Punctuation,
    }
}

struct Builder<'a> {
    source: &'a SourceUnit,
    tokens: &'a [Token],
    nodes: Vec<CtTokenNode>,
    origins: Vec<CtOrigin>,
    input: Vec<CtInputToken>,
    left: usize,
}
impl Builder<'_> {
    fn origin(&mut self, span: Span, parent: Option<CtOriginId>) -> CompileResult<CtOriginId> {
        self.left = self.left.checked_sub(1).ok_or_else(|| error("provider origin budget exceeded", span))?;
        let id = CtOriginId(u32::try_from(self.origins.len()).map_err(|_| error("provider origin identity overflow", span))?);
        self.origins.push(CtOrigin { span, parent, role: CtOriginRole::Input });
        Ok(id)
    }
    fn node(&mut self, node: CtTokenNode, span: Span) -> CompileResult<CtTokenNodeId> {
        self.left = self.left.checked_sub(1).ok_or_else(|| error("provider token budget exceeded", span))?;
        let id = CtTokenNodeId(u32::try_from(self.nodes.len()).map_err(|_| error("provider token identity overflow", span))?);
        self.nodes.push(node);
        Ok(id)
    }
    fn tree(&mut self, tree: &s::TokenTree, parent: Option<CtOriginId>, depth: usize, limits: &Limits) -> CompileResult<CtTokenNodeId> {
        if depth > limits.max_call_depth { return Err(error("provider token nesting budget exceeded", tree.span)); }
        let origin = self.origin(tree.span, parent)?;
        let mut children = Vec::new();
        for entry in &tree.children {
            match entry {
                s::TokenTreeEntry::Tree(nested) => children.push(self.tree(nested, Some(origin), depth + 1, limits)?),
                s::TokenTreeEntry::Token { text, span } => {
                    if !span.is_valid_for(self.source) || &self.source.text[span.start..span.end] != text {
                        return Err(error("provider token differs from source bytes", *span));
                    }
                    let at = self.tokens.partition_point(|token| token.span.start < span.start);
                    let token = self.tokens.get(at).filter(|token| token.span == *span).ok_or_else(|| error("provider token has no lossless lexer origin", *span))?;
                    let token_class = class(token.kind, text);
                    let token_origin = self.origin(*span, Some(origin))?;
                    let table_id = CtInputTokenId(u32::try_from(self.input.len()).map_err(|_| error("provider input token identity overflow", *span))?);
                    let mut scope = b"zil/ct-input-scope/v1\0".to_vec();
                    scope.extend_from_slice(&self.source.fingerprint().0);
                    scope.extend_from_slice(&span.start.to_le_bytes());
                    scope.extend_from_slice(&span.end.to_le_bytes());
                    let scope_fingerprint = Fingerprint::of(&scope);
                    self.input.push(CtInputToken { span: *span, class: token_class, text: text.clone(), scope_fingerprint, binding: None });
                    let name = (token_class == CtTokenClass::Identifier).then(|| CtIdent {
                        text: text.clone(), namespace: CtNameNamespace::Value, source: CtNameSource::Input(table_id),
                        member_owner: None, member_owner_type: None,
                    });
                    children.push(self.node(CtTokenNode::Token(CtToken { class: token_class, text: text.clone(), origin: token_origin, name }), *span)?);
                }
            }
        }
        self.node(CtTokenNode::Group { delimiter: delimiter(tree.delimiter), children, origin }, tree.span)
    }
}

pub(super) fn from_source(source: &SourceUnit, tokens: &[Token], tree: &s::TokenTree, path: &[String], limits: &Limits) -> CompileResult<(CtTokenTree, CtInputTable)> {
    if !tree.span.is_valid_for(source) { return Err(error("provider invocation span is not in source", tree.span)); }
    let mut builder = Builder { source, tokens, nodes: Vec::new(), origins: Vec::new(), input: Vec::new(), left: limits.max_nodes };
    let root = builder.tree(tree, None, 0, limits)?;
    let mut identity = b"zil/ct-provider-invocation/v1\0".to_vec();
    identity.extend_from_slice(&source.fingerprint().0);
    identity.extend_from_slice(&tree.span.start.to_le_bytes());
    identity.extend_from_slice(&tree.span.end.to_le_bytes());
    for segment in path { identity.extend_from_slice(segment.as_bytes()); identity.push(0); }
    let invocation_fingerprint = Fingerprint::of(&identity);
    Ok((CtTokenTree { nodes: builder.nodes, roots: vec![root], origins: builder.origins },
        CtInputTable { invocation_fingerprint, tokens: builder.input }))
}

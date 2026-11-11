#![forbid(unsafe_code)]
//! ZIL's unverified front end. Parse recovery never means successful compilation:
//! callers must inspect `diagnostics` or use `into_result` before continuing.

pub mod ast;
pub mod attributes;
pub mod format;
pub mod lexer;
mod parsed_source;
mod parser;
pub mod token;
pub mod unicode;
pub use parsed_source::ParsedSource;

use zil_base::{CompileResult, Diagnostic, Limits, SourceUnit, Span};

#[derive(Clone)]
pub struct ParseResult {
    pub module: ast::Module,
    pub tokens: Vec<token::Token>,
    pub diagnostics: Vec<Diagnostic>,
    /// These attachments need a configuration snapshot or compile-time
    /// selection before their attributes can be resolved. The raw AST remains
    /// unverified and must not be mistaken for a materialized module.
    pub deferred_attributes: Vec<attributes::DeferredAttribute>,
    /// Attachments known to be removed by cfg. They remain in the raw AST so
    /// source tools can preserve their spelling; later phases must materialize
    /// configuration before using declarations.
    pub cfg_removed: Vec<Span>,
    /// Actual parser/attribute work needed by successful immutable syntax.
    /// This receipt carries no verification authority.
    pub usage: ParseUsage,
    /// The parser encountered a TypeRef MetaEval, MetaSplice or MetaBlock.
    /// This is only an untrusted routing candidate, including in quote templates
    /// or recovered input; it grants no compile-time or verification authority.
    pub type_meta_candidate: bool,
}

// Usage and routing hints are process-local metadata. Keep syntax dump formatting
// identical to the original raw front-end result.
impl std::fmt::Debug for ParseResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParseResult")
            .field("module", &self.module)
            .field("tokens", &self.tokens)
            .field("diagnostics", &self.diagnostics)
            .field("deferred_attributes", &self.deferred_attributes)
            .field("cfg_removed", &self.cfg_removed)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseUsage {
    pub nodes_required: usize,
}

/// Opaque IDs refer to driver-owned, untrusted origin and name-context tables.
/// Parsing preserves them; it does not authenticate binding or grant a scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedTokenContext {
    pub span: Span,
    pub origin: u32,
    pub name_context: Option<u32>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GeneratedParseContext {
    pub tokens: Vec<GeneratedTokenContext>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedSyntax {
    Expr(ast::Expr),
    Type(ast::TypeRef),
    StmtList(Vec<ast::Stmt>),
    ItemList(Vec<ast::Item>),
}
#[derive(Clone, Debug)]
pub struct GeneratedParseResult {
    pub syntax: Option<GeneratedSyntax>,
    pub tokens: Vec<token::Token>,
    pub context: GeneratedParseContext,
    pub diagnostics: Vec<Diagnostic>,
    pub usage: ParseUsage,
    /// Untrusted TypeRef meta-syntax routing hint, as in `ParseResult`.
    pub type_meta_candidate: bool,
}
impl GeneratedParseResult {
    pub fn into_result(self) -> CompileResult<Self> {
        if self.diagnostics.is_empty() && self.syntax.is_some() {
            Ok(self)
        } else {
            Err(self.diagnostics)
        }
    }
}

/// Parse a complete generated category while retaining its token provenance.
/// Caller must bind these opaque context IDs to the actual CT source tables,
/// apply hygiene, and run ordinary attributes/expansion/type/safety checking.
pub fn parse_generated(
    source: &SourceUnit,
    category: ast::SyntaxCategory,
    context: &GeneratedParseContext,
    limits: &Limits,
) -> GeneratedParseResult {
    let lexed = lexer::lex_with_limits(source, limits);
    let mut diagnostics = lexed.diagnostics;
    let lookup = lexed
        .tokens
        .len()
        .checked_ilog2()
        .map_or(1, |n| n as usize + 2);
    let context_work = context
        .tokens
        .len()
        .checked_mul(lookup.saturating_add(2))
        .and_then(|n| n.checked_add(lexed.tokens.len()));
    let mut valid = context.tokens.len() <= limits.max_tokens
        && context_work.is_some_and(|n| n <= limits.max_nodes);
    if !valid {
        diagnostics.push(Diagnostic::limit(
            zil_base::Phase::Parse,
            "generated provenance exceeds parser limits",
            Some(source.span()),
        ));
    }
    let mut previous = None;
    for entry in &context.tokens {
        if !valid {
            break;
        }
        let at = lexed
            .tokens
            .partition_point(|t| t.span.start < entry.span.start);
        if previous.is_some_and(|end| entry.span.start < end)
            || entry.span.file != source.file_id
            || lexed.tokens.get(at).is_none_or(|t| {
                t.span != entry.span
                    || t.kind == token::TokenKind::Eof
                    || (entry.name_context.is_some() && t.kind != token::TokenKind::Identifier)
            })
        {
            diagnostics.push(Diagnostic::error(
                zil_base::Phase::Parse,
                "E-PARSE-GENERATED-CONTEXT",
                Some(source.span()),
                "generated provenance must name ordered complete source tokens",
            ));
            valid = false;
        }
        previous = Some(entry.span.end);
    }
    if valid {
        let mut entry = 0;
        for token in &lexed.tokens {
            if token.kind == token::TokenKind::Eof || token.is_trivia() {
                continue;
            }
            while context
                .tokens
                .get(entry)
                .is_some_and(|c| c.span.start < token.span.start)
            {
                entry += 1;
            }
            if context
                .tokens
                .get(entry)
                .is_none_or(|c| c.span != token.span)
            {
                diagnostics.push(Diagnostic::error(
                    zil_base::Phase::Parse,
                    "E-PARSE-GENERATED-CONTEXT",
                    Some(token.span),
                    "every generated source token requires its exact provenance",
                ));
                valid = false;
                break;
            }
            entry += 1;
        }
    }
    let (syntax, mut errors, nodes, type_meta_candidate) = if diagnostics.is_empty() {
        let remaining = Limits {
            max_nodes: limits.max_nodes - context_work.unwrap(),
            ..limits.clone()
        };
        parser::generated(source, &lexed.tokens, category, &remaining)
    } else {
        (None, Vec::new(), 0, false)
    };
    diagnostics.append(&mut errors);
    GeneratedParseResult {
        syntax,
        tokens: lexed.tokens,
        context: if valid {
            context.clone()
        } else {
            GeneratedParseContext::default()
        },
        diagnostics,
        usage: ParseUsage {
            nodes_required: nodes.saturating_add(context_work.unwrap_or(usize::MAX)),
        },
        type_meta_candidate,
    }
}

impl ParseResult {
    /// Sufficient for reusing a successful parse under new token/node quotas.
    /// EOF is included conservatively to preserve project charging. The caller
    /// must still bind source/position identity, source bytes, depth and context.
    pub fn fits_limits(&self, limits: &Limits) -> bool {
        self.diagnostics.is_empty()
            && self.tokens.len() <= limits.max_tokens
            && self.usage.nodes_required <= limits.max_nodes
    }

    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
    pub fn into_result(self) -> CompileResult<ast::Module> {
        if self.diagnostics.is_empty() {
            Ok(self.module)
        } else {
            Err(self.diagnostics)
        }
    }
}

/// Parse a whole module. A recovered AST is untrusted when diagnostics are present.
pub fn parse(source: &SourceUnit) -> ParseResult {
    parse_with_limits(source, &Limits::default())
}

pub fn parse_with_limits(source: &SourceUnit, limits: &Limits) -> ParseResult {
    parse_with_attribute_context(source, limits, &attributes::AttributeContext::default())
}

/// Parse and validate retained attributes using an explicitly supplied frozen
/// configuration/custom-attribute registry. This never executes attributes or
/// removes source nodes. Name, value-type, layout, lint, and trait checks remain
/// later-phase obligations.
pub fn parse_with_attribute_context(
    source: &SourceUnit,
    limits: &Limits,
    context: &attributes::AttributeContext,
) -> ParseResult {
    let lexed = lexer::lex_with_limits(source, limits);
    let (module, mut diagnostics, parser_nodes, type_meta_candidate) =
        parser::module(source, &lexed.tokens, limits);
    let mut all = lexed.diagnostics;
    all.append(&mut diagnostics);
    let (validation, attribute_work) = if all.is_empty() {
        attributes::validate_with_usage(&module, context, limits)
    } else {
        (attributes::AttributeValidation::default(), 0)
    };
    all.extend(validation.diagnostics);
    ParseResult {
        module,
        tokens: lexed.tokens,
        diagnostics: all,
        deferred_attributes: validation.deferred,
        cfg_removed: validation.removed,
        usage: ParseUsage {
            nodes_required: parser_nodes.max(attribute_work),
        },
        type_meta_candidate,
    }
}

#[cfg(test)]
mod generated_tests;

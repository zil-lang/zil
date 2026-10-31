#![forbid(unsafe_code)]
//! ZIL's unverified front end. Parse recovery never means successful compilation:
//! callers must inspect `diagnostics` or use `into_result` before continuing.

pub mod ast;
pub mod attributes;
pub mod format;
pub mod lexer;
mod parser;
pub mod token;
pub mod unicode;

use zil_base::{CompileResult, Diagnostic, Limits, SourceUnit, Span};

#[derive(Clone, Debug)]
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseUsage {
    pub nodes_required: usize,
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
    let (module, mut diagnostics, parser_nodes) = parser::module(source, &lexed.tokens, limits);
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
    }
}

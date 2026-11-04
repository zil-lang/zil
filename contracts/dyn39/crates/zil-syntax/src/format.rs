//! Conservative source formatting. Only trivia outside syntax islands changes.
//! Logical newline positions, syntax tokens, comments, and literal bytes stay
//! intact. This is intentionally not an AST pretty-printer that guesses where
//! semicolons or line continuations can be inserted.
use crate::token::{Symbol, Token, TokenKind};
use zil_base::{CompileResult, Diagnostic, Limits, Phase, SourceUnit};

pub fn format(source: &SourceUnit) -> CompileResult<String> {
    format_with_limits(source, &Limits::default())
}

pub fn format_with_limits(source: &SourceUnit, limits: &Limits) -> CompileResult<String> {
    let parsed = crate::parse_with_limits(source, limits);
    if !parsed.is_ok() {
        return Err(parsed.diagnostics);
    }
    let tokens = &parsed.tokens;
    let mut out = String::with_capacity(source.text.len());
    let mut at_line_start = true;
    let mut horizontal_space = false;
    let mut braces = 0usize;
    let mut previous_significant = None;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        match token.kind {
            TokenKind::Eof => break,
            TokenKind::Whitespace => {
                horizontal_space = true;
                index += 1;
                continue;
            }
            TokenKind::Newline => {
                out.push('\n');
                at_line_start = true;
                horizontal_space = false;
                index += 1;
                continue;
            }
            _ => {}
        }
        if at_line_start {
            let indentation =
                braces.saturating_sub(usize::from(token.kind == TokenKind::Symbol(Symbol::RBrace)));
            out.extend(std::iter::repeat_n(' ', indentation * 4));
        } else if horizontal_space {
            out.push(' ');
        }
        horizontal_space = false;
        at_line_start = false;

        // A syntax island's language can give trivia its own meaning. Preserve
        // its entire delimited payload. Conservatively protecting `!(...)` in
        // an ordinary expression is harmless and avoids guessing names/types.
        if previous_significant == Some(TokenKind::Symbol(Symbol::Bang))
            && opening(token.kind).is_some()
        {
            let end = balanced_end(tokens, index).expect("successfully parsed delimiters");
            out.push_str(&source.text[token.span.start..tokens[end].span.end]);
            previous_significant = Some(tokens[end].kind);
            index = end + 1;
            continue;
        }
        out.push_str(token.text(&source.text));
        if token.kind == TokenKind::Symbol(Symbol::LBrace) {
            braces += 1;
        }
        if token.kind == TokenKind::Symbol(Symbol::RBrace) {
            braces = braces.saturating_sub(1);
        }
        if !token.is_trivia() {
            previous_significant = Some(token.kind);
        }
        index += 1;
    }
    let formatted = SourceUnit::new(source.file_id, source.module_id, &source.path, out);
    let reparsed = crate::parse_with_limits(&formatted, limits);
    if !reparsed.is_ok() || !same_tokens(tokens, &source.text, &reparsed.tokens, &formatted.text) {
        return Err(vec![Diagnostic::error(
            Phase::Parse,
            "E-FORMAT-INVARIANT",
            Some(source.span()),
            "formatting changed syntax tokens or a logical newline",
        )]);
    }
    Ok(formatted.text)
}

fn opening(kind: TokenKind) -> Option<Symbol> {
    match kind {
        TokenKind::Symbol(Symbol::LParen) => Some(Symbol::RParen),
        TokenKind::Symbol(Symbol::LBracket) => Some(Symbol::RBracket),
        TokenKind::Symbol(Symbol::LBrace) => Some(Symbol::RBrace),
        _ => None,
    }
}
fn balanced_end(tokens: &[Token], start: usize) -> Option<usize> {
    let mut stack = vec![opening(tokens[start].kind)?];
    for (index, token) in tokens.iter().enumerate().skip(start + 1) {
        if let Some(close) = opening(token.kind) {
            stack.push(close);
        } else if let TokenKind::Symbol(symbol) = token.kind
            && Some(&symbol) == stack.last()
        {
            stack.pop();
            if stack.is_empty() {
                return Some(index);
            }
        }
    }
    None
}
fn same_tokens(before: &[Token], before_text: &str, after: &[Token], after_text: &str) -> bool {
    let significant = |kind: TokenKind| kind != TokenKind::Whitespace;
    let mut left = before.iter().filter(|t| significant(t.kind));
    let mut right = after.iter().filter(|t| significant(t.kind));
    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(a), Some(b))
                if a.kind == b.kind
                    && (a.kind == TokenKind::Newline
                        || a.text(before_text) == b.text(after_text)) => {}
            _ => return false,
        }
    }
}

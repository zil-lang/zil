//! UTF-8 lexer with a lossless trivia tape and exact pinned Unicode XID tables.
use crate::token::{Symbol, Token, TokenKind};
use crate::unicode::{is_xid_continue, is_xid_start};
use zil_base::{Diagnostic, Limits, Phase, SourceUnit, Span};

#[derive(Clone, Debug)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn lex(source: &SourceUnit) -> LexResult {
    lex_with_limits(source, &Limits::default())
}

pub fn lex_with_limits(source: &SourceUnit, limits: &Limits) -> LexResult {
    let mut lexer = Lexer {
        source,
        offset: 0,
        tokens: Vec::new(),
        diagnostics: Vec::new(),
        previous: None,
    };
    if source.text.len() > limits.max_source_bytes {
        lexer.diagnostics.push(Diagnostic::limit(
            Phase::Lex,
            "source byte limit exceeded",
            Some(source.span()),
        ));
        lexer.offset = source.text.len();
        lexer.push(TokenKind::Invalid, 0);
    } else {
        while lexer.offset < source.text.len() {
            if lexer.tokens.len() >= limits.max_tokens {
                let start = lexer.offset;
                lexer.offset = source.text.len();
                lexer.diagnostics.push(Diagnostic::limit(
                    Phase::Lex,
                    "token limit exceeded",
                    Some(lexer.span(start)),
                ));
                lexer.push(TokenKind::Invalid, start);
                break;
            }
            lexer.next_token();
        }
    }
    let end = source.text.len();
    lexer.tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(source.file_id, end, end),
    });
    LexResult {
        tokens: lexer.tokens,
        diagnostics: lexer.diagnostics,
    }
}

struct Lexer<'a> {
    source: &'a SourceUnit,
    offset: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    previous: Option<TokenKind>,
}

impl Lexer<'_> {
    fn remaining(&self) -> &str {
        &self.source.text[self.offset..]
    }
    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.offset += c.len_utf8();
        Some(c)
    }
    fn span(&self, start: usize) -> Span {
        Span::new(self.source.file_id, start, self.offset)
    }
    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: self.span(start),
        });
        if !matches!(
            kind,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment
        ) {
            self.previous = Some(kind);
        }
    }
    fn error(&mut self, code: &str, start: usize, message: &str) {
        self.diagnostics.push(Diagnostic::error(
            Phase::Lex,
            code,
            Some(self.span(start)),
            message,
        ));
    }
    fn consume_while(&mut self, predicate: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&predicate) {
            self.bump();
        }
    }
    fn next_token(&mut self) {
        let start = self.offset;
        let c = self.peek().expect("nonempty lexer input");
        if c == '\n' || c == '\r' {
            self.bump();
            if c == '\r' && self.peek() == Some('\n') {
                self.bump();
            }
            self.push(TokenKind::Newline, start);
        } else if c.is_whitespace() {
            self.consume_while(|x| x.is_whitespace() && x != '\n' && x != '\r');
            self.push(TokenKind::Whitespace, start);
        } else if self.remaining().starts_with("//") {
            self.offset += 2;
            self.consume_while(|x| x != '\n' && x != '\r');
            self.push(TokenKind::LineComment, start);
        } else if self.remaining().starts_with("/*") {
            self.block_comment(start);
        } else if is_xid_start(c) {
            self.bump();
            self.consume_while(is_xid_continue);
            self.push(TokenKind::Identifier, start);
        } else if c.is_ascii_digit() {
            self.number(start);
        } else if c == '"' {
            self.quoted(start, '"', TokenKind::String);
        } else if c == '\'' {
            self.apostrophe(start);
        } else if let Some((text, symbol)) = SYMBOLS
            .iter()
            .find(|(text, _)| self.remaining().starts_with(text))
        {
            self.offset += text.len();
            self.push(TokenKind::Symbol(*symbol), start);
        } else {
            self.bump();
            self.error(
                "E-LEX-CHARACTER",
                start,
                "character is not a ZIL token or Unicode XID identifier character",
            );
            self.push(TokenKind::Invalid, start);
        }
    }
    fn block_comment(&mut self, start: usize) {
        self.offset += 2;
        let mut depth = 1usize;
        while self.offset < self.source.text.len() && depth != 0 {
            if self.remaining().starts_with("/*") {
                depth += 1;
                self.offset += 2;
            } else if self.remaining().starts_with("*/") {
                depth -= 1;
                self.offset += 2;
            } else {
                self.bump();
            }
        }
        if depth != 0 {
            self.error("E-LEX-COMMENT", start, "unterminated nested block comment");
        }
        self.push(TokenKind::BlockComment, start);
    }
    fn number(&mut self, start: usize) {
        if self.previous == Some(TokenKind::Symbol(Symbol::Dot)) {
            self.consume_while(|c| c.is_ascii_digit() || c == '_');
            if !digit_group(&self.source.text[start..self.offset], 10) {
                self.error("E-LEX-NUMBER", start, "invalid decimal tuple index");
            }
            self.push(TokenKind::TupleIndex, start);
            return;
        }
        let radix = if self.remaining().starts_with("0x") {
            Some(16)
        } else if self.remaining().starts_with("0o") {
            Some(8)
        } else if self.remaining().starts_with("0b") {
            Some(2)
        } else {
            None
        };
        if let Some(radix) = radix {
            self.offset += 2;
            self.consume_while(|c| c.is_ascii_alphanumeric() || c == '_');
            let spelling = &self.source.text[start + 2..self.offset];
            let digits = spelling
                .strip_suffix("u8")
                .or_else(|| spelling.strip_suffix("u16"))
                .unwrap_or(spelling);
            if !digit_group(digits, radix) {
                self.error(
                    "E-LEX-NUMBER",
                    start,
                    "invalid radix digit or number separator",
                );
            }
            self.push(TokenKind::Integer, start);
            return;
        }
        self.consume_while(|c| c.is_ascii_digit() || c == '_');
        let integer_end = self.offset;
        let mut valid = digit_group(&self.source.text[start..integer_end], 10);
        let mut floating = false;
        if self.peek() == Some('.')
            && self
                .remaining()
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_digit())
        {
            floating = true;
            self.offset += 1;
            let fraction = self.offset;
            self.consume_while(|c| c.is_ascii_digit() || c == '_');
            valid &= digit_group(&self.source.text[fraction..self.offset], 10);
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            floating = true;
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            let exponent = self.offset;
            self.consume_while(|c| c.is_ascii_digit() || c == '_');
            valid &= digit_group(&self.source.text[exponent..self.offset], 10);
        }
        if self.peek().is_some_and(is_xid_continue) {
            let suffix = self.offset;
            self.consume_while(is_xid_continue);
            // u8/u16 are the admitted source suffixes. The i32/i64
            // spellings used by later normalization remain internal forms.
            valid &= !floating && matches!(&self.source.text[suffix..self.offset], "u8" | "u16");
        }
        if !valid {
            self.error(
                "E-LEX-NUMBER",
                start,
                "invalid number spelling, suffix, or separator",
            );
        }
        self.push(
            if floating {
                TokenKind::Float
            } else {
                TokenKind::Integer
            },
            start,
        );
    }
    fn apostrophe(&mut self, start: usize) {
        let after_quote = start + 1;
        let tail = &self.source.text[after_quote..];
        if tail.chars().next().is_some_and(is_xid_start) {
            let mut end = after_quote;
            for c in tail.chars() {
                if !is_xid_continue(c) {
                    break;
                }
                end += c.len_utf8();
            }
            if self.source.text[end..].starts_with('\'') {
                self.quoted(start, '\'', TokenKind::Char);
            } else {
                self.offset = end;
                self.push(TokenKind::Lifetime, start);
            }
        } else {
            self.quoted(start, '\'', TokenKind::Char);
        }
    }
    fn quoted(&mut self, start: usize, quote: char, kind: TokenKind) {
        self.bump();
        let mut units = 0;
        let mut closed = false;
        while let Some(c) = self.peek() {
            if c == quote {
                self.bump();
                closed = true;
                break;
            }
            if c == '\n' || c == '\r' {
                break;
            }
            self.bump();
            if c == '\\' {
                self.escape(start);
            }
            units += 1;
        }
        if !closed {
            self.error(
                "E-LEX-STRING",
                start,
                "unterminated string or character literal",
            );
        }
        if kind == TokenKind::Char && closed && units != 1 {
            self.error(
                "E-LEX-CHAR",
                start,
                "character literal must contain exactly one Unicode scalar",
            );
        }
        self.push(kind, start);
    }
    fn escape(&mut self, literal_start: usize) {
        match self.bump() {
            Some('n' | 'r' | 't' | '0' | '\\' | '\'' | '"') => {}
            Some('u') if self.peek() == Some('{') => {
                self.bump();
                let digits = self.offset;
                self.consume_while(|c| c.is_ascii_hexdigit());
                let spelling = &self.source.text[digits..self.offset];
                let scalar = u32::from_str_radix(spelling, 16)
                    .ok()
                    .and_then(char::from_u32);
                if scalar.is_none() || spelling.len() > 6 || self.peek() != Some('}') {
                    self.error(
                        "E-LEX-ESCAPE",
                        literal_start,
                        "invalid Unicode scalar escape",
                    );
                }
                if self.peek() == Some('}') {
                    self.bump();
                }
            }
            _ => self.error(
                "E-LEX-ESCAPE",
                literal_start,
                "unsupported or incomplete literal escape",
            ),
        }
    }
}

fn digit_group(text: &str, radix: u32) -> bool {
    !text.is_empty()
        && !text.starts_with('_')
        && !text.ends_with('_')
        && !text.contains("__")
        && text.chars().all(|c| c == '_' || c.is_digit(radix))
}

const SYMBOLS: &[(&str, Symbol)] = &[
    ("...", Symbol::Ellipsis),
    ("..", Symbol::DotDot),
    ("::", Symbol::ColonColon),
    ("->", Symbol::Arrow),
    ("=>", Symbol::FatArrow),
    ("|>", Symbol::Pipe),
    ("+%", Symbol::PlusWrapping),
    ("-%", Symbol::MinusWrapping),
    ("*%", Symbol::StarWrapping),
    ("&&", Symbol::AmpAmp),
    ("||", Symbol::BarBar),
    ("==", Symbol::EqEq),
    ("!=", Symbol::BangEq),
    ("<=", Symbol::LtEq),
    (">=", Symbol::GtEq),
    ("<<", Symbol::Shl),
    (">>", Symbol::Shr),
    ("+=", Symbol::PlusEq),
    ("-=", Symbol::MinusEq),
    ("*=", Symbol::StarEq),
    ("/=", Symbol::SlashEq),
    ("%=", Symbol::PercentEq),
    ("&=", Symbol::AmpEq),
    ("|=", Symbol::BarEq),
    ("^=", Symbol::CaretEq),
    ("(", Symbol::LParen),
    (")", Symbol::RParen),
    ("[", Symbol::LBracket),
    ("]", Symbol::RBracket),
    ("{", Symbol::LBrace),
    ("}", Symbol::RBrace),
    (",", Symbol::Comma),
    (";", Symbol::Semicolon),
    (":", Symbol::Colon),
    (".", Symbol::Dot),
    ("#", Symbol::Hash),
    ("?", Symbol::Question),
    ("+", Symbol::Plus),
    ("-", Symbol::Minus),
    ("*", Symbol::Star),
    ("/", Symbol::Slash),
    ("%", Symbol::Percent),
    ("&", Symbol::Amp),
    ("|", Symbol::Bar),
    ("^", Symbol::Caret),
    ("~", Symbol::Tilde),
    ("!", Symbol::Bang),
    ("=", Symbol::Eq),
    ("<", Symbol::Lt),
    (">", Symbol::Gt),
];

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId};
    fn source(text: &str) -> SourceUnit {
        SourceUnit::new(FileId(2), ModuleId(1), "lexer.zil", text)
    }
    #[test]
    fn lossless_utf8_nested_comments_crlf() {
        let src = source("fn 读取() { /* 外\n /* 内 */ */\r\nlet a = \"x\\n\" //尾\n}");
        let result = lex(&src);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(
            result
                .tokens
                .iter()
                .map(|t| t.text(&src.text))
                .collect::<String>(),
            src.text
        );
        assert!(result.tokens.iter().all(|t| t.span.is_valid_for(&src)));
        assert!(
            result
                .tokens
                .iter()
                .any(|t| t.kind == TokenKind::BlockComment)
        );
    }
    #[test]
    fn xid_is_not_alphabetic_approximation() {
        assert!(crate::unicode::is_xid_start('变'));
        assert!(!crate::unicode::is_xid_start('\u{301}'));
        assert!(crate::unicode::is_xid_continue('\u{301}'));
        assert!(!crate::unicode::is_xid_start('²'));
        assert!(!lex(&source("let 😀 = 1")).diagnostics.is_empty());
    }
    #[test]
    fn tuple_indices_do_not_become_float_literals() {
        let src = source("pair.0.1 + 1.25 + 010 + 0xFF");
        let result = lex(&src);
        assert!(result.diagnostics.is_empty());
        let kinds: Vec<_> = result
            .tokens
            .iter()
            .filter(|t| !t.is_trivia())
            .map(|t| t.kind)
            .collect();
        assert_eq!(
            kinds
                .iter()
                .filter(|&&k| k == TokenKind::TupleIndex)
                .count(),
            2
        );
        assert_eq!(kinds.iter().filter(|&&k| k == TokenKind::Float).count(), 1);
    }
    #[test]
    fn malformed_tokens_preserve_their_spans() {
        for text in [
            "/* a /* b */",
            "0x_FF",
            "12__3",
            "\"unterminated",
            "'ab'",
            "\"\\u{D800}\"",
        ] {
            let src = source(text);
            let result = lex(&src);
            assert!(!result.diagnostics.is_empty(), "{text}");
            assert_eq!(
                result
                    .tokens
                    .iter()
                    .map(|t| t.text(&src.text))
                    .collect::<String>(),
                text
            );
            assert!(
                result
                    .diagnostics
                    .iter()
                    .all(|d| d.span.is_some_and(|s| s.is_valid_for(&src)))
            );
        }
    }

    #[test]
    fn byte_suffix_preserves_radix_and_separator_validation() {
        for text in [
            "0u8",
            "255u8",
            "2_55u8",
            "0xffu8",
            "0o377u8",
            "0b1111_1111u8",
        ] {
            let src = source(text);
            let result = lex(&src);
            assert!(
                result.diagnostics.is_empty(),
                "{text}: {:?}",
                result.diagnostics
            );
            assert_eq!(result.tokens[0].kind, TokenKind::Integer);
            assert_eq!(result.tokens[0].text(&src.text), text);
        }
        for text in [
            "1_u8", "0x_u8", "0xff_u8", "0bu8", "0o8u8", "1u80", "1.0u8", "1e2u8", "1i32", "1i64",
        ] {
            assert!(!lex(&source(text)).diagnostics.is_empty(), "{text}");
        }
    }
}

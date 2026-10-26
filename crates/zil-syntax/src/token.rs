//! Lossless lexical tape: every source byte belongs to exactly one non-EOF token.
use zil_base::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span.start..self.span.end]
    }
    pub fn is_trivia(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Identifier,
    Integer,
    Float,
    String,
    Char,
    Lifetime,
    TupleIndex,
    Whitespace,
    Newline,
    LineComment,
    BlockComment,
    Symbol(Symbol),
    Invalid,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Colon,
    Dot,
    DotDot,
    Ellipsis,
    ColonColon,
    Hash,
    Question,
    Arrow,
    FatArrow,
    Pipe,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusWrapping,
    MinusWrapping,
    StarWrapping,
    Amp,
    Bar,
    Caret,
    Tilde,
    Bang,
    AmpAmp,
    BarBar,
    Eq,
    EqEq,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Shl,
    Shr,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    BarEq,
    CaretEq,
}

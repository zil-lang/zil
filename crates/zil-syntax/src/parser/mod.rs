//! Recovery-oriented recursive descent and Pratt parsing. No name/type queries.
mod expr;
mod items;
mod stmt;
mod types;

use crate::ast::*;
use crate::token::{Symbol, Token, TokenKind};
use zil_base::{Diagnostic, Limits, Phase, SourceUnit, Span};

type P<T> = Result<T, ()>;

pub(crate) fn module(
    source: &SourceUnit,
    raw: &[Token],
    limits: &Limits,
) -> (Module, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    for token in raw {
        match token.kind {
            TokenKind::Whitespace | TokenKind::LineComment => {}
            TokenKind::BlockComment => {
                // A comment must not accidentally join two complete statements.
                if let Some(i) = token.text(&source.text).find(['\r', '\n']) {
                    let start = token.span.start + i;
                    tokens.push(Token {
                        kind: TokenKind::Newline,
                        span: Span::new(source.file_id, start, start + 1),
                    });
                }
            }
            _ => tokens.push(token.clone()),
        }
    }
    if tokens.last().is_none_or(|t| t.kind != TokenKind::Eof) {
        tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(source.file_id, source.text.len(), source.text.len()),
        });
    }
    let mut parser = Parser {
        source,
        raw,
        tokens,
        pos: 0,
        last: Span::new(source.file_id, 0, 0),
        diagnostics: Vec::new(),
        soft_newlines: false,
        depth: 0,
        nodes: 0,
        limits,
    };
    let items = parser.item_list(false);
    (
        Module {
            id: source.module_id,
            items,
            span: source.span(),
        },
        parser.diagnostics,
    )
}

struct Parser<'a> {
    source: &'a SourceUnit,
    raw: &'a [Token],
    tokens: Vec<Token>,
    pos: usize,
    last: Span,
    diagnostics: Vec<Diagnostic>,
    soft_newlines: bool,
    depth: usize,
    nodes: usize,
    limits: &'a Limits,
}

impl Parser<'_> {
    fn peek(&mut self) -> Token {
        if self.soft_newlines {
            self.newlines();
        }
        self.tokens[self.pos].clone()
    }
    fn bump(&mut self) -> Token {
        let token = self.peek();
        if token.kind != TokenKind::Eof {
            self.pos += 1;
        }
        self.last = token.span;
        token
    }
    fn text(&self, token: &Token) -> &str {
        token.text(&self.source.text)
    }
    fn at(&mut self, symbol: Symbol) -> bool {
        self.peek().kind == TokenKind::Symbol(symbol)
    }
    fn at_word(&mut self, word: &str) -> bool {
        let token = self.peek();
        token.kind == TokenKind::Identifier && self.text(&token) == word
    }
    fn eat(&mut self, symbol: Symbol) -> bool {
        if self.at(symbol) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn eat_word(&mut self, word: &str) -> bool {
        if self.at_word(word) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, symbol: Symbol) -> P<Token> {
        if self.at(symbol) {
            Ok(self.bump())
        } else {
            self.fail("E-PARSE-EXPECTED", format!("expected {symbol:?}"))
        }
    }
    fn expect_word(&mut self, word: &str) -> P<Token> {
        if self.at_word(word) {
            Ok(self.bump())
        } else {
            self.fail("E-PARSE-EXPECTED", format!("expected `{word}`"))
        }
    }
    fn ident(&mut self) -> P<String> {
        let token = self.peek();
        if token.kind == TokenKind::Identifier && !keyword(self.text(&token)) {
            self.bump();
            Ok(self.text(&token).to_owned())
        } else {
            self.fail("E-PARSE-IDENTIFIER", "expected an identifier")
        }
    }
    fn fail<T>(&mut self, code: &str, message: impl Into<String>) -> P<T> {
        let span = self.peek().span;
        self.diagnostics
            .push(Diagnostic::error(Phase::Parse, code, Some(span), message));
        Err(())
    }
    fn span_from(&self, start: Span) -> Span {
        start.join(self.last)
    }
    fn newlines(&mut self) {
        while self.tokens[self.pos].kind == TokenKind::Newline {
            self.pos += 1;
        }
    }
    fn separators(&mut self) {
        while matches!(
            self.tokens[self.pos].kind,
            TokenKind::Newline | TokenKind::Symbol(Symbol::Semicolon)
        ) {
            self.pos += 1;
        }
    }
    fn scoped<T>(&mut self, soft: bool, f: impl FnOnce(&mut Self) -> P<T>) -> P<T> {
        let saved = self.soft_newlines;
        self.soft_newlines = soft;
        let result = f(self);
        self.soft_newlines = saved;
        result
    }
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> P<T>) -> P<T> {
        if self.depth >= self.limits.max_call_depth || self.nodes >= self.limits.max_nodes {
            let span = self.peek().span;
            self.diagnostics.push(Diagnostic::limit(
                Phase::Parse,
                "syntax nesting or node limit exceeded",
                Some(span),
            ));
            return Err(());
        }
        self.depth += 1;
        self.nodes += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }
    fn end(&mut self) -> P<()> {
        let kind = self.peek().kind;
        match kind {
            TokenKind::Newline | TokenKind::Symbol(Symbol::Semicolon) => { self.bump(); Ok(()) },
            TokenKind::Symbol(Symbol::RBrace) | TokenKind::Eof => Ok(()),
            _ => self.fail("E-PARSE-TERMINATOR", "expected a semicolon, logical newline, or closing brace after the complete statement"),
        }
    }
    fn field_end(&mut self) -> P<()> {
        if self.eat(Symbol::Comma) {
            Ok(())
        } else {
            self.end()
        }
    }
    fn qualified(&mut self) -> P<Vec<String>> {
        let mut path = vec![self.ident()?];
        while self.eat(Symbol::Dot) {
            self.newlines();
            path.push(self.ident()?);
        }
        Ok(path)
    }
    fn recover(&mut self, before: usize, in_block: bool) {
        // Keep a closing brace for its owner; otherwise make progress and stop at
        // a complete separator. Skipped text remains in the lossless token tape.
        let mut depth = 0usize;
        loop {
            let token = self.tokens[self.pos].clone();
            match token.kind {
                TokenKind::Eof => break,
                TokenKind::Symbol(Symbol::RBrace) if depth == 0 && in_block => break,
                TokenKind::Newline | TokenKind::Symbol(Symbol::Semicolon) if depth == 0 => {
                    self.pos += 1;
                    break;
                }
                TokenKind::Symbol(Symbol::LBrace | Symbol::LParen | Symbol::LBracket) => depth += 1,
                TokenKind::Symbol(Symbol::RBrace | Symbol::RParen | Symbol::RBracket) => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            self.pos += 1;
            if depth == 0
                && self.pos > before
                && item_keyword(self.tokens[self.pos].text(&self.source.text))
            {
                break;
            }
        }
        if self.pos == before
            && self.tokens[self.pos].kind != TokenKind::Eof
            && !(in_block && self.tokens[self.pos].kind == TokenKind::Symbol(Symbol::RBrace))
        {
            self.pos += 1;
        }
    }
    fn literal(&mut self) -> Option<Literal> {
        let token = self.peek();
        let text = self.text(&token).to_owned();
        let value = match token.kind {
            TokenKind::Integer => Literal::Integer(text),
            TokenKind::Float => Literal::Float(text),
            TokenKind::String => Literal::String(text),
            TokenKind::Char => Literal::Char(text),
            TokenKind::Identifier if text == "true" => Literal::Bool(true),
            TokenKind::Identifier if text == "false" => Literal::Bool(false),
            _ => return None,
        };
        self.bump();
        Some(value)
    }
}

fn keyword(text: &str) -> bool {
    matches!(
        text,
        "fn" | "let"
            | "var"
            | "return"
            | "yield"
            | "if"
            | "else"
            | "while"
            | "for"
            | "in"
            | "loop"
            | "match"
            | "with"
            | "unsafe"
            | "break"
            | "continue"
            | "defer"
            | "import"
            | "syntax"
            | "use"
            | "as"
            | "pub"
            | "extern"
            | "struct"
            | "resource"
            | "package"
            | "enum"
            | "trait"
            | "impl"
            | "type"
            | "const"
            | "manages"
            | "life"
            | "ref"
            | "mut"
            | "give"
            | "move"
            | "dyn"
            | "where"
            | "contract"
            | "true"
            | "false"
    )
}
fn item_keyword(text: &str) -> bool {
    matches!(
        text,
        "fn" | "pub"
            | "import"
            | "syntax"
            | "extern"
            | "struct"
            | "resource"
            | "package"
            | "enum"
            | "trait"
            | "impl"
            | "type"
            | "const"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, ResultCategory};
    fn parse_text(text: &str) -> crate::ParseResult {
        crate::parse(&SourceUnit::new(FileId(4), ModuleId(1), "test.zil", text))
    }
    fn ok(text: &str) -> Module {
        let result = parse_text(text);
        assert!(
            result.is_ok(),
            "source: {text}\ndiagnostics: {:?}",
            result.diagnostics
        );
        result.module
    }
    fn rejected(text: &str, code: &str) {
        let result = parse_text(text);
        assert!(
            result.diagnostics.iter().any(|d| d.code == code),
            "source: {text}\ndiagnostics: {:?}",
            result.diagnostics
        );
        assert!(!result.is_ok());
        for diagnostic in result.diagnostics {
            assert!(diagnostic.span.is_some());
        }
    }
    #[test]
    fn real_function_body_and_pratt_precedence() {
        let module = ok(
            "fn main() -> i32 { let x = 2 + 3 * 4; var y: i32 = x; while y > 0 { y -= 1 }; return x }",
        );
        let ItemKind::Function(function) = &module.items[0].kind else {
            panic!("function AST");
        };
        let FunctionBody::Block(body) = &function.body else {
            panic!("block AST");
        };
        assert_eq!(body.statements.len(), 4);
        let StmtKind::Binding { value, .. } = &body.statements[0].kind else {
            panic!("binding AST");
        };
        let ExprKind::Binary {
            op: BinaryOp::Add,
            right,
            ..
        } = &value.kind
        else {
            panic!("addition root");
        };
        assert!(matches!(
            right.kind,
            ExprKind::Binary {
                op: BinaryOp::Multiply,
                ..
            }
        ));
    }
    #[test]
    fn receivers_records_and_function_types() {
        ok(
            "struct Point { x: i32, y: i32 }\nimpl Point(self: &Self) { fn sum() -> i32 => .x + .y\n fn shift(self: &mut Self, n: i32) { .x += n } }\ntrait Read { fn get(self: &Self) -> i32; }\nimpl Read for Point { fn get() -> i32 => .x }\ntype F = (fn(i32) -> E!i32)?\nfn pair() -> Point => ({x: 1, y: 2})",
        );
    }
    #[test]
    fn closures_pipelines_and_compile_time_forms() {
        ok(
            "#[cfg(true)]\nfn f<T, #N: usize>(x: T) -> [T; N] where T: Copy { let cb = move (n: i32) => n + 1; let p = 1 |> ((x) => x + 1); let a = #(1 + 2); let b = #{ yield a }; #if (true) { cb(p) } else { cb(0) } }\n#if (true) { const N = 2 } else { const N = 3 }\nsyntax use build.tokens as b\nfn island() { b.tokens!{ fn x ( [a] ) } }",
        );
    }
    #[test]
    fn newlines_are_logical_and_do_not_back_join() {
        let module = ok(
            "fn f() { return\n(1)\nlet a = 1 +\n2\nlet b = (a\n+ 3)\na\n(4)\na /* break\n here */ (5) }",
        );
        let ItemKind::Function(function) = &module.items[0].kind else {
            panic!();
        };
        let FunctionBody::Block(body) = &function.body else {
            panic!();
        };
        assert_eq!(body.statements.len(), 8);
        assert!(matches!(body.statements[0].kind, StmtKind::Return(None)));
    }
    #[test]
    fn frozen_parse_rejections_are_specific() {
        rejected(
            "impl Read for Point(self: &Self) {}",
            "E-PARSE-TRAIT-RECEIVER-DEFAULT",
        );
        rejected("fn f() { fn nested() {} }", "E-PARSE-NESTED-NAMED-FN");
        rejected("fn f() { let x = 1 |> (x) => x }", "E-PARSE-PIPE-LAMBDA");
        rejected("fn f() -> Point => { x: 1 }", "E-PARSE-RECORD-BLOCK");
        rejected(
            "fn f() { let x = { a: 1, ...p } }",
            "E-PARSE-SPREAD-POSITION",
        );
        rejected("let x = 1", "E-PARSE-MODULE-STATEMENT");
        rejected("fn f() { let x = 1 < 2 < 3 }", "E-PARSE-COMPARISON-CHAIN");
        rejected("fn f() { #\n(1) }", "E-PARSE-HASH-DISPATCH");
    }
    #[test]
    fn nested_type_closers_do_not_change_shift_tokens() {
        let result = parse_text("type X = Outer<Inner<u8>>\nfn f(x: i32) -> i32 => x >> 1");
        assert!(result.is_ok(), "{:?}", result.diagnostics);
        assert_eq!(
            result
                .tokens
                .iter()
                .filter(|t| t.kind == TokenKind::Symbol(Symbol::Shr))
                .count(),
            2
        );
    }
    #[test]
    fn malformed_source_recovers_to_later_item() {
        let result = parse_text("fn broken(x i32) {}\nfn good() -> i32 => 7");
        assert!(!result.is_ok());
        assert!(
            result
                .module
                .items
                .iter()
                .any(|item| matches!(&item.kind, ItemKind::Function(f) if f.name == "good"))
        );
    }
    #[test]
    fn nesting_limits_are_resource_limits() {
        let source = SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "limit.zil",
            "fn f() { let x = (((((1))))) }",
        );
        let limits = Limits {
            max_call_depth: 5,
            ..Limits::default()
        };
        let result = crate::parse_with_limits(&source, &limits);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
        );
    }
    #[test]
    fn finite_contract_dsl_accepts_give_and_paths() {
        ok(
            "pub fn pick(a: Ref<Node>, b: Ref<Node>, first: bool) -> Ref<Node>\ncontract {\nresult(from(a, b));\nprovides(manage(result));\n} { return if first { a } else { b } }\nfn transfer(give x: Owned) -> Owned contract { result(give); effects(cleanup(Owned)) } { return give x }",
        );
        rejected("fn f() contract { result(1 + 2) } {}", "E-PARSE-IDENTIFIER");
    }
    #[test]
    fn syntax_islands_preserve_trivia_inside_each_delimiter() {
        let module = ok("fn f() { b.tokens!{ // line\n a /* nested /* inner */ */ (\n  x\t) } }");
        let ItemKind::Function(function) = &module.items[0].kind else {
            panic!();
        };
        let FunctionBody::Block(body) = &function.body else {
            panic!();
        };
        let StmtKind::Expression(expr) = &body.statements[0].kind else {
            panic!();
        };
        let ExprKind::SyntaxIsland { tree, .. } = &expr.kind else {
            panic!();
        };
        fn text(tree: &TokenTree) -> String {
            let (open, close) = match tree.delimiter {
                Delimiter::Brace => ('{', '}'),
                Delimiter::Bracket => ('[', ']'),
                Delimiter::Parenthesis => ('(', ')'),
            };
            let mut out = String::from(open);
            for child in &tree.children {
                match child {
                    TokenTreeEntry::Token { text, .. } => out.push_str(text),
                    TokenTreeEntry::Tree(t) => out.push_str(&text(t)),
                }
            }
            out.push(close);
            out
        }
        assert_eq!(
            text(tree),
            "{ // line\n a /* nested /* inner */ */ (\n  x\t) }"
        );
    }
    #[test]
    fn chains_are_bounded_before_recursive_ast_drop() {
        for expression in [
            std::iter::repeat_n("1", 600).collect::<Vec<_>>().join("+"),
            format!("x{}", ".x".repeat(600)),
        ] {
            let result = parse_text(&format!("fn f() {{ let x = {expression} }}"));
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit)
            );
        }
    }
    #[test]
    fn control_heads_slices_and_type_arrows_have_one_parse() {
        ok(
            "fn f(a: [i32], p: Point) { if (Point { x: 1 }).x > 0 { return }\n let b = a[(1 < 2)..]\n let c = a[..]\n let d = a[1 | 2] }\ntype A = fn() -> i32?\ntype B = (fn() -> i32)?\ntype C = &(fn() -> E!i32)",
        );
        rejected("fn f(a: [i32]) { let x = a[] }", "E-PARSE-EXPRESSION");
        rejected("fn f(a: [i32]) { let x = a[1 < 2] }", "E-PARSE-EXPECTED");
        rejected("type F = &fn() -> i32", "E-PARSE-IDENTIFIER");
        rejected("fn f() { let mut = 1 }", "E-PARSE-IDENTIFIER");
        rejected("fn f() { let x = 1 := 2 }", "E-PARSE-TERMINATOR");
    }
}

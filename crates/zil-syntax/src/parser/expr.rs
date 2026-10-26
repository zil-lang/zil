use super::*;

impl Parser<'_> {
    pub(super) fn expr(&mut self, record: bool) -> P<Expr> {
        self.nested(|p| {
            if p.closure_ahead() {
                p.closure()
            } else {
                p.pratt(0, record)
            }
        })
    }
    fn closure_ahead(&mut self) -> bool {
        self.peek();
        let mut i = self.pos;
        if self.tokens[i].text(&self.source.text) == "move" {
            i += 1;
        }
        if self.tokens[i].kind == TokenKind::Identifier {
            i += 1;
            if self.soft_newlines {
                while self.tokens[i].kind == TokenKind::Newline {
                    i += 1;
                }
            }
            return matches!(
                self.tokens[i].kind,
                TokenKind::Symbol(Symbol::FatArrow | Symbol::Arrow)
            );
        }
        if self.tokens[i].kind != TokenKind::Symbol(Symbol::LParen) {
            return false;
        }
        let mut depth = 0usize;
        while i < self.tokens.len() {
            match self.tokens[i].kind {
                TokenKind::Symbol(Symbol::LParen) => depth += 1,
                TokenKind::Symbol(Symbol::RParen) => {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        if self.soft_newlines {
                            while self.tokens[i].kind == TokenKind::Newline {
                                i += 1;
                            }
                        }
                        return matches!(
                            self.tokens[i].kind,
                            TokenKind::Symbol(Symbol::FatArrow | Symbol::Arrow)
                        );
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }
    fn closure(&mut self) -> P<Expr> {
        let start = self.peek().span;
        let move_capture = self.eat_word("move");
        let params = if self.eat(Symbol::LParen) {
            self.scoped(true, |p| {
                let mut params = Vec::new();
                if !p.at(Symbol::RParen) {
                    loop {
                        let start = p.peek().span;
                        let name = p.ident()?;
                        let ty = if p.eat(Symbol::Colon) {
                            Some(p.ty()?)
                        } else {
                            None
                        };
                        params.push(ClosureParam {
                            name,
                            ty,
                            span: p.span_from(start),
                        });
                        if !p.eat(Symbol::Comma) || p.at(Symbol::RParen) {
                            break;
                        }
                    }
                }
                p.expect(Symbol::RParen)?;
                Ok(params)
            })?
        } else {
            let param_start = self.peek().span;
            let name = self.ident()?;
            vec![ClosureParam {
                name,
                ty: None,
                span: self.span_from(param_start),
            }]
        };
        let return_type = if self.eat(Symbol::Arrow) {
            self.newlines();
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(Symbol::FatArrow)?;
        self.newlines();
        let body = if self.at(Symbol::LBrace) {
            ArrowBody::Block(self.closure_block()?)
        } else {
            ArrowBody::Expression(Box::new(self.expr(true)?))
        };
        Ok(Expr {
            kind: ExprKind::Closure {
                move_capture,
                params,
                return_type,
                body,
            },
            span: self.span_from(start),
        })
    }
    pub(super) fn arrow_body(&mut self) -> P<ArrowBody> {
        if self.at(Symbol::LBrace) {
            Ok(ArrowBody::Block(self.block(true)?))
        } else {
            Ok(ArrowBody::Expression(Box::new(self.expr(true)?)))
        }
    }
    fn pratt(&mut self, min: u8, record: bool) -> P<Expr> {
        self.nested(|p| p.pratt_inner(min, record))
    }
    fn pratt_inner(&mut self, min: u8, record: bool) -> P<Expr> {
        let mut left = self.prefix(record)?;
        let mut chain_depth = 0usize;
        loop {
            let token = self.peek();
            let Some((precedence, op)) = binary(token.kind) else {
                break;
            };
            if precedence < min {
                break;
            }
            chain_depth += 1;
            if chain_depth >= self.limits.max_call_depth {
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Parse,
                    "expression chain nesting limit exceeded",
                    Some(token.span),
                ));
                return Err(());
            }
            if precedence == 4
                && matches!(
                    left.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Equal
                            | BinaryOp::NotEqual
                            | BinaryOp::Less
                            | BinaryOp::LessEqual
                            | BinaryOp::Greater
                            | BinaryOp::GreaterEqual,
                        ..
                    }
                )
            {
                return self.fail(
                    "E-PARSE-COMPARISON-CHAIN",
                    "comparisons do not associate; parenthesize the intended comparison",
                );
            }
            if precedence == 5
                && matches!(
                    left.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Range,
                        ..
                    }
                )
            {
                return self.fail("E-PARSE-RANGE-CHAIN", "range operators do not associate");
            }
            self.bump();
            self.newlines();
            if op.is_none() && self.closure_ahead() {
                return self.fail(
                    "E-PARSE-PIPE-LAMBDA",
                    "a pipeline lambda target must be parenthesized",
                );
            }
            let right = self.pratt(precedence + 1, record)?;
            let span = left.span.join(right.span);
            left = Expr {
                kind: if let Some(op) = op {
                    ExprKind::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    }
                } else {
                    ExprKind::Pipeline {
                        input: Box::new(left),
                        target: Box::new(right),
                    }
                },
                span,
            };
        }
        Ok(left)
    }
    fn prefix(&mut self, record: bool) -> P<Expr> {
        let start = self.peek().span;
        let op = match self.peek().kind {
            TokenKind::Symbol(Symbol::Bang) => Some(UnaryOp::Not),
            TokenKind::Symbol(Symbol::Minus) => Some(UnaryOp::Negate),
            TokenKind::Symbol(Symbol::Tilde) => Some(UnaryOp::BitNot),
            TokenKind::Symbol(Symbol::Star) => Some(UnaryOp::Dereference),
            TokenKind::Symbol(Symbol::Amp) => Some(UnaryOp::Borrow),
            _ if self.at_word("give") => Some(UnaryOp::Give),
            _ => None,
        };
        if let Some(mut op) = op {
            self.bump();
            if matches!(op, UnaryOp::Borrow | UnaryOp::Give) {
                self.newlines();
            }
            if op == UnaryOp::Borrow && self.eat_word("mut") {
                op = UnaryOp::BorrowMut;
                self.newlines();
            }
            let operand = self.nested(|p| p.prefix(record))?;
            return Ok(Expr {
                kind: ExprKind::Unary {
                    op,
                    operand: Box::new(operand),
                },
                span: self.span_from(start),
            });
        }
        let first = self.primary(record)?;
        self.postfix(first, record)
    }
    fn primary(&mut self, record: bool) -> P<Expr> {
        let start = self.peek().span;
        if let Some(literal) = self.literal() {
            return Ok(Expr {
                kind: ExprKind::Literal(literal),
                span: self.span_from(start),
            });
        }
        let kind = if self.eat(Symbol::LParen) {
            self.scoped(true, |p| {
                if p.eat(Symbol::RParen) {
                    return Ok(ExprKind::Tuple(Vec::new()));
                }
                let first = p.expr(true)?;
                if p.eat(Symbol::Comma) {
                    let mut elements = vec![first];
                    while !p.at(Symbol::RParen) {
                        elements.push(p.expr(true)?);
                        if !p.eat(Symbol::Comma) {
                            break;
                        }
                    }
                    p.expect(Symbol::RParen)?;
                    Ok(ExprKind::Tuple(elements))
                } else {
                    p.expect(Symbol::RParen)?;
                    Ok(ExprKind::Group(Box::new(first)))
                }
            })?
        } else if self.eat(Symbol::LBracket) {
            let values = self.scoped(true, |p| {
                let mut values = Vec::new();
                while !p.at(Symbol::RBracket) {
                    values.push(p.expr(true)?);
                    if !p.eat(Symbol::Comma) {
                        break;
                    }
                }
                p.expect(Symbol::RBracket)?;
                Ok(values)
            })?;
            ExprKind::Array(values)
        } else if self.at(Symbol::LBrace) && record {
            return self.record(None);
        } else if self.eat(Symbol::Dot) {
            self.newlines();
            if self.at(Symbol::LBrace) {
                let mut value = self.record(None)?;
                value.span = self.span_from(start);
                return Ok(value);
            }
            ExprKind::ReceiverMember(self.ident()?)
        } else if self.eat_word("if") {
            let condition = self.expr(false)?;
            self.newlines();
            self.expect(Symbol::LBrace)?;
            self.newlines();
            let then_value = self.scoped(false, |p| p.expr(true))?;
            self.newlines();
            self.expect(Symbol::RBrace)?;
            self.newlines();
            self.expect_word("else")?;
            self.newlines();
            self.expect(Symbol::LBrace)?;
            self.newlines();
            let else_value = self.scoped(false, |p| p.expr(true))?;
            self.newlines();
            self.expect(Symbol::RBrace)?;
            ExprKind::If {
                condition: Box::new(condition),
                then_value: Box::new(then_value),
                else_value: Box::new(else_value),
            }
        } else if self.eat(Symbol::Hash) {
            if self.at(Symbol::LBracket) {
                return self.fail(
                    "E-PARSE-ATTRIBUTE-ATTACHMENT",
                    "attributes cannot attach to a value expression",
                );
            } else if self.eat(Symbol::LParen) {
                let value = self.scoped(true, |p| {
                    let e = p.expr(true)?;
                    p.expect(Symbol::RParen)?;
                    Ok(e)
                })?;
                ExprKind::MetaEval(Box::new(value))
            } else if self.at(Symbol::LBrace) {
                ExprKind::MetaBlock(self.block(false)?)
            } else {
                return self.fail("E-PARSE-HASH-DISPATCH", "expression `#` must be immediately followed on the same logical line by `(` or `{`");
            }
        } else if self.peek().kind == TokenKind::Identifier {
            ExprKind::Name(self.ident()?)
        } else {
            return self.fail("E-PARSE-EXPRESSION", "expected an expression");
        };
        Ok(Expr {
            kind,
            span: self.span_from(start),
        })
    }
    fn postfix(&mut self, mut object: Expr, record: bool) -> P<Expr> {
        let mut chain_depth = 0usize;
        loop {
            if chain_depth >= self.limits.max_call_depth {
                let span = self.peek().span;
                self.diagnostics.push(Diagnostic::limit(
                    Phase::Parse,
                    "postfix chain nesting limit exceeded",
                    Some(span),
                ));
                return Err(());
            }
            let start = object.span;
            let kind = if self.eat(Symbol::Dot) {
                self.newlines();
                let member = if self.peek().kind == TokenKind::TupleIndex {
                    let token = self.bump();
                    let digits = self.text(&token).replace('_', "");
                    let Ok(index) = digits.parse() else {
                        return self.fail("E-PARSE-TUPLE-INDEX", "tuple index is too large");
                    };
                    MemberName::TupleIndex(index)
                } else {
                    MemberName::Name(self.ident()?)
                };
                ExprKind::Member {
                    object: Box::new(object),
                    member,
                }
            } else if self.eat(Symbol::ColonColon) {
                let arguments = self.type_args()?;
                ExprKind::TypeApply {
                    callee: Box::new(object),
                    arguments,
                }
            } else if self.eat(Symbol::LParen) {
                let arguments = self.scoped(true, |p| {
                    let mut values = Vec::new();
                    while !p.at(Symbol::RParen) {
                        let start = p.peek().span;
                        let spread = p.eat(Symbol::Ellipsis);
                        let value = p.expr(true)?;
                        values.push(Argument {
                            spread,
                            value,
                            span: p.span_from(start),
                        });
                        if !p.eat(Symbol::Comma) {
                            break;
                        }
                    }
                    p.expect(Symbol::RParen)?;
                    Ok(values)
                })?;
                ExprKind::Call {
                    callee: Box::new(object),
                    arguments,
                }
            } else if self.eat(Symbol::LBracket) {
                let (first, second, slice) = self.scoped(true, |p| {
                    let first = if p.at(Symbol::DotDot) {
                        None
                    } else {
                        Some(Box::new(p.pratt(6, true)?))
                    };
                    let slice = p.eat(Symbol::DotDot);
                    let second = if slice && !p.at(Symbol::RBracket) {
                        Some(Box::new(p.pratt(6, true)?))
                    } else {
                        None
                    };
                    p.expect(Symbol::RBracket)?;
                    Ok((first, second, slice))
                })?;
                if slice {
                    ExprKind::Slice {
                        object: Box::new(object),
                        start: first,
                        end: second,
                    }
                } else {
                    ExprKind::Index {
                        object: Box::new(object),
                        index: first.expect("non-slice index is parsed"),
                    }
                }
            } else if self.eat(Symbol::Question) {
                ExprKind::Try(Box::new(object))
            } else if record && self.at(Symbol::LBrace) && value_type_path(&object) {
                object = self.record(Some(object))?;
                continue;
            } else if self.at(Symbol::Bang) && expression_path(&object).is_some() {
                let path = expression_path(&object).expect("checked above");
                self.bump();
                let tree = self.token_tree()?;
                ExprKind::SyntaxIsland { path, tree }
            } else {
                break;
            };
            object = Expr {
                kind,
                span: self.span_from(start),
            };
            chain_depth += 1;
        }
        Ok(object)
    }
    fn record(&mut self, ty: Option<Expr>) -> P<Expr> {
        let start = ty.as_ref().map_or_else(|| self.peek().span, |t| t.span);
        self.expect(Symbol::LBrace)?;
        let (base, fields) = self.scoped(true, |p| {
            let mut base = None;
            let mut fields = Vec::new();
            if p.eat(Symbol::Ellipsis) {
                base = Some(Box::new(p.expr(true)?));
                if !p.eat(Symbol::Comma) {
                    p.expect(Symbol::RBrace)?;
                    return Ok((base, fields));
                }
            }
            while !p.at(Symbol::RBrace) {
                if p.at(Symbol::Ellipsis) {
                    return p.fail(
                        "E-PARSE-SPREAD-POSITION",
                        "record spread must occur once, before all named fields",
                    );
                }
                let start = p.peek().span;
                let name = p.ident()?;
                let value = if p.eat(Symbol::Colon) {
                    Some(p.expr(true)?)
                } else {
                    None
                };
                fields.push(RecordField {
                    name,
                    value,
                    span: p.span_from(start),
                });
                if !p.eat(Symbol::Comma) {
                    break;
                }
            }
            p.expect(Symbol::RBrace)?;
            Ok((base, fields))
        })?;
        Ok(Expr {
            kind: ExprKind::Record {
                ty: ty.map(Box::new),
                base,
                fields,
            },
            span: self.span_from(start),
        })
    }
    pub(super) fn token_tree(&mut self) -> P<TokenTree> {
        let start = self.peek().span.start;
        let mut raw_pos = self.raw.partition_point(|t| t.span.start < start);
        let tree = self.raw_token_tree(&mut raw_pos)?;
        while self.tokens[self.pos].span.start < tree.span.end {
            self.pos += 1;
        }
        self.last = tree.span;
        Ok(tree)
    }
    fn raw_token_tree(&mut self, raw_pos: &mut usize) -> P<TokenTree> {
        self.nested(|p| {
            let opening = p.raw[*raw_pos].clone();
            let start = opening.span;
            let (delimiter, close) = match opening.kind {
                TokenKind::Symbol(Symbol::LParen) => (Delimiter::Parenthesis, Symbol::RParen),
                TokenKind::Symbol(Symbol::LBracket) => (Delimiter::Bracket, Symbol::RBracket),
                TokenKind::Symbol(Symbol::LBrace) => (Delimiter::Brace, Symbol::RBrace),
                _ => {
                    return p.fail(
                        "E-PARSE-TOKEN-TREE",
                        "expected a balanced syntax island delimiter",
                    );
                }
            };
            *raw_pos += 1;
            let mut children = Vec::new();
            loop {
                let token = p.raw[*raw_pos].clone();
                if token.kind == TokenKind::Symbol(close) {
                    *raw_pos += 1;
                    return Ok(TokenTree {
                        delimiter,
                        children,
                        span: start.join(token.span),
                    });
                }
                match token.kind {
                    TokenKind::Symbol(Symbol::LParen | Symbol::LBracket | Symbol::LBrace) => {
                        children.push(TokenTreeEntry::Tree(p.raw_token_tree(raw_pos)?))
                    }
                    TokenKind::Eof
                    | TokenKind::Symbol(Symbol::RParen | Symbol::RBracket | Symbol::RBrace) => {
                        p.diagnostics.push(Diagnostic::error(
                            Phase::Parse,
                            "E-PARSE-TOKEN-TREE",
                            Some(token.span),
                            "unclosed or mismatched syntax island delimiter",
                        ));
                        return Err(());
                    }
                    _ => {
                        children.push(TokenTreeEntry::Token {
                            text: p.text(&token).to_owned(),
                            span: token.span,
                        });
                        *raw_pos += 1;
                    }
                }
            }
        })
    }
}

fn value_type_path(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Name(_) => true,
        ExprKind::Member {
            object,
            member: MemberName::Name(_),
        } => value_type_path(object),
        ExprKind::TypeApply { callee, .. } => value_type_path(callee),
        _ => false,
    }
}
fn expression_path(expr: &Expr) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::Name(name) => Some(vec![name.clone()]),
        ExprKind::Member {
            object,
            member: MemberName::Name(name),
        } => {
            let mut path = expression_path(object)?;
            path.push(name.clone());
            Some(path)
        }
        _ => None,
    }
}
fn binary(kind: TokenKind) -> Option<(u8, Option<BinaryOp>)> {
    let TokenKind::Symbol(symbol) = kind else {
        return None;
    };
    Some(match symbol {
        Symbol::Pipe => (1, None),
        Symbol::BarBar => (2, Some(BinaryOp::Or)),
        Symbol::AmpAmp => (3, Some(BinaryOp::And)),
        Symbol::EqEq => (4, Some(BinaryOp::Equal)),
        Symbol::BangEq => (4, Some(BinaryOp::NotEqual)),
        Symbol::Lt => (4, Some(BinaryOp::Less)),
        Symbol::LtEq => (4, Some(BinaryOp::LessEqual)),
        Symbol::Gt => (4, Some(BinaryOp::Greater)),
        Symbol::GtEq => (4, Some(BinaryOp::GreaterEqual)),
        Symbol::DotDot => (5, Some(BinaryOp::Range)),
        Symbol::Bar => (6, Some(BinaryOp::BitOr)),
        Symbol::Caret => (7, Some(BinaryOp::BitXor)),
        Symbol::Amp => (8, Some(BinaryOp::BitAnd)),
        Symbol::Shl => (9, Some(BinaryOp::ShiftLeft)),
        Symbol::Shr => (9, Some(BinaryOp::ShiftRight)),
        Symbol::Plus => (10, Some(BinaryOp::Add)),
        Symbol::Minus => (10, Some(BinaryOp::Subtract)),
        Symbol::PlusWrapping => (10, Some(BinaryOp::AddWrapping)),
        Symbol::MinusWrapping => (10, Some(BinaryOp::SubtractWrapping)),
        Symbol::Star => (11, Some(BinaryOp::Multiply)),
        Symbol::Slash => (11, Some(BinaryOp::Divide)),
        Symbol::Percent => (11, Some(BinaryOp::Remainder)),
        Symbol::StarWrapping => (11, Some(BinaryOp::MultiplyWrapping)),
        _ => return None,
    })
}

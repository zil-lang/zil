use super::*;

impl Parser<'_> {
    /// Only closure bodies are value-producing blocks. Keep ordinary Block
    /// parsing unchanged, then turn reachable tail paths into explicit returns.
    pub(super) fn closure_block(&mut self) -> P<Block> {
        let mut body = self.block(true)?;
        self.closure_tail(&mut body)?;
        Ok(body)
    }

    fn closure_tail(&mut self, body: &mut Block) -> P<()> {
        self.nested(|p| {
            let closing = Span::new(body.span.file, body.span.end - 1, body.span.end - 1);
            let Some(last) = body.statements.last_mut() else {
                body.statements.push(Stmt {
                    kind: StmtKind::Return(None),
                    span: closing,
                });
                return Ok(());
            };
            let after_value = match &last.kind {
                StmtKind::Expression(value) => value.span.end,
                _ => last.span.end,
            };
            let first = p.tokens.partition_point(|t| t.span.start < after_value);
            let discarded = p.tokens[first..]
                .iter()
                .take_while(|t| t.span.start < body.span.end)
                .any(|t| t.kind == TokenKind::Symbol(Symbol::Semicolon));
            if !discarded {
                match &mut last.kind {
                    StmtKind::Expression(_) => {
                        let StmtKind::Expression(value) =
                            std::mem::replace(&mut last.kind, StmtKind::Return(None))
                        else {
                            unreachable!("matched expression")
                        };
                        last.kind = StmtKind::Return(Some(value));
                        return Ok(());
                    }
                    StmtKind::If { .. } => return p.closure_tail_if(last),
                    _ => {}
                }
            }
            // Explicitly terminated paths have no Unit continuation. This also
            // covers match arms that all return, without making match a value
            // expression or changing the ordinary block tail rules.
            if !p.closure_path_ends(last)? {
                body.statements.push(Stmt {
                    kind: StmtKind::Return(None),
                    span: closing,
                });
            }
            Ok(())
        })
    }

    fn closure_path_ends(&mut self, statement: &Stmt) -> P<bool> {
        self.nested(|p| {
            Ok(match &statement.kind {
                StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => true,
                StmtKind::Unsafe(body) => p.closure_block_ends(body)?,
                StmtKind::If {
                    then_body,
                    else_body: Some(other),
                    ..
                } => {
                    p.closure_block_ends(then_body)?
                        && match other {
                            ElseBranch::Block(body) => p.closure_block_ends(body)?,
                            ElseBranch::If(statement) => p.closure_path_ends(statement)?,
                        }
                }
                StmtKind::Match { arms, .. } => {
                    if arms.is_empty() {
                        return Ok(false);
                    }
                    for arm in arms {
                        let ArrowBody::Block(body) = &arm.body else {
                            return Ok(false);
                        };
                        if !p.closure_block_ends(body)? {
                            return Ok(false);
                        }
                    }
                    // Exhaustiveness is established by the ordinary type pass.
                    true
                }
                _ => false,
            })
        })
    }

    fn closure_block_ends(&mut self, body: &Block) -> P<bool> {
        match body.statements.last() {
            Some(statement) => self.closure_path_ends(statement),
            None => Ok(false),
        }
    }

    fn closure_tail_if(&mut self, statement: &mut Stmt) -> P<()> {
        self.nested(|p| {
            let StmtKind::If {
                then_body,
                else_body,
                ..
            } = &mut statement.kind
            else {
                unreachable!("tail branch is an if statement")
            };
            p.closure_tail(then_body)?;
            match else_body {
                Some(ElseBranch::Block(body)) => p.closure_tail(body)?,
                Some(ElseBranch::If(nested)) => p.closure_tail_if(nested)?,
                None => {
                    let span =
                        Span::new(statement.span.file, statement.span.end, statement.span.end);
                    *else_body = Some(ElseBranch::Block(Block {
                        statements: vec![Stmt {
                            kind: StmtKind::Return(None),
                            span,
                        }],
                        span,
                    }));
                }
            }
            Ok(())
        })
    }

    pub(super) fn block(&mut self, arrow: bool) -> P<Block> {
        self.nested(|p| {
            let start = p.expect(Symbol::LBrace)?.span;
            p.scoped(false, |p| {
                let mut statements = Vec::new();
                p.separators();
                while !p.at(Symbol::RBrace) && p.peek().kind != TokenKind::Eof {
                    let before = p.pos;
                    if arrow && p.peek().kind == TokenKind::Identifier
                        && p.tokens.get(p.pos + 1).is_some_and(|t| t.kind == TokenKind::Symbol(Symbol::Colon)) {
                        let _: P<()> = p.fail("E-PARSE-RECORD-BLOCK", "`=> { ... }` is a statement block; parenthesize a record expression as `=> ({ ... })`");
                        p.recover(before, true);
                    } else {
                        match p.statement() {
                            Ok(stmt) => statements.push(stmt),
                            Err(()) => p.recover(before, true),
                        }
                    }
                    p.separators();
                }
                p.expect(Symbol::RBrace)?;
                Ok(Block { statements, span: p.span_from(start) })
            })
        })
    }
    pub(super) fn statement(&mut self) -> P<Stmt> {
        self.nested(|p| p.statement_inner())
    }
    fn statement_inner(&mut self) -> P<Stmt> {
        let start = self.peek().span;
        if self.at(Symbol::Hash)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|t| t.kind == TokenKind::Symbol(Symbol::LBracket))
        {
            return self.fail(
                "E-PARSE-ATTRIBUTE-ATTACHMENT",
                "attributes cannot attach to an executable statement",
            );
        }
        let kind = if self.at_word("let") || self.at_word("var") {
            let mutable = self.eat_word("var");
            if !mutable {
                self.expect_word("let")?;
            }
            let pattern = self.pattern()?;
            let ty = if self.eat(Symbol::Colon) {
                Some(self.ty()?)
            } else {
                None
            };
            self.expect(Symbol::Eq)?;
            self.newlines();
            let value = self.expr(true)?;
            self.end()?;
            StmtKind::Binding {
                mutable,
                pattern,
                ty,
                value,
            }
        } else if self.eat_word("return") {
            let value = if self.end_ahead() {
                None
            } else {
                Some(self.expr(true)?)
            };
            self.end()?;
            StmtKind::Return(value)
        } else if self.eat_word("yield") {
            let value = self.expr(true)?;
            self.end()?;
            StmtKind::Yield(value)
        } else if self.eat_word("break") {
            self.end()?;
            StmtKind::Break
        } else if self.eat_word("continue") {
            self.end()?;
            StmtKind::Continue
        } else if self.eat_word("defer") {
            self.newlines();
            StmtKind::Defer(self.block(false)?)
        } else if self.eat_word("if") {
            let condition = self.condition()?;
            self.newlines();
            let then_body = self.block(false)?;
            self.newlines();
            let else_body = if self.eat_word("else") {
                self.newlines();
                Some(if self.at_word("if") {
                    ElseBranch::If(Box::new(self.statement()?))
                } else {
                    ElseBranch::Block(self.block(false)?)
                })
            } else {
                None
            };
            StmtKind::If {
                condition,
                then_body,
                else_body,
            }
        } else if self.eat_word("while") {
            let condition = self.condition()?;
            self.newlines();
            let body = self.block(false)?;
            StmtKind::While { condition, body }
        } else if self.eat_word("for") {
            let pattern = self.pattern()?;
            self.expect_word("in")?;
            let iterable = self.expr(false)?;
            self.newlines();
            let body = self.block(false)?;
            StmtKind::For {
                pattern,
                iterable,
                body,
            }
        } else if self.eat_word("loop") {
            self.newlines();
            StmtKind::Loop(self.block(false)?)
        } else if self.eat_word("match") {
            let value = self.expr(false)?;
            self.newlines();
            self.expect(Symbol::LBrace)?;
            let arms = self.scoped(false, |p| {
                let mut arms = Vec::new();
                p.separators();
                while !p.at(Symbol::RBrace) {
                    let start = p.peek().span;
                    let pattern = p.pattern()?;
                    p.expect(Symbol::FatArrow)?;
                    p.newlines();
                    let body = p.arrow_body()?;
                    p.field_end()?;
                    arms.push(MatchArm {
                        pattern,
                        body,
                        span: p.span_from(start),
                    });
                    p.separators();
                }
                p.expect(Symbol::RBrace)?;
                Ok(arms)
            })?;
            StmtKind::Match { value, arms }
        } else if self.eat_word("with") {
            let name = self.ident()?;
            self.expect(Symbol::Eq)?;
            self.newlines();
            let value = self.expr(false)?;
            self.newlines();
            let body = self.block(false)?;
            StmtKind::With { name, value, body }
        } else if self.eat_word("unsafe") {
            self.newlines();
            StmtKind::Unsafe(self.block(false)?)
        } else if self.at_word("fn") {
            return self.fail("E-PARSE-NESTED-NAMED-FN", "named functions may not be declared in a statement block; use a module function or an anonymous closure");
        } else if self.meta_statement_ahead() {
            StmtKind::Meta(self.meta_statement()?)
        } else {
            let target = self.expr(true)?;
            let assignment = match self.peek().kind {
                TokenKind::Symbol(Symbol::Eq) => Some(AssignOp::Set),
                TokenKind::Symbol(Symbol::PlusEq) => Some(AssignOp::Add),
                TokenKind::Symbol(Symbol::MinusEq) => Some(AssignOp::Subtract),
                TokenKind::Symbol(Symbol::StarEq) => Some(AssignOp::Multiply),
                TokenKind::Symbol(Symbol::SlashEq) => Some(AssignOp::Divide),
                TokenKind::Symbol(Symbol::PercentEq) => Some(AssignOp::Remainder),
                TokenKind::Symbol(Symbol::AmpEq) => Some(AssignOp::BitAnd),
                TokenKind::Symbol(Symbol::BarEq) => Some(AssignOp::BitOr),
                TokenKind::Symbol(Symbol::CaretEq) => Some(AssignOp::BitXor),
                _ => None,
            };
            let result = if let Some(op) = assignment {
                self.bump();
                self.newlines();
                StmtKind::Assign {
                    target,
                    op,
                    value: self.expr(true)?,
                }
            } else {
                StmtKind::Expression(target)
            };
            self.end()?;
            result
        };
        Ok(Stmt {
            kind,
            span: self.span_from(start),
        })
    }
    fn end_ahead(&mut self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Newline
                | TokenKind::Eof
                | TokenKind::Symbol(Symbol::Semicolon | Symbol::RBrace)
        )
    }
    fn condition(&mut self) -> P<Condition> {
        if self.eat_word("let") {
            let pattern = self.pattern()?;
            self.expect(Symbol::Eq)?;
            self.newlines();
            Ok(Condition::Let {
                pattern,
                value: self.expr(false)?,
            })
        } else {
            Ok(Condition::Expression(self.expr(false)?))
        }
    }
    pub(super) fn pattern(&mut self) -> P<Pattern> {
        self.nested(|p| {
            let start = p.peek().span;
            let kind = if let Some(value) = p.literal() {
                PatternKind::Literal(value)
            } else if p.eat_word("_") {
                PatternKind::Wildcard
            } else if p.eat_word("ref") {
                let mutable = p.eat_word("mut");
                PatternKind::Reference {
                    mutable,
                    pattern: Box::new(p.pattern()?),
                }
            } else if p.eat(Symbol::LParen) {
                PatternKind::Tuple(p.pattern_list()?)
            } else {
                let path = p.qualified()?;
                if p.eat(Symbol::LParen) {
                    PatternKind::Constructor {
                        path,
                        arguments: p.pattern_list()?,
                    }
                } else if path.len() == 1 {
                    PatternKind::Name(path.into_iter().next().expect("nonempty path"))
                } else {
                    return p.fail(
                        "E-PARSE-PATTERN",
                        "qualified patterns require a constructor argument list",
                    );
                }
            };
            Ok(Pattern {
                kind,
                span: p.span_from(start),
            })
        })
    }
    fn pattern_list(&mut self) -> P<Vec<Pattern>> {
        self.scoped(true, |p| {
            let mut values = Vec::new();
            while !p.at(Symbol::RParen) {
                values.push(p.pattern()?);
                if !p.eat(Symbol::Comma) {
                    break;
                }
            }
            p.expect(Symbol::RParen)?;
            Ok(values)
        })
    }
    fn meta_statement_ahead(&mut self) -> bool {
        self.at(Symbol::Hash)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|t| matches!(t.text(&self.source.text), "if" | "for" | "emit"))
    }
    fn meta_statement(&mut self) -> P<MetaStatement> {
        self.expect(Symbol::Hash)?;
        if self.eat_word("if") {
            let condition = self.meta_condition()?;
            self.newlines();
            let then_body = self.block(false)?;
            self.newlines();
            let else_body = if self.eat_word("else") {
                self.newlines();
                Some(self.block(false)?)
            } else {
                None
            };
            Ok(MetaStatement::If {
                condition,
                then_body,
                else_body,
            })
        } else if self.eat_word("for") {
            let (binding, iterable) = self.meta_iteration()?;
            self.newlines();
            let body = self.block(false)?;
            Ok(MetaStatement::For {
                binding,
                iterable,
                body,
            })
        } else if self.eat_word("emit") {
            let value = self.expr(true)?;
            self.end()?;
            Ok(MetaStatement::Emit(value))
        } else {
            self.fail("E-PARSE-HASH-DISPATCH", "invalid compile-time statement")
        }
    }
    pub(super) fn meta_condition(&mut self) -> P<Expr> {
        self.expect(Symbol::LParen)?;
        self.scoped(true, |p| {
            let value = p.expr(true)?;
            p.expect(Symbol::RParen)?;
            Ok(value)
        })
    }
    pub(super) fn meta_iteration(&mut self) -> P<(String, Expr)> {
        self.expect(Symbol::LParen)?;
        self.scoped(true, |p| {
            let binding = p.ident()?;
            p.expect_word("in")?;
            let value = p.expr(true)?;
            p.expect(Symbol::RParen)?;
            Ok((binding, value))
        })
    }
}

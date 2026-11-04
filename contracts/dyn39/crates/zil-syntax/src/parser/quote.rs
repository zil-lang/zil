use super::*;

pub(super) struct QuoteFrame {
    pub active: bool,
    pub holes: Vec<QuoteHole>,
}

impl Parser<'_> {
    pub(super) fn in_quote(&self) -> bool {
        self.quote_frames.last().is_some_and(|f| f.active)
    }
    pub(super) fn quote_ahead(&mut self) -> bool {
        self.at_word("quote")
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|t| t.kind == TokenKind::Symbol(Symbol::Lt))
            && self
                .tokens
                .get(self.pos + 2)
                .is_some_and(|t| t.kind == TokenKind::Identifier)
            && self
                .tokens
                .get(self.pos + 3)
                .is_some_and(|t| t.kind == TokenKind::Symbol(Symbol::Gt))
            && self
                .tokens
                .get(self.pos + 4)
                .is_some_and(|t| t.kind == TokenKind::Symbol(Symbol::LBrace))
    }
    pub(super) fn splice_ahead(&self) -> bool {
        self.tokens.get(self.pos).is_some_and(|t| {
            t.kind == TokenKind::Identifier && t.text(&self.source.text) == "splice"
        })
    }
    fn quote_charge(&mut self, span: Span, amount: usize) -> P<()> {
        if self
            .nodes
            .checked_add(amount)
            .is_none_or(|n| n > self.limits.max_nodes)
        {
            self.diagnostics.push(Diagnostic::limit(
                Phase::Parse,
                "quote token/name budget exhausted",
                Some(span),
            ));
            return Err(());
        }
        self.nodes += amount;
        Ok(())
    }
    pub(super) fn quote_hole(&mut self, kind: QuoteHoleKind) -> P<u32> {
        if !self.in_quote() {
            return self.fail(
                "E-PARSE-QUOTE-HOLE",
                "a syntax hole requires its own quote layer",
            );
        }
        let start = self.expect(Symbol::Dollar)?.span;
        self.expect(Symbol::LParen)?;
        self.quote_frames.last_mut().unwrap().active = false;
        let value = self.scoped(true, |p| {
            let e = p.expr(true)?;
            p.expect(Symbol::RParen)?;
            Ok(e)
        });
        self.quote_frames.last_mut().unwrap().active = true;
        let value = value?;
        let span = self.span_from(start);
        self.quote_charge(span, 1)?;
        let index = u32::try_from(self.quote_frames.last().unwrap().holes.len()).map_err(|_| {
            self.diagnostics.push(Diagnostic::limit(
                Phase::Parse,
                "quote hole index overflow",
                Some(span),
            ));
        })?;
        self.quote_frames.last_mut().unwrap().holes.push(QuoteHole {
            kind,
            value: Box::new(value),
            span,
        });
        Ok(index)
    }
    pub(super) fn quote(&mut self) -> P<Expr> {
        let start = self.expect_word("quote")?.span;
        self.expect(Symbol::Lt)?;
        let category = self.bump();
        let category = match category.text(&self.source.text) {
            "Expr" => SyntaxCategory::Expr,
            "Type" => SyntaxCategory::Type,
            "StmtList" => SyntaxCategory::StmtList,
            "ItemList" => SyntaxCategory::ItemList,
            _ => {
                return self.fail(
                    "E-PARSE-QUOTE-CATEGORY",
                    "quote requires Expr, Type, StmtList or ItemList",
                );
            }
        };
        self.expect(Symbol::Gt)?;
        let body_pos = self.pos;
        let body_last = self.last;
        let raw = self.token_tree()?;
        self.pos = body_pos;
        self.last = body_last;
        self.quote_charge(start, 1)?;
        self.quote_frames.push(QuoteFrame {
            active: true,
            holes: Vec::new(),
        });
        let validation = match category {
            SyntaxCategory::Expr => self.scoped(true, |p| {
                p.expect(Symbol::LBrace)?;
                p.expr(true)?;
                p.expect(Symbol::RBrace)?;
                Ok(())
            }),
            SyntaxCategory::Type => self.scoped(true, |p| {
                p.expect(Symbol::LBrace)?;
                p.ty()?;
                p.expect(Symbol::RBrace)?;
                Ok(())
            }),
            SyntaxCategory::StmtList => self.block(false).map(|_| ()),
            SyntaxCategory::ItemList => self.scoped(false, |p| {
                p.expect(Symbol::LBrace)?;
                p.item_list(true);
                p.expect(Symbol::RBrace)?;
                Ok(())
            }),
        };
        let frame = self.quote_frames.pop().unwrap();
        validation?;
        let mut next = 0usize;
        let mut sites = Vec::new();
        let tree = self.quote_tree(raw, &frame.holes, &mut next, &mut sites)?;
        if next != frame.holes.len() {
            return self.fail(
                "E-PARSE-QUOTE-HOLE",
                "quote hole did not match its actual token tree",
            );
        }
        Ok(Expr {
            kind: ExprKind::Quote {
                category,
                template: QuoteTemplate {
                    tree,
                    holes: frame.holes,
                    name_sites: sites,
                },
            },
            span: self.span_from(start),
        })
    }
    fn quote_tree(
        &mut self,
        raw: TokenTree,
        holes: &[QuoteHole],
        next: &mut usize,
        sites: &mut Vec<QuoteNameSite>,
    ) -> P<QuoteTokenTree> {
        self.nested(|p| {
            let mut entries = raw.children.into_iter();
            let mut children = Vec::new();
            while let Some(entry) = entries.next() {
                match entry {
                    TokenTreeEntry::Token { text, span } => {
                        p.quote_charge(span, 1)?;
                        if holes.get(*next).is_some_and(|h| h.span.start == span.start) {
                            let Some(TokenTreeEntry::Tree(group)) = entries.next() else {
                                return p.fail(
                                    "E-PARSE-QUOTE-HOLE",
                                    "hole requires its actual parenthesized tokens",
                                );
                            };
                            if text != "$"
                                || group.delimiter != Delimiter::Parenthesis
                                || group.span.end != holes[*next].span.end
                            {
                                return p.fail(
                                    "E-PARSE-QUOTE-HOLE",
                                    "hole token range differs from its source",
                                );
                            }
                            children.push(QuoteTokenTreeEntry::Hole {
                                index: *next as u32,
                                span: holes[*next].span,
                            });
                            *next += 1;
                        } else {
                            let lookup = p.raw.len().checked_ilog2().map_or(1, |n| n as usize + 2);
                            p.quote_charge(span, lookup)?;
                            let index = p.raw.partition_point(|t| t.span.start < span.start);
                            if p.raw
                                .get(index)
                                .is_some_and(|t| t.kind == TokenKind::Identifier && t.span == span)
                            {
                                p.quote_charge(span, 1)?;
                                let token_index = u32::try_from(index).map_err(|_| {
                                    p.diagnostics.push(Diagnostic::limit(
                                        Phase::Parse,
                                        "quote source token index overflow",
                                        Some(span),
                                    ));
                                })?;
                                sites.push(QuoteNameSite { span, token_index });
                            }
                            children.push(QuoteTokenTreeEntry::Token { text, span });
                        }
                    }
                    TokenTreeEntry::Tree(tree) => children.push(QuoteTokenTreeEntry::Tree(
                        p.quote_tree(tree, holes, next, sites)?,
                    )),
                }
            }
            Ok(QuoteTokenTree {
                delimiter: raw.delimiter,
                children,
                span: raw.span,
            })
        })
    }
    pub(super) fn quote_dummy(&self, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Name(String::new()),
            span,
        }
    }
}

impl Parser<'_> {
    pub(super) fn quote_list_hole_ahead(&mut self) -> P<bool> {
        if !self.at(Symbol::Dollar)
            || self
                .tokens
                .get(self.pos + 1)
                .is_none_or(|t| t.kind != TokenKind::Symbol(Symbol::LParen))
        {
            return Ok(false);
        }
        let mut depth = 0usize;
        for i in self.pos + 1..self.tokens.len() {
            let token = self.tokens[i].clone();
            self.quote_charge(token.span, 1)?;
            match token.kind {
                TokenKind::Symbol(Symbol::LParen) => depth += 1,
                TokenKind::Symbol(Symbol::RParen) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(self.tokens.get(i + 1).is_some_and(|t| {
                            matches!(
                                t.kind,
                                TokenKind::Newline
                                    | TokenKind::Eof
                                    | TokenKind::Symbol(Symbol::RBrace)
                            )
                        }));
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
        }
        Ok(false)
    }
}

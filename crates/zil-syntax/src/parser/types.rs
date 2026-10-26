use super::*;

impl Parser<'_> {
    pub(super) fn ty(&mut self) -> P<TypeRef> {
        self.nested(|p| p.type_inner())
    }
    fn type_inner(&mut self) -> P<TypeRef> {
        let start = self.peek().span;
        if self.eat_word("extern") {
            let token = self.peek().clone();
            if token.kind != TokenKind::String {
                return self.fail(
                    "E-PARSE-FOREIGN-TYPE",
                    "external callback type requires an ABI string",
                );
            }
            self.bump();
            let abi = self.text(&token).to_owned();
            self.expect_word("fn")?;
            self.expect(Symbol::LParen)?;
            let params = self.type_list(Symbol::RParen)?;
            self.expect(Symbol::Arrow)?;
            self.newlines();
            let result = Box::new(self.ty()?);
            return Ok(TypeRef {
                kind: TypeKind::ForeignFunction {
                    abi,
                    params,
                    result,
                },
                span: self.span_from(start),
            });
        }
        if self.eat_word("fn") {
            self.expect(Symbol::LParen)?;
            let params = self.type_list(Symbol::RParen)?;
            self.expect(Symbol::Arrow)?;
            self.newlines();
            let result = Box::new(self.ty()?);
            return Ok(TypeRef {
                kind: TypeKind::Function { params, result },
                span: self.span_from(start),
            });
        }
        let mut members = vec![self.optional_type()?];
        while self.eat(Symbol::Bar) {
            self.newlines();
            members.push(self.optional_type()?);
        }
        let mut value = if members.len() == 1 {
            members.pop().expect("one type member")
        } else {
            TypeRef {
                kind: TypeKind::Union(members),
                span: self.span_from(start),
            }
        };
        if self.eat(Symbol::Bang) {
            self.newlines();
            let result = self.ty()?;
            value = TypeRef {
                kind: TypeKind::Error {
                    error: Box::new(value),
                    result: Box::new(result),
                },
                span: self.span_from(start),
            };
        }
        Ok(value)
    }
    fn optional_type(&mut self) -> P<TypeRef> {
        let value = self.type_primary()?;
        if self.eat(Symbol::Question) {
            let span = value.span.join(self.last);
            Ok(TypeRef {
                kind: TypeKind::Optional(Box::new(value)),
                span,
            })
        } else {
            Ok(value)
        }
    }
    fn type_primary(&mut self) -> P<TypeRef> {
        self.nested(|p| p.type_primary_inner())
    }
    fn type_primary_inner(&mut self) -> P<TypeRef> {
        let start = self.peek().span;
        let kind = if self.eat(Symbol::Amp) {
            self.newlines();
            let lifetime = if self.peek().kind == TokenKind::Lifetime {
                let token = self.bump();
                Some(self.text(&token)[1..].to_owned())
            } else {
                None
            };
            let mutable = self.eat_word("mut");
            let inner = Box::new(self.type_primary()?);
            TypeKind::Reference {
                lifetime,
                mutable,
                inner,
            }
        } else if self.eat(Symbol::LParen) {
            self.scoped(true, |p| {
                if p.eat(Symbol::RParen) {
                    return Ok(TypeKind::Tuple(Vec::new()));
                }
                let first = p.ty()?;
                if p.eat(Symbol::Comma) {
                    let mut values = vec![first];
                    while !p.at(Symbol::RParen) {
                        values.push(p.ty()?);
                        if !p.eat(Symbol::Comma) {
                            break;
                        }
                    }
                    p.expect(Symbol::RParen)?;
                    Ok(TypeKind::Tuple(values))
                } else {
                    p.expect(Symbol::RParen)?;
                    Ok(TypeKind::Group(Box::new(first)))
                }
            })?
        } else if self.eat(Symbol::LBracket) {
            self.scoped(true, |p| {
                let element = Box::new(p.ty()?);
                let length = if p.eat(Symbol::Semicolon) {
                    Some(Box::new(p.expr(true)?))
                } else {
                    None
                };
                p.expect(Symbol::RBracket)?;
                Ok(TypeKind::Array { element, length })
            })?
        } else if self.eat_word("dyn") {
            let path = self.qualified()?;
            let args = self.nominal_type_args(&path)?;
            TypeKind::Dyn { path, args }
        } else if self.eat_word("for") {
            self.expect(Symbol::Lt)?;
            let lifetime = self.scoped(true, |p| {
                p.expect_word("life")?;
                let lifetime = p.ident()?;
                p.close_angle()?;
                Ok(lifetime)
            })?;
            let inner = Box::new(self.type_primary()?);
            TypeKind::ForLifetime { lifetime, inner }
        } else if self.eat(Symbol::Hash) {
            if self.eat(Symbol::LParen) {
                let value = self.scoped(true, |p| {
                    let value = p.expr(true)?;
                    p.expect(Symbol::RParen)?;
                    Ok(value)
                })?;
                TypeKind::MetaEval(Box::new(value))
            } else if self.at(Symbol::LBrace) {
                TypeKind::MetaBlock(self.block(false)?)
            } else {
                return self.fail(
                    "E-PARSE-HASH-DISPATCH",
                    "type `#` must be immediately followed by `(` or `{`",
                );
            }
        } else {
            let path = self.qualified()?;
            let args = self.nominal_type_args(&path)?;
            TypeKind::Named { path, args }
        };
        Ok(TypeRef {
            kind,
            span: self.span_from(start),
        })
    }
    /// The public callable spelling has a parameter list. Its existing nominal
    /// AST representation still carries Args as a tuple plus the result type.
    fn nominal_type_args(&mut self, path: &[String]) -> P<Vec<GenericArg>> {
        let callable = path.len() == 1 && matches!(path[0].as_str(), "Fn" | "FnMut" | "FnOnce");
        if callable {
            if self.at(Symbol::Lt) {
                return self.fail(
                    "E-PARSE-CALLABLE-SYNTAX",
                    "use `Fn(A) -> R`, `FnMut(A) -> R` or `FnOnce(A) -> R`; Args tuples are internal",
                );
            }
            let start = self.expect(Symbol::LParen)?.span;
            let params = self.type_list(Symbol::RParen)?;
            let tuple = TypeRef {
                kind: TypeKind::Tuple(params),
                span: self.span_from(start),
            };
            self.expect(Symbol::Arrow)?;
            self.newlines();
            let result = self.ty()?;
            Ok(vec![GenericArg::Type(tuple), GenericArg::Type(result)])
        } else if self.at(Symbol::Lt) {
            self.type_args()
        } else {
            Ok(Vec::new())
        }
    }
    pub(super) fn return_type(&mut self) -> P<TypeRef> {
        let start = self.peek().span;
        if self.eat_word("impl") {
            let bounds = self.bounds()?;
            Ok(TypeRef {
                kind: TypeKind::Impl(bounds),
                span: self.span_from(start),
            })
        } else {
            self.ty()
        }
    }
    pub(super) fn type_list(&mut self, close: Symbol) -> P<Vec<TypeRef>> {
        self.scoped(true, |p| {
            let mut list = Vec::new();
            while !p.at(close) {
                list.push(p.ty()?);
                if !p.eat(Symbol::Comma) {
                    break;
                }
            }
            p.expect(close)?;
            Ok(list)
        })
    }
    pub(super) fn type_args(&mut self) -> P<Vec<GenericArg>> {
        self.expect(Symbol::Lt)?;
        self.scoped(true, |p| {
            let mut args = Vec::new();
            loop {
                let start = p.peek().span;
                if matches!(p.peek().kind, TokenKind::Integer | TokenKind::Float)
                    || p.at_word("true")
                    || p.at_word("false")
                {
                    let value = p.literal().expect("literal type argument checked above");
                    args.push(GenericArg::Const(Expr {
                        kind: ExprKind::Literal(value),
                        span: p.span_from(start),
                    }));
                } else {
                    args.push(GenericArg::Type(p.ty()?));
                }
                if !p.eat(Symbol::Comma) || p.at_angle_close() {
                    break;
                }
            }
            p.close_angle()?;
            Ok(args)
        })
    }
    pub(super) fn generics(&mut self) -> P<Vec<GenericParam>> {
        if !self.eat(Symbol::Lt) {
            return Ok(Vec::new());
        }
        self.scoped(true, |p| {
            let mut params = Vec::new();
            loop {
                let start = p.peek().span;
                let (name, kind) = if p.eat_word("life") {
                    (p.ident()?, GenericParamKind::Lifetime)
                } else if p.eat(Symbol::Hash) {
                    let name = p.ident()?;
                    p.expect(Symbol::Colon)?;
                    (name, GenericParamKind::Const(p.ty()?))
                } else {
                    let name = p.ident()?;
                    let bounds = if p.eat(Symbol::Colon) {
                        p.bounds()?
                    } else {
                        Vec::new()
                    };
                    (name, GenericParamKind::Type(bounds))
                };
                params.push(GenericParam {
                    name,
                    kind,
                    span: p.span_from(start),
                });
                if !p.eat(Symbol::Comma) || p.at_angle_close() {
                    break;
                }
            }
            p.close_angle()?;
            Ok(params)
        })
    }
    fn at_angle_close(&mut self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Symbol(Symbol::Gt | Symbol::Shr | Symbol::GtEq)
        )
    }
    fn close_angle(&mut self) -> P<()> {
        let token = self.peek();
        match token.kind {
            TokenKind::Symbol(Symbol::Gt) => {
                self.bump();
                Ok(())
            }
            TokenKind::Symbol(Symbol::Shr | Symbol::GtEq) => {
                // Keep the lexical tape untouched. Only the parser cursor splits
                // a type-context closer; value-context >> remains one operator.
                self.tokens[self.pos] = Token {
                    kind: TokenKind::Symbol(if token.kind == TokenKind::Symbol(Symbol::Shr) {
                        Symbol::Gt
                    } else {
                        Symbol::Eq
                    }),
                    span: Span::new(token.span.file, token.span.start + 1, token.span.end),
                };
                self.last = Span::new(token.span.file, token.span.start, token.span.start + 1);
                Ok(())
            }
            _ => self.fail(
                "E-PARSE-EXPECTED",
                "expected closing `>` in a type argument or parameter list",
            ),
        }
    }
    pub(super) fn bounds(&mut self) -> P<Vec<TypeRef>> {
        let mut bounds = vec![self.ty()?];
        while self.eat(Symbol::Plus) {
            self.newlines();
            bounds.push(self.ty()?);
        }
        Ok(bounds)
    }
    pub(super) fn where_clause(&mut self) -> P<Vec<BoundClause>> {
        if !self.eat_word("where") {
            return Ok(Vec::new());
        }
        let mut clauses = Vec::new();
        loop {
            let start = self.peek().span;
            let (subject, bounds, lifetime_bound) = if self.peek().kind == TokenKind::Lifetime {
                let token = self.bump();
                let name = self.text(&token)[1..].to_owned();
                self.expect(Symbol::Colon)?;
                if self.peek().kind != TokenKind::Lifetime {
                    return self.fail("E-PARSE-LIFETIME", "expected lifetime bound");
                }
                let token = self.bump();
                (
                    BoundSubject::Lifetime(name),
                    Vec::new(),
                    Some(self.text(&token)[1..].to_owned()),
                )
            } else {
                let ty = self.ty()?;
                self.expect(Symbol::Colon)?;
                (BoundSubject::Type(ty), self.bounds()?, None)
            };
            clauses.push(BoundClause {
                subject,
                bounds,
                lifetime_bound,
                span: self.span_from(start),
            });
            if !self.eat(Symbol::Comma) {
                break;
            }
            self.newlines();
        }
        Ok(clauses)
    }
}

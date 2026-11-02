use super::*;

impl Parser<'_> {
    pub(super) fn item_list(&mut self, in_braces: bool) -> Vec<Item> {
        let mut items = Vec::new();
        self.separators();
        while self.peek().kind != TokenKind::Eof && !(in_braces && self.at(Symbol::RBrace)) {
            let before = self.pos;
            match self.item(false) {
                Ok(item) => items.push(item),
                Err(()) => self.recover(before, in_braces),
            }
            self.separators();
        }
        items
    }
    fn item(&mut self, associated: bool) -> P<Item> {
        self.nested(|p| {
            let start = p.peek().span;
            let attributes = p.attributes()?;
            let public = p.eat_word("pub");
            let kind = if p.at_word("fn") { ItemKind::Function(p.function()?) }
                else if p.at_word("struct") || p.at_word("resource") || p.at_word("package") { ItemKind::Aggregate(p.aggregate()?) }
                else if p.at_word("enum") { ItemKind::Enum(p.enum_decl()?) }
                else if p.at_word("trait") { ItemKind::Trait(p.trait_decl()?) }
                else if p.at_word("impl") { ItemKind::Impl(p.impl_block()?) }
                else if p.at_word("type") { ItemKind::TypeAlias(p.type_alias(associated)?) }
                else if p.at_word("const") { ItemKind::Const(p.const_decl(associated)?) }
                else if p.eat_word("extern") {
                    if p.peek().kind != TokenKind::String { return p.fail("E-PARSE-EXTERN-ABI", "extern requires a string ABI"); }
                    let token = p.bump();
                    let abi = p.text(&token).to_owned();
                    ItemKind::Extern { abi, function: p.function()? }
                } else if p.at_word("import") || p.at_word("syntax") {
                    if public || !attributes.is_empty() { return p.fail("E-PARSE-IMPORT-MODIFIER", "import does not accept item attributes or pub"); }
                    ItemKind::Import(p.import()?)
                } else if p.at(Symbol::Hash) {
                    if public || !attributes.is_empty() { return p.fail("E-PARSE-META-MODIFIER", "compile-time declaration does not accept item attributes or pub"); }
                    ItemKind::Meta(p.meta_item()?)
                } else if !attributes.is_empty() {
                    return p.fail("E-PARSE-ATTRIBUTE-ATTACHMENT", "attributes require a following declaration, field, variant, or parameter");
                } else { return p.fail("E-PARSE-MODULE-STATEMENT", "module scope requires a declaration; executable statements belong inside a function"); };
            Ok(Item { kind, attributes, public, span: p.span_from(start) })
        })
    }
    fn import(&mut self) -> P<Import> {
        let syntax = self.eat_word("syntax");
        if syntax {
            self.expect_word("use")?;
        } else {
            self.expect_word("import")?;
        }
        let mut path = vec![self.ident()?];
        while self.eat(Symbol::Dot) {
            self.newlines();
            path.push(self.ident()?);
        }
        if self.at(Symbol::Slash) {
            return self.fail(
                "E-PARSE-IMPORT-SEPARATOR",
                "module paths use dots; replace '/' with '.' (for example import a.b)",
            );
        }
        let alias = if self.eat_word("as") {
            Some(self.ident()?)
        } else if syntax {
            return self.fail(
                "E-PARSE-SYNTAX-ALIAS",
                "syntax use requires an explicit alias",
            );
        } else {
            None
        };
        self.end()?;
        Ok(Import {
            path,
            alias,
            syntax,
        })
    }
    pub(super) fn attributes(&mut self) -> P<Vec<Attribute>> {
        let mut attributes = Vec::new();
        while self.at(Symbol::Hash)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|t| t.kind == TokenKind::Symbol(Symbol::LBracket))
        {
            let start = self.bump().span;
            self.expect(Symbol::LBracket)?;
            let (path, arguments, string_value) = self.scoped(true, |p| {
                let path = p.qualified()?;
                let mut arguments = Vec::new();
                let mut string_value = None;
                if p.eat(Symbol::LParen) {
                    while !p.at(Symbol::RParen) {
                        arguments.push(p.expr(true)?);
                        if !p.eat(Symbol::Comma) {
                            break;
                        }
                    }
                    p.expect(Symbol::RParen)?;
                } else if p.eat(Symbol::Eq) {
                    if p.peek().kind != TokenKind::String {
                        return p.fail(
                            "E-PARSE-ATTRIBUTE-VALUE",
                            "attribute equals form requires a string literal",
                        );
                    }
                    let token = p.bump();
                    string_value = Some(p.text(&token).to_owned());
                }
                p.expect(Symbol::RBracket)?;
                Ok((path, arguments, string_value))
            })?;
            attributes.push(Attribute {
                path,
                arguments,
                string_value,
                span: self.span_from(start),
            });
            self.newlines();
        }
        if !attributes.is_empty()
            && matches!(
                self.peek().kind,
                TokenKind::Eof
                    | TokenKind::Symbol(
                        Symbol::RBrace | Symbol::RParen | Symbol::Comma | Symbol::Semicolon
                    )
            )
        {
            return self.fail(
                "E-PARSE-ATTRIBUTE-ATTACHMENT",
                "attributes require a following declaration, field, variant, or parameter",
            );
        }
        Ok(attributes)
    }
    pub(super) fn function(&mut self) -> P<Function> {
        let start = self.expect_word("fn")?.span;
        let name = self.ident()?;
        let generics = self.generics()?;
        self.expect(Symbol::LParen)?;
        let params = self.scoped(true, |p| {
            let mut params = Vec::new();
            while !p.at(Symbol::RParen) {
                params.push(p.parameter()?);
                if !p.eat(Symbol::Comma) {
                    break;
                }
            }
            p.expect(Symbol::RParen)?;
            Ok(params)
        })?;
        let return_type = if self.eat(Symbol::Arrow) {
            self.newlines();
            Some(self.return_type()?)
        } else {
            None
        };
        self.newlines();
        let where_clause = self.where_clause()?;
        self.newlines();
        let contract = self.contract()?;
        self.newlines();
        let body = if self.at(Symbol::LBrace) {
            FunctionBody::Block(self.block(false)?)
        } else if self.eat(Symbol::FatArrow) {
            self.newlines();
            if self.at(Symbol::LBrace) {
                FunctionBody::Block(self.block(true)?)
            } else {
                let value = self.expr(true)?;
                self.end()?;
                FunctionBody::Expression(value)
            }
        } else if self.eat(Symbol::Semicolon) {
            FunctionBody::Declaration
        } else {
            return self.fail(
                "E-PARSE-FUNCTION-BODY",
                "function requires a block, arrow expression, or declaration semicolon",
            );
        };
        Ok(Function {
            name,
            generics,
            params,
            return_type,
            where_clause,
            contract,
            body,
            span: self.span_from(start),
        })
    }
    fn parameter(&mut self) -> P<Param> {
        let start = self.peek().span;
        let attributes = self.attributes()?;
        let give = self.eat_word("give");
        let name = self.ident()?;
        self.expect(Symbol::Colon)?;
        let ty = self.ty()?;
        Ok(Param {
            name,
            ty,
            give,
            attributes,
            span: self.span_from(start),
        })
    }
    fn aggregate(&mut self) -> P<Aggregate> {
        let kind = if self.eat_word("struct") {
            AggregateKind::Struct
        } else if self.eat_word("resource") {
            AggregateKind::Resource
        } else {
            self.expect_word("package")?;
            AggregateKind::Package
        };
        let name = self.ident()?;
        let generics = self.generics()?;
        self.newlines();
        self.expect(Symbol::LBrace)?;
        self.scoped(false, |p| {
            let mut fields = Vec::new();
            let mut methods = Vec::new();
            p.separators();
            while !p.at(Symbol::RBrace) {
                if p.peek().kind == TokenKind::Eof {
                    return p.fail("E-PARSE-EXPECTED", "unclosed aggregate declaration");
                }
                let start = p.peek().span;
                let attributes = p.attributes()?;
                let public = p.eat_word("pub");
                if p.at_word("fn") {
                    let function = p.function()?;
                    methods.push(Item {
                        kind: ItemKind::Function(function),
                        attributes,
                        public,
                        span: p.span_from(start),
                    });
                } else {
                    let manages = p.eat_word("manages");
                    if manages && kind != AggregateKind::Package {
                        return p.fail(
                            "E-PARSE-MANAGED-FIELD",
                            "manages fields are declared only in package",
                        );
                    }
                    if manages && public {
                        return p.fail(
                            "E-PARSE-MANAGED-FIELD",
                            "a managed field does not accept pub",
                        );
                    }
                    fields.push(p.field_after_prefix(start, attributes, public, manages)?);
                }
                p.separators();
            }
            p.expect(Symbol::RBrace)?;
            Ok(Aggregate {
                kind,
                name,
                generics,
                fields,
                methods,
            })
        })
    }
    fn field_after_prefix(
        &mut self,
        start: Span,
        attributes: Vec<Attribute>,
        public: bool,
        manages: bool,
    ) -> P<Field> {
        let name = self.ident()?;
        self.expect(Symbol::Colon)?;
        let ty = self.ty()?;
        let default = if self.eat(Symbol::Eq) {
            self.newlines();
            Some(self.expr(true)?)
        } else {
            None
        };
        self.field_end()?;
        Ok(Field {
            name,
            ty,
            default,
            public,
            manages,
            attributes,
            span: self.span_from(start),
        })
    }
    fn enum_decl(&mut self) -> P<EnumDecl> {
        self.expect_word("enum")?;
        let name = self.ident()?;
        let generics = self.generics()?;
        self.newlines();
        self.expect(Symbol::LBrace)?;
        self.scoped(false, |p| {
            let mut variants = Vec::new();
            p.separators();
            while !p.at(Symbol::RBrace) {
                let start = p.peek().span;
                let attributes = p.attributes()?;
                let name = p.ident()?;
                let fields = if p.eat(Symbol::LParen) {
                    let fields = p.scoped(true, |p| {
                        let mut fields = Vec::new();
                        p.newlines();
                        while !p.at(Symbol::RParen) {
                            let start = p.peek().span;
                            let manages = p.eat_word("manages");
                            let ty = p.ty()?;
                            fields.push(VariantTupleField {
                                ty,
                                manages,
                                span: p.span_from(start),
                            });
                            p.newlines();
                            if !p.eat(Symbol::Comma) {
                                break;
                            }
                            p.newlines();
                        }
                        p.expect(Symbol::RParen)?;
                        Ok(fields)
                    })?;
                    VariantFields::Tuple(fields)
                } else if p.eat(Symbol::LBrace) {
                    let mut fields = Vec::new();
                    p.separators();
                    while !p.at(Symbol::RBrace) {
                        let start = p.peek().span;
                        let attributes = p.attributes()?;
                        let public = p.eat_word("pub");
                        fields.push(p.field_after_prefix(start, attributes, public, false)?);
                        p.separators();
                    }
                    p.expect(Symbol::RBrace)?;
                    VariantFields::Record(fields)
                } else {
                    VariantFields::Unit
                };
                p.field_end()?;
                variants.push(Variant {
                    name,
                    fields,
                    attributes,
                    span: p.span_from(start),
                });
                p.separators();
            }
            p.expect(Symbol::RBrace)?;
            Ok(EnumDecl {
                name,
                generics,
                variants,
            })
        })
    }
    fn trait_decl(&mut self) -> P<TraitDecl> {
        self.expect_word("trait")?;
        let name = self.ident()?;
        let generics = self.generics()?;
        let bounds = if self.eat(Symbol::Colon) {
            self.bounds()?
        } else {
            Vec::new()
        };
        self.newlines();
        let members = self.members()?;
        Ok(TraitDecl {
            name,
            generics,
            bounds,
            members,
        })
    }
    fn impl_block(&mut self) -> P<ImplBlock> {
        self.expect_word("impl")?;
        let generics = self.generics()?;
        let first = self.ty()?;
        let (trait_type, target) = if self.eat_word("for") {
            (Some(first), self.ty()?)
        } else {
            (None, first)
        };
        let receiver_default = if self.eat(Symbol::LParen) {
            if trait_type.is_some() {
                return self.fail("E-PARSE-TRAIT-RECEIVER-DEFAULT", "a trait impl inherits receiver contracts from the trait and cannot declare a group receiver default");
            }
            Some(self.scoped(true, |p| {
                let param = p.parameter()?;
                if param.name != "self" || !param.attributes.is_empty() {
                    return p.fail(
                        "E-PARSE-RECEIVER-DEFAULT",
                        "impl receiver default must be `self: Type` with an optional give prefix",
                    );
                }
                p.expect(Symbol::RParen)?;
                Ok(param)
            })?)
        } else {
            None
        };
        self.newlines();
        let where_clause = self.where_clause()?;
        self.newlines();
        let members = self.members()?;
        Ok(ImplBlock {
            generics,
            trait_type,
            target,
            receiver_default,
            where_clause,
            members,
        })
    }
    fn members(&mut self) -> P<Vec<Item>> {
        self.expect(Symbol::LBrace)?;
        self.scoped(false, |p| {
            let mut members = Vec::new();
            p.separators();
            while !p.at(Symbol::RBrace) {
                let before = p.pos;
                let item = p.item(true)?;
                if !matches!(item.kind, ItemKind::Function(_) | ItemKind::TypeAlias(_) | ItemKind::Const(_)) {
                    p.pos = before;
                    return p.fail("E-PARSE-IMPL-MEMBER", "trait and impl bodies contain methods, associated types, or associated constants");
                }
                members.push(item);
                p.separators();
            }
            p.expect(Symbol::RBrace)?;
            Ok(members)
        })
    }
    fn type_alias(&mut self, associated: bool) -> P<TypeAlias> {
        self.expect_word("type")?;
        let name = self.ident()?;
        let generics = self.generics()?;
        let bounds = if associated && self.eat(Symbol::Colon) {
            self.bounds()?
        } else {
            Vec::new()
        };
        let value = if self.eat(Symbol::Eq) {
            self.newlines();
            Some(self.ty()?)
        } else if associated {
            None
        } else {
            return self.fail("E-PARSE-TYPE-ALIAS", "module type alias requires a value");
        };
        self.end()?;
        Ok(TypeAlias {
            name,
            generics,
            bounds,
            value,
        })
    }
    fn const_decl(&mut self, associated: bool) -> P<ConstDecl> {
        self.expect_word("const")?;
        let name = self.ident()?;
        let ty = if self.eat(Symbol::Colon) {
            Some(self.ty()?)
        } else if associated {
            return self.fail(
                "E-PARSE-ASSOCIATED-CONST",
                "associated constant requires an explicit type",
            );
        } else {
            None
        };
        let value = if self.eat(Symbol::Eq) {
            self.newlines();
            Some(self.expr(true)?)
        } else if associated {
            None
        } else {
            return self.fail("E-PARSE-CONST", "module constant requires an initializer");
        };
        self.end()?;
        Ok(ConstDecl { name, ty, value })
    }
    fn contract(&mut self) -> P<Vec<ContractClause>> {
        if !self.eat_word("contract") {
            return Ok(Vec::new());
        }
        self.expect(Symbol::LBrace)?;
        self.scoped(false, |p| {
            let mut clauses = Vec::new();
            p.separators();
            while !p.at(Symbol::RBrace) {
                let start = p.peek().span;
                let name = p.ident()?;
                if !matches!(
                    name.as_str(),
                    "result" | "requires" | "provides" | "effects" | "resume"
                ) {
                    return p.fail("E-PARSE-CONTRACT-CLAUSE", "unknown contract clause");
                }
                p.expect(Symbol::LParen)?;
                let terms = p.contract_terms()?;
                p.end()?;
                clauses.push(ContractClause {
                    name,
                    terms,
                    span: p.span_from(start),
                });
                p.separators();
            }
            p.expect(Symbol::RBrace)?;
            Ok(clauses)
        })
    }
    fn contract_terms(&mut self) -> P<Vec<ContractTerm>> {
        self.scoped(true, |p| {
            let mut terms = Vec::new();
            while !p.at(Symbol::RParen) {
                terms.push(p.contract_term()?);
                if !p.eat(Symbol::Comma) {
                    break;
                }
            }
            p.expect(Symbol::RParen)?;
            Ok(terms)
        })
    }
    fn contract_term(&mut self) -> P<ContractTerm> {
        self.nested(|p| {
            let start = p.peek().span;
            // `give` is a reserved expression prefix, and also a specified atom
            // of the finite contract DSL: `result(give)`.
            let path = if p.eat_word("give") {
                vec!["give".to_owned()]
            } else {
                p.qualified()?
            };
            let type_args = if p.at(Symbol::Lt) {
                p.type_args()?
            } else {
                Vec::new()
            };
            let mut arguments = Vec::new();
            let mut index = None;
            if p.eat(Symbol::LParen) {
                arguments = p.contract_terms()?;
            } else if p.eat(Symbol::LBracket) {
                index = Some(Box::new(p.contract_term()?));
                p.expect(Symbol::RBracket)?;
            }
            Ok(ContractTerm {
                path,
                type_args,
                arguments,
                index,
                span: p.span_from(start),
            })
        })
    }
    fn meta_item(&mut self) -> P<MetaItem> {
        self.expect(Symbol::Hash)?;
        if self.eat_word("if") {
            let condition = self.meta_condition()?;
            self.newlines();
            self.expect(Symbol::LBrace)?;
            let then_items = self.scoped(false, |p| {
                let items = p.item_list(true);
                p.expect(Symbol::RBrace)?;
                Ok(items)
            })?;
            self.newlines();
            let else_items = if self.eat_word("else") {
                self.newlines();
                self.expect(Symbol::LBrace)?;
                self.scoped(false, |p| {
                    let items = p.item_list(true);
                    p.expect(Symbol::RBrace)?;
                    Ok(items)
                })?
            } else {
                Vec::new()
            };
            Ok(MetaItem::If {
                condition,
                then_items,
                else_items,
            })
        } else if self.eat_word("for") {
            let (binding, iterable) = self.meta_iteration()?;
            self.newlines();
            self.expect(Symbol::LBrace)?;
            let items = self.scoped(false, |p| {
                let items = p.item_list(true);
                p.expect(Symbol::RBrace)?;
                Ok(items)
            })?;
            Ok(MetaItem::For {
                binding,
                iterable,
                items,
            })
        } else if self.eat_word("emit") {
            let value = self.expr(true)?;
            self.end()?;
            Ok(MetaItem::Emit(value))
        } else {
            self.fail(
                "E-PARSE-HASH-DISPATCH",
                "declaration `#` requires if, for, or emit on the same logical line",
            )
        }
    }
}

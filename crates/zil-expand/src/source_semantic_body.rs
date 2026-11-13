//! Preserve original operations before specialization and type inference.
//! Every returned node is minted here from the original syntax preorder. The
//! ledger is an emission correspondence, never a type or ownership proof.
use super::*;
use zil_base::NodeId;
#[path = "source_semantic_free_uses.rs"]
mod free_uses;

pub(super) enum OriginalBodyNodeKind {
    Expression,
    Binding,
}
pub(super) struct OriginalBodyNode {
    pub path: Vec<u32>,
    pub id: NodeId,
    pub kind: OriginalBodyNodeKind,
}
pub(super) struct OriginalBodyOutput {
    pub body: SemanticOriginalFunctionBody,
    pub nodes: Vec<OriginalBodyNode>,
}
struct Mint {
    next: u32,
    nodes: Vec<OriginalBodyNode>,
}
impl Builder<'_, '_, '_, '_> {
    fn body_id(
        &mut self,
        mint: &mut Mint,
        path: &[u32],
        kind: OriginalBodyNodeKind,
        span: Span,
        depth: usize,
    ) -> R<NodeId> {
        self.budget.node(span, depth)?;
        let id = NodeId(mint.next);
        mint.next = mint
            .next
            .checked_add(1)
            .ok_or_else(|| error(span, "original function operation namespace exhausted"))?;
        self.budget.identity_work(path.len(), span)?;
        self.budget.lineage_bytes(
            path.len()
                .checked_mul(std::mem::size_of::<u32>())
                .ok_or_else(|| error(span, "original operation path overflow"))?,
            span,
        )?;
        self.push(
            &mut mint.nodes,
            OriginalBodyNode {
                path: path.to_vec(),
                id,
                kind,
            },
            span,
        )?;
        Ok(id)
    }
    fn body_generated_id(&mut self, mint: &mut Mint, span: Span, depth: usize) -> R<NodeId> {
        self.budget.node(span, depth)?;
        let id = NodeId(mint.next);
        mint.next = mint
            .next
            .checked_add(1)
            .ok_or_else(|| error(span, "original structural wrapper namespace exhausted"))?;
        Ok(id)
    }
    fn body_box<T>(&mut self, value: T, span: Span) -> R<Box<T>> {
        self.budget.lineage_bytes(std::mem::size_of::<T>(), span)?;
        Ok(Box::new(value))
    }
    fn body_path(&mut self, path: &mut Vec<u32>, field: usize, span: Span) -> R<()> {
        let field =
            u32::try_from(field).map_err(|_| error(span, "original child ordinal overflow"))?;
        self.push(path, field, span)
    }
    fn body_type(
        &mut self,
        ty: &s::TypeRef,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        // Declaration headers and body annotations occupy disjoint original
        // occurrence namespaces. A body statement numbered one must never
        // alias the function's parameter-one lifetime occurrence.
        let mut occurrence = Vec::new();
        self.push(&mut occurrence, 40, ty.span)?;
        for field in path.iter().copied() {
            self.push(&mut occurrence, field, ty.span)?;
        }
        self.ty(ty, &mut occurrence, depth, false)
    }
    pub(super) fn original_body(&mut self, body: &s::FunctionBody) -> R<OriginalBodyOutput> {
        let mut mint = Mint {
            next: 0,
            nodes: Vec::new(),
        };
        let body = match body {
            s::FunctionBody::Declaration => SemanticOriginalFunctionBody::Declaration,
            s::FunctionBody::Block(block) => SemanticOriginalFunctionBody::Defined(
                self.body_block(block, &mut mint, &mut Vec::new(), 0)?,
            ),
            s::FunctionBody::Expression(value) => SemanticOriginalFunctionBody::Defined(
                self.body_return(value, &mut mint, &mut Vec::new(), 0)?,
            ),
        };
        Ok(OriginalBodyOutput {
            body,
            nodes: mint.nodes,
        })
    }
    pub(super) fn original_arrow_body(&mut self, body: &s::ArrowBody) -> R<OriginalBodyOutput> {
        self.original_arrow_body_depth(body, 0)
    }
    pub(super) fn original_arrow_body_depth(
        &mut self,
        body: &s::ArrowBody,
        depth: usize,
    ) -> R<OriginalBodyOutput> {
        let mut mint = Mint {
            next: 0,
            nodes: Vec::new(),
        };
        let body = SemanticOriginalFunctionBody::Defined(self.body_arrow(
            body,
            &mut mint,
            &mut Vec::new(),
            depth,
        )?);
        Ok(OriginalBodyOutput {
            body,
            nodes: mint.nodes,
        })
    }
    fn body_return(
        &mut self,
        value: &s::Expr,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalBlock> {
        // The Return is an explicit semantic lowering of the arrow. It has a
        // distinct node, but no fake original statement is added to the ledger.
        self.budget.node(value.span, depth)?;
        let id = NodeId(mint.next);
        mint.next = mint
            .next
            .checked_add(1)
            .ok_or_else(|| error(value.span, "original arrow namespace exhausted"))?;
        let value = self.body_expr(value, mint, path, depth + 1)?;
        let span = value.span;
        let mut statements = Vec::new();
        self.push(
            &mut statements,
            CoreStatement {
                id,
                kind: CoreStatementKind::Return(Some(value)),
                span,
            },
            span,
        )?;
        Ok(CoreBlock { statements, span })
    }
    fn body_arrow(
        &mut self,
        body: &s::ArrowBody,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalBlock> {
        match body {
            s::ArrowBody::Expression(value) => self.body_return(value, mint, path, depth),
            s::ArrowBody::Block(block) => self.body_block(block, mint, path, depth),
        }
    }
    fn body_block(
        &mut self,
        block: &s::Block,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalBlock> {
        self.budget.node(block.span, depth)?;
        let mut statements = Vec::new();
        for (index, statement) in block.statements.iter().enumerate() {
            self.body_path(path, index, statement.span)?;
            let statement = self.body_statement(statement, mint, path, depth + 1)?;
            path.pop();
            self.push(&mut statements, statement, block.span)?;
        }
        Ok(CoreBlock {
            statements,
            span: block.span,
        })
    }
    fn body_child_expr(
        &mut self,
        value: &s::Expr,
        field: usize,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalExpr> {
        self.body_path(path, field, value.span)?;
        let result = self.body_expr(value, mint, path, depth);
        path.pop();
        result
    }
    fn body_child_block(
        &mut self,
        body: &s::Block,
        field: usize,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalBlock> {
        self.body_path(path, field, body.span)?;
        let result = self.body_block(body, mint, path, depth);
        path.pop();
        result
    }
    fn body_else(
        &mut self,
        body: &Option<s::ElseBranch>,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<Option<SemanticOriginalBlock>> {
        let Some(body) = body else { return Ok(None) };
        self.body_path(
            path,
            2,
            match body {
                s::ElseBranch::Block(b) => b.span,
                s::ElseBranch::If(s) => s.span,
            },
        )?;
        let result = match body {
            s::ElseBranch::Block(block) => self.body_block(block, mint, path, depth)?,
            s::ElseBranch::If(statement) => {
                let statement = self.body_statement(statement, mint, path, depth)?;
                let span = statement.span;
                let mut statements = Vec::new();
                self.push(&mut statements, statement, span)?;
                CoreBlock { statements, span }
            }
        };
        path.pop();
        Ok(Some(result))
    }
    fn body_statement(
        &mut self,
        statement: &s::Stmt,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalStatement> {
        let span = statement.span;
        let id = self.body_id(mint, path, OriginalBodyNodeKind::Binding, span, depth)?;
        let kind = match &statement.kind {
            s::StmtKind::Binding {
                mutable,
                pattern,
                ty,
                value,
            } => {
                let ty = ty
                    .as_ref()
                    .map(|ty| self.body_type(ty, path, depth + 1))
                    .transpose()?;
                let pattern = self.body_pattern(pattern, path, depth + 1)?;
                let value = self.body_child_expr(value, 0, mint, path, depth + 1)?;
                match pattern {
                    CorePattern {
                        kind: CorePatternKind::Name(name),
                        ..
                    } => CoreStatementKind::Binding {
                        name,
                        mutable: *mutable,
                        ty,
                        initializer: Some(value),
                    },
                    pattern => CoreStatementKind::TupleBinding {
                        pattern,
                        mutable: *mutable,
                        ty,
                        initializer: value,
                    },
                }
            }
            s::StmtKind::Return(value) => CoreStatementKind::Return(
                value
                    .as_ref()
                    .map(|v| self.body_child_expr(v, 0, mint, path, depth + 1))
                    .transpose()?,
            ),
            s::StmtKind::Expression(value) => CoreStatementKind::Expression(self.body_child_expr(
                value,
                0,
                mint,
                path,
                depth + 1,
            )?),
            s::StmtKind::Break => CoreStatementKind::Break,
            s::StmtKind::Continue => CoreStatementKind::Continue,
            s::StmtKind::Defer(block) => {
                CoreStatementKind::Defer(self.body_child_block(block, 0, mint, path, depth + 1)?)
            }
            s::StmtKind::Unsafe(block) => {
                CoreStatementKind::Unsafe(self.body_child_block(block, 0, mint, path, depth + 1)?)
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let (pattern, value) = match condition {
                    s::Condition::Expression(value) => (None, value),
                    s::Condition::Let { pattern, value } => {
                        (Some(self.body_pattern(pattern, path, depth + 1)?), value)
                    }
                };
                let value = self.body_child_expr(value, 0, mint, path, depth + 1)?;
                let then_body = self.body_child_block(then_body, 1, mint, path, depth + 1)?;
                let else_body = self.body_else(else_body, mint, path, depth + 1)?;
                match pattern {
                    None => CoreStatementKind::If {
                        condition: value,
                        then_body,
                        else_body,
                    },
                    Some(pattern) => CoreStatementKind::IfLet {
                        pattern,
                        value,
                        then_body,
                        else_body,
                    },
                }
            }
            s::StmtKind::While { condition, body } => {
                let (pattern, value) = match condition {
                    s::Condition::Expression(value) => (None, value),
                    s::Condition::Let { pattern, value } => {
                        (Some(self.body_pattern(pattern, path, depth + 1)?), value)
                    }
                };
                let value = self.body_child_expr(value, 0, mint, path, depth + 1)?;
                let body = self.body_child_block(body, 1, mint, path, depth + 1)?;
                match pattern {
                    None => CoreStatementKind::While {
                        condition: value,
                        body,
                    },
                    Some(pattern) => CoreStatementKind::WhileLet {
                        pattern,
                        value,
                        body,
                    },
                }
            }
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => CoreStatementKind::ForEach {
                pattern: self.body_pattern(pattern, path, depth + 1)?,
                iterator: self.body_child_expr(iterable, 0, mint, path, depth + 1)?,
                body: self.body_child_block(body, 1, mint, path, depth + 1)?,
            },
            s::StmtKind::Match { value, arms } => {
                let value = self.body_child_expr(value, 0, mint, path, depth + 1)?;
                let mut result = Vec::new();
                for (index, arm) in arms.iter().enumerate() {
                    self.body_path(path, index + 1, arm.span)?;
                    let pattern = self.body_pattern(&arm.pattern, path, depth + 1)?;
                    let body = self.body_arrow(&arm.body, mint, path, depth + 1)?;
                    path.pop();
                    self.push(
                        &mut result,
                        CoreMatchArm {
                            pattern,
                            body,
                            span: arm.span,
                        },
                        arm.span,
                    )?;
                }
                CoreStatementKind::Match {
                    value,
                    arms: result,
                }
            }
            s::StmtKind::Assign {
                target,
                op: s::AssignOp::Set,
                value,
            } => CoreStatementKind::Assign {
                target: self.body_child_expr(target, 0, mint, path, depth + 1)?,
                value: self.body_child_expr(value, 1, mint, path, depth + 1)?,
            },
            s::StmtKind::Assign { target, op, value } => {
                let target = self.body_child_expr(target, 0, mint, path, depth + 1)?;
                let value = self.body_child_expr(value, 1, mint, path, depth + 1)?;
                let op = match op {
                    s::AssignOp::Add => BinaryOp::Add,
                    s::AssignOp::Subtract => BinaryOp::Subtract,
                    s::AssignOp::Multiply => BinaryOp::Multiply,
                    s::AssignOp::Divide => BinaryOp::Divide,
                    s::AssignOp::Remainder => BinaryOp::Remainder,
                    s::AssignOp::BitAnd => BinaryOp::BitAnd,
                    s::AssignOp::BitOr => BinaryOp::BitOr,
                    s::AssignOp::BitXor => BinaryOp::BitXor,
                    s::AssignOp::Set => {
                        return Err(error(span, "simple assignment dispatched as compound"));
                    }
                };
                CoreStatementKind::CompoundAssign { target, op, value }
            }
            s::StmtKind::Yield(value) => {
                CoreStatementKind::Yield(self.body_child_expr(value, 0, mint, path, depth + 1)?)
            }
            s::StmtKind::Loop(block) => {
                CoreStatementKind::Loop(self.body_child_block(block, 0, mint, path, depth + 1)?)
            }
            s::StmtKind::With { name, value, body } => CoreStatementKind::With {
                name: self.text(name, span)?,
                value: self.body_child_expr(value, 0, mint, path, depth + 1)?,
                body: self.body_child_block(body, 1, mint, path, depth + 1)?,
            },
            s::StmtKind::Meta(meta) => {
                let inner_id = self.body_generated_id(mint, span, depth + 1)?;
                let kind = match meta {
                    s::MetaStatement::If {
                        condition,
                        then_body,
                        else_body,
                    } => CoreStatementKind::If {
                        condition: self.body_child_expr(condition, 0, mint, path, depth + 1)?,
                        then_body: self.body_child_block(then_body, 1, mint, path, depth + 1)?,
                        else_body: else_body
                            .as_ref()
                            .map(|body| self.body_child_block(body, 2, mint, path, depth + 1))
                            .transpose()?,
                    },
                    s::MetaStatement::For {
                        binding,
                        iterable,
                        body,
                    } => CoreStatementKind::ForEach {
                        pattern: CorePattern {
                            kind: CorePatternKind::Name(self.text(binding, span)?),
                            span,
                        },
                        iterator: self.body_child_expr(iterable, 0, mint, path, depth + 1)?,
                        body: self.body_child_block(body, 1, mint, path, depth + 1)?,
                    },
                    s::MetaStatement::Emit(value) => CoreStatementKind::Expression(
                        self.body_child_expr(value, 0, mint, path, depth + 1)?,
                    ),
                };
                CoreStatementKind::CompileTime(self.body_box(
                    CoreStatement {
                        id: inner_id,
                        kind,
                        span,
                    },
                    span,
                )?)
            }
        };
        Ok(CoreStatement { id, kind, span })
    }
    fn body_const_formal(&mut self, ty: &s::TypeRef) -> R<bool> {
        let s::TypeKind::Named { path, args } = &ty.kind else {
            return Ok(false);
        };
        let [name] = path.as_slice() else {
            return Ok(false);
        };
        if !args.is_empty() {
            return Ok(false);
        }
        let mut owner = Some(self.owner);
        while let Some(index) = owner {
            let row = &self.inventory.declarations[index];
            for formal in row.parameters() {
                self.budget.node(formal.span, 0)?;
                self.budget.text(name, ty.span)?;
                self.budget.text(&formal.name, formal.span)?;
                if formal.name == *name {
                    return Ok(matches!(formal.kind, s::GenericParamKind::Const(_)));
                }
            }
            owner = row.parent;
        }
        Ok(false)
    }
    fn body_names(&mut self, names: &[String], span: Span) -> R<Vec<String>> {
        let mut result = Vec::new();
        for name in names {
            let name = self.text(name, span)?;
            self.push(&mut result, name, span)?;
        }
        Ok(result)
    }
    fn body_definition(&mut self, names: &[String], span: Span) -> R<SemanticDefinitionId> {
        if names.is_empty() {
            return Err(error(
                span,
                "explicit original constructor has an empty owner path",
            ));
        }
        if !self.builtin_shadow(names, span)?
            && let Some(definition) = self.compiler_nominal(names, span)?
        {
            return Ok(definition);
        }
        // Self in a member is the real lexical impl target, never the method
        // formal zero or the first same-shaped nominal in the current module.
        if names.len() == 1 && names[0] == "Self" {
            let mut owner = Some(self.owner);
            let inventory = self.inventory;
            while let Some(index) = owner {
                self.budget.node(span, 0)?;
                let row = &inventory.declarations[index];
                if let s::ItemKind::Impl(implementation) = &row.item.kind {
                    let mut target = &implementation.target;
                    let mut depth = 0;
                    while let s::TypeKind::Group(inner) = &target.kind {
                        self.budget.node(target.span, depth)?;
                        depth = depth
                            .checked_add(1)
                            .ok_or_else(|| error(span, "original Self type path overflow"))?;
                        target = inner;
                    }
                    let s::TypeKind::Named { path, .. } = &target.kind else {
                        return Err(unsupported(
                            span,
                            "original Self constructor requires the complete nominal impl target",
                        ));
                    };
                    let saved = self.owner;
                    self.owner = index;
                    let selected = self.named(path, span);
                    self.owner = saved;
                    return selected.map(|index| self.id(index));
                }
                owner = row.parent;
            }
            return Err(error(span, "original Self constructor has no lexical impl"));
        }
        let index = self.named(names, span)?;
        Ok(self.id(index))
    }
    fn body_tree(&mut self, tree: &s::TokenTree, depth: usize) -> R<CoreSyntaxTree> {
        self.budget.node(tree.span, depth)?;
        let mut children = Vec::new();
        for child in &tree.children {
            self.budget.node(tree.span, depth + 1)?;
            let child = match child {
                s::TokenTreeEntry::Token { text, span } => CoreSyntaxTreeEntry::Token {
                    text: self.text(text, *span)?,
                    span: *span,
                },
                s::TokenTreeEntry::Tree(tree) => {
                    CoreSyntaxTreeEntry::Tree(self.body_tree(tree, depth + 1)?)
                }
            };
            self.push(&mut children, child, tree.span)?;
        }
        Ok(CoreSyntaxTree {
            delimiter: body_delimiter(tree.delimiter),
            children,
            span: tree.span,
        })
    }
    fn body_quote_tree(&mut self, tree: &s::QuoteTokenTree, depth: usize) -> R<CoreSyntaxTree> {
        self.budget.node(tree.span, depth)?;
        let mut children = Vec::new();
        for child in &tree.children {
            self.budget.node(tree.span, depth + 1)?;
            let child = match child {
                s::QuoteTokenTreeEntry::Token { text, span } => CoreSyntaxTreeEntry::Token {
                    text: self.text(text, *span)?,
                    span: *span,
                },
                s::QuoteTokenTreeEntry::Tree(tree) => {
                    CoreSyntaxTreeEntry::Tree(self.body_quote_tree(tree, depth + 1)?)
                }
                s::QuoteTokenTreeEntry::Hole { index, span } => CoreSyntaxTreeEntry::Hole {
                    index: *index,
                    span: *span,
                },
            };
            self.push(&mut children, child, tree.span)?;
        }
        Ok(CoreSyntaxTree {
            delimiter: body_delimiter(tree.delimiter),
            children,
            span: tree.span,
        })
    }
    fn body_quote(
        &mut self,
        template: &s::QuoteTemplate,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalQuoteTemplate> {
        self.budget.node(template.tree.span, depth)?;
        let tree = self.body_quote_tree(&template.tree, depth + 1)?;
        let mut holes = Vec::new();
        self.body_path(path, 1, template.tree.span)?;
        for (index, hole) in template.holes.iter().enumerate() {
            let kind = match hole.kind {
                s::QuoteHoleKind::Expr => CoreQuoteHoleKind::Expr,
                s::QuoteHoleKind::Type => CoreQuoteHoleKind::Type,
                s::QuoteHoleKind::StmtList => CoreQuoteHoleKind::StmtList,
                s::QuoteHoleKind::ItemList => CoreQuoteHoleKind::ItemList,
                s::QuoteHoleKind::Ident { namespace, role } => CoreQuoteHoleKind::Ident {
                    namespace: match namespace {
                        s::QuoteNameNamespace::Value => CoreQuoteNameNamespace::Value,
                        s::QuoteNameNamespace::Type => CoreQuoteNameNamespace::Type,
                        s::QuoteNameNamespace::Member => CoreQuoteNameNamespace::Member,
                        s::QuoteNameNamespace::Module => CoreQuoteNameNamespace::Module,
                        s::QuoteNameNamespace::Label => CoreQuoteNameNamespace::Label,
                    },
                    role: match role {
                        s::QuoteNameRole::Binding => CoreQuoteNameRole::Binding,
                        s::QuoteNameRole::Reference => CoreQuoteNameRole::Reference,
                    },
                },
            };
            let value = self.body_child_expr(&hole.value, index, mint, path, depth + 1)?;
            let value = self.body_box(value, hole.span)?;
            self.push(
                &mut holes,
                CoreQuoteHole {
                    kind,
                    value,
                    span: hole.span,
                },
                hole.span,
            )?;
        }
        path.pop();
        let mut name_sites = Vec::new();
        for site in &template.name_sites {
            self.push(
                &mut name_sites,
                CoreQuoteNameSite {
                    span: site.span,
                    token_index: site.token_index,
                },
                site.span,
            )?;
        }
        Ok(CoreQuoteTemplate {
            tree,
            holes,
            name_sites,
        })
    }
    fn body_pattern(
        &mut self,
        pattern: &s::Pattern,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalPattern> {
        self.budget.node(pattern.span, depth)?;
        let kind = match &pattern.kind {
            s::PatternKind::Name(name) => CorePatternKind::Name(self.text(name, pattern.span)?),
            s::PatternKind::Wildcard => CorePatternKind::Wildcard,
            s::PatternKind::Literal(s::Literal::Integer(value)) => {
                CorePatternKind::Integer(self.text(value, pattern.span)?)
            }
            s::PatternKind::Literal(s::Literal::Bool(value)) => CorePatternKind::Bool(*value),
            s::PatternKind::Literal(s::Literal::Float(value)) => {
                CorePatternKind::Float(self.text(value, pattern.span)?)
            }
            s::PatternKind::Literal(s::Literal::String(value)) => {
                CorePatternKind::String(self.text(value, pattern.span)?)
            }
            s::PatternKind::Literal(s::Literal::Char(value)) => {
                CorePatternKind::Character(self.text(value, pattern.span)?)
            }
            s::PatternKind::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    let field = self.body_pattern(field, path, depth + 1)?;
                    self.push(&mut result, field, pattern.span)?;
                }
                CorePatternKind::Tuple(result)
            }
            s::PatternKind::Reference {
                mutable,
                pattern: inner,
            } => {
                let inner = self.body_pattern(inner, path, depth + 1)?;
                CorePatternKind::Reference {
                    mutable: *mutable,
                    pattern: self.body_box(inner, pattern.span)?,
                }
            }
            s::PatternKind::ErrorArm {
                is_error,
                pattern: inner,
            } => {
                let inner = self.body_pattern(inner, path, depth + 1)?;
                CorePatternKind::ErrorArm {
                    is_error: *is_error,
                    pattern: self.body_box(inner, pattern.span)?,
                }
            }
            s::PatternKind::Constructor {
                path: names,
                arguments,
            } => {
                let (variant, owner) = names.split_last().ok_or_else(|| {
                    error(pattern.span, "original constructor pattern has no variant")
                })?;
                let definition = if owner.is_empty() {
                    None
                } else {
                    Some(self.body_definition(owner, pattern.span)?)
                };
                let mut fields = Vec::new();
                for field in arguments {
                    let field = self.body_pattern(field, path, depth + 1)?;
                    self.push(&mut fields, field, pattern.span)?;
                }
                CorePatternKind::Constructor {
                    definition,
                    variant: self.text(variant, pattern.span)?,
                    fields,
                }
            }
            s::PatternKind::RecordConstructor {
                path: names,
                fields: original,
            } => {
                let definition = if names.is_empty() {
                    None
                } else {
                    Some(self.body_definition(names, pattern.span)?)
                };
                let mut fields = Vec::new();
                for field in original {
                    let name = self.text(&field.name, field.span)?;
                    let field = self.body_pattern(&field.pattern, path, depth + 1)?;
                    self.push(&mut fields, (name, field), pattern.span)?;
                }
                CorePatternKind::RecordConstructor { definition, fields }
            }
        };
        Ok(CorePattern {
            kind,
            span: pattern.span,
        })
    }
    fn body_expr(
        &mut self,
        value: &s::Expr,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalExpr> {
        let span = value.span;
        let id = self.body_id(mint, path, OriginalBodyNodeKind::Expression, span, depth)?;
        let kind = match &value.kind {
            s::ExprKind::Literal(s::Literal::Integer(text)) => {
                CoreExprKind::Integer(self.text(text, span)?)
            }
            s::ExprKind::Literal(s::Literal::Float(text)) => {
                CoreExprKind::Float(self.text(text, span)?)
            }
            s::ExprKind::Literal(s::Literal::String(text)) => {
                CoreExprKind::String(self.text(text, span)?)
            }
            s::ExprKind::Literal(s::Literal::Bool(value)) => CoreExprKind::Bool(*value),
            s::ExprKind::Literal(s::Literal::Char(text)) => {
                CoreExprKind::Character(self.text(text, span)?)
            }
            s::ExprKind::Group(inner) => {
                let inner = self.body_child_expr(inner, 0, mint, path, depth + 1)?;
                CoreExprKind::Group(self.body_box(inner, span)?)
            }
            s::ExprKind::Pipeline { input, target } => {
                let input = self.body_child_expr(input, 0, mint, path, depth + 1)?;
                let target = self.body_child_expr(target, 1, mint, path, depth + 1)?;
                CoreExprKind::Pipeline {
                    input: self.body_box(input, span)?,
                    target: self.body_box(target, span)?,
                }
            }
            s::ExprKind::Name(name) => CoreExprKind::Name(self.text(name, span)?),
            s::ExprKind::ReceiverMember(name) => {
                // The implicit receiver is one real language-level read.
                // Give that head its own original node so a capture site can
                // select it rather than selecting the field's resulting data.
                self.body_path(path, 0, span)?;
                let head = self.body_id(
                    mint,
                    path,
                    OriginalBodyNodeKind::Expression,
                    span,
                    depth + 1,
                )?;
                path.pop();
                let object = CoreExpr {
                    id: head,
                    kind: CoreExprKind::Name(self.text("self", span)?),
                    span,
                };
                CoreExprKind::Member {
                    object: self.body_box(object, span)?,
                    member: self.text(name, span)?,
                    owner: None,
                }
            }
            s::ExprKind::Ascribed { ty, value } => {
                let ty = self.body_type(ty, path, depth + 1)?;
                let value = self.body_child_expr(value, 0, mint, path, depth + 1)?;
                CoreExprKind::Ascribed {
                    ty,
                    value: self.body_box(value, span)?,
                }
            }
            s::ExprKind::Tuple(fields) | s::ExprKind::Array(fields) => {
                let mut result = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    let field = self.body_child_expr(field, index, mint, path, depth + 1)?;
                    self.push(&mut result, field, span)?;
                }
                if matches!(value.kind, s::ExprKind::Tuple(_)) {
                    CoreExprKind::Tuple(result)
                } else {
                    CoreExprKind::Array(result)
                }
            }
            s::ExprKind::Unary { op, operand } => {
                if matches!(op, s::UnaryOp::Borrow | s::UnaryOp::BorrowMut) {
                    self.budget
                        .lineage_bytes((path.len() + 1) * std::mem::size_of::<u32>(), span)?;
                    let mut occurrence = Vec::with_capacity(path.len() + 1);
                    occurrence.push(40);
                    occurrence.extend_from_slice(path);
                    // A real authored borrow introduces this original
                    // occurrence obligation. The subsequent TypeUse is tied
                    // to its actual emitted operation, never a fabricated
                    // root, Static class, initialized value or permission.
                    self.implicit(&occurrence, span)?;
                }
                let operand = self.body_child_expr(operand, 0, mint, path, depth + 1)?;
                let op = match op {
                    s::UnaryOp::Not => UnaryOp::Not,
                    s::UnaryOp::Negate => UnaryOp::Negate,
                    s::UnaryOp::BitNot => UnaryOp::BitNot,
                    s::UnaryOp::Dereference => UnaryOp::Dereference,
                    s::UnaryOp::Borrow => UnaryOp::Borrow,
                    s::UnaryOp::BorrowMut => UnaryOp::BorrowMut,
                    s::UnaryOp::Give => UnaryOp::Give,
                };
                CoreExprKind::Unary {
                    op,
                    operand: self.body_box(operand, span)?,
                }
            }
            s::ExprKind::Binary { op, left, right } => {
                let left = self.body_child_expr(left, 0, mint, path, depth + 1)?;
                let right = self.body_child_expr(right, 1, mint, path, depth + 1)?;
                if *op == s::BinaryOp::Range {
                    CoreExprKind::Range {
                        start: self.body_box(left, span)?,
                        end: self.body_box(right, span)?,
                    }
                } else {
                    let op = crate::binary(*op).ok_or_else(|| {
                        error(span, "ordinary binary operation has no exact Core operator")
                    })?;
                    CoreExprKind::Binary {
                        op,
                        left: self.body_box(left, span)?,
                        right: self.body_box(right, span)?,
                    }
                }
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                let callee = self.body_child_expr(callee, 0, mint, path, depth + 1)?;
                let mut result = Vec::new();
                for (index, argument) in arguments.iter().enumerate() {
                    self.body_path(path, index + 1, span)?;
                    let argument =
                        match argument {
                            s::GenericArg::Type(ty) if self.body_const_formal(ty)? => {
                                let s::TypeKind::Named { path: names, .. } = &ty.kind else {
                                    return Err(error(
                                        ty.span,
                                        "original const formal lost its actual name syntax",
                                    ));
                                };
                                let id = self.body_id(
                                    mint,
                                    path,
                                    OriginalBodyNodeKind::Expression,
                                    ty.span,
                                    depth + 1,
                                )?;
                                CoreGenericArgument::Const(CoreExpr {
                                    id,
                                    kind: CoreExprKind::Name(self.text(&names[0], ty.span)?),
                                    span: ty.span,
                                })
                            }
                            s::GenericArg::Type(ty) => {
                                CoreGenericArgument::Type(self.body_type(ty, path, depth + 1)?)
                            }
                            s::GenericArg::Lifetime(name) => {
                                CoreGenericArgument::Lifetime(self.text(name, span)?)
                            }
                            s::GenericArg::Const(value) => CoreGenericArgument::Const(
                                self.body_expr(value, mint, path, depth + 1)?,
                            ),
                            s::GenericArg::AssociatedBinding(_) => {
                                return Err(error(
                                    span,
                                    "associated binding is not an original call actual",
                                ));
                            }
                        };
                    path.pop();
                    self.push(&mut result, argument, span)?;
                }
                CoreExprKind::GenericApply {
                    callee: self.body_box(callee, span)?,
                    arguments: result,
                }
            }
            s::ExprKind::Call { callee, arguments } => {
                let callee = self.body_child_expr(callee, 0, mint, path, depth + 1)?;
                let mut result = Vec::new();
                for (index, argument) in arguments.iter().enumerate() {
                    let spread_id = if argument.spread {
                        Some(self.body_generated_id(mint, argument.span, depth + 1)?)
                    } else {
                        None
                    };
                    let value =
                        self.body_child_expr(&argument.value, index + 1, mint, path, depth + 1)?;
                    let value = if let Some(spread_id) = spread_id {
                        // This node is source Call syntax, not a second
                        // evaluation of its actual argument expression.
                        CoreExpr {
                            id: spread_id,
                            kind: CoreExprKind::Spread(self.body_box(value, span)?),
                            span: argument.span,
                        }
                    } else {
                        value
                    };
                    self.push(&mut result, value, span)?;
                }
                CoreExprKind::Call {
                    callee: self.body_box(callee, span)?,
                    arguments: result,
                }
            }
            s::ExprKind::Member { object, member } => {
                let object = self.body_child_expr(object, 0, mint, path, depth + 1)?;
                let (member, owner) = match member {
                    s::MemberName::Name(name) => (self.text(name, span)?, None),
                    s::MemberName::Constrained { name, owner } => (
                        self.text(name, span)?,
                        Some(self.body_type(owner, path, depth + 1)?),
                    ),
                    s::MemberName::TupleIndex(index) => {
                        self.budget.lineage_bytes(10, span)?;
                        (index.to_string(), None)
                    }
                };
                CoreExprKind::Member {
                    object: self.body_box(object, span)?,
                    member,
                    owner,
                }
            }
            s::ExprKind::Index { object, index } => {
                let object = self.body_child_expr(object, 0, mint, path, depth + 1)?;
                let index = self.body_child_expr(index, 1, mint, path, depth + 1)?;
                CoreExprKind::Index {
                    object: self.body_box(object, span)?,
                    index: self.body_box(index, span)?,
                }
            }
            s::ExprKind::Slice { object, start, end } => {
                let object = self.body_child_expr(object, 0, mint, path, depth + 1)?;
                let start = start
                    .as_ref()
                    .map(|v| self.body_child_expr(v, 1, mint, path, depth + 1))
                    .transpose()?
                    .map(|v| self.body_box(v, span))
                    .transpose()?;
                let end = end
                    .as_ref()
                    .map(|v| self.body_child_expr(v, 2, mint, path, depth + 1))
                    .transpose()?
                    .map(|v| self.body_box(v, span))
                    .transpose()?;
                CoreExprKind::Slice {
                    object: self.body_box(object, span)?,
                    start,
                    end,
                }
            }
            s::ExprKind::Record { ty, base, fields } => {
                let ty = if let Some(ty) = ty {
                    let ty = super::plan::type_expression(ty, self.budget)?;
                    Some(self.body_type(&ty, path, depth + 1)?)
                } else {
                    None
                };
                let base = base
                    .as_ref()
                    .map(|base| self.body_child_expr(base, 0, mint, path, depth + 1))
                    .transpose()?;
                let mut result = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    let name = self.text(&field.name, field.span)?;
                    let value = if let Some(value) = &field.value {
                        self.body_child_expr(value, index + 1, mint, path, depth + 1)?
                    } else {
                        // Shorthand is one real lexical read, not a guessed
                        // field type or a clone of a specialized operand.
                        self.body_path(path, index + 1, field.span)?;
                        let id = self.body_id(
                            mint,
                            path,
                            OriginalBodyNodeKind::Expression,
                            field.span,
                            depth + 1,
                        )?;
                        path.pop();
                        CoreExpr {
                            id,
                            kind: CoreExprKind::Name(self.text(&field.name, field.span)?),
                            span: field.span,
                        }
                    };
                    self.push(&mut result, (name, value), field.span)?;
                }
                if let Some(base) = base {
                    CoreExprKind::RecordUpdate {
                        ty,
                        base: self.body_box(base, span)?,
                        fields: result,
                    }
                } else {
                    match ty {
                        Some(ty) => CoreExprKind::Record { ty, fields: result },
                        None => CoreExprKind::RecordConstructor {
                            definition: None,
                            fields: result,
                        },
                    }
                }
            }
            s::ExprKind::Closure {
                move_capture,
                params,
                return_type,
                body,
                ..
            } => {
                let (parameters, return_type, body) = self.original_closure_payload(
                    params,
                    return_type.as_ref(),
                    body,
                    span,
                    depth + 1,
                )?;
                CoreExprKind::Closure {
                    move_capture: *move_capture,
                    parameters,
                    return_type,
                    body,
                }
            }
            s::ExprKind::Try(inner) => {
                let inner = self.body_child_expr(inner, 0, mint, path, depth + 1)?;
                CoreExprKind::Try(self.body_box(inner, span)?)
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.body_child_expr(condition, 0, mint, path, depth + 1)?;
                let then_value = self.body_child_expr(then_value, 1, mint, path, depth + 1)?;
                let else_value = self.body_child_expr(else_value, 2, mint, path, depth + 1)?;
                CoreExprKind::If {
                    condition: self.body_box(condition, span)?,
                    then_value: self.body_box(then_value, span)?,
                    else_value: self.body_box(else_value, span)?,
                }
            }
            s::ExprKind::MetaEval(inner) | s::ExprKind::MetaSplice(inner) => {
                let inner = self.body_child_expr(inner, 0, mint, path, depth + 1)?;
                let inner = self.body_box(inner, span)?;
                if matches!(value.kind, s::ExprKind::MetaEval(_)) {
                    CoreExprKind::CompileTime(inner)
                } else {
                    CoreExprKind::Splice(inner)
                }
            }
            s::ExprKind::MetaBlock(block) => CoreExprKind::CompileTimeBlock(
                self.body_child_block(block, 0, mint, path, depth + 1)?,
            ),
            s::ExprKind::Quote { category, template } => {
                let category = match category {
                    s::SyntaxCategory::Expr => CoreSyntaxCategory::Expr,
                    s::SyntaxCategory::Type => CoreSyntaxCategory::Type,
                    s::SyntaxCategory::StmtList => CoreSyntaxCategory::StmtList,
                    s::SyntaxCategory::ItemList => CoreSyntaxCategory::ItemList,
                };
                CoreExprKind::SyntaxQuote {
                    category,
                    template: self.body_quote(template, mint, path, depth + 1)?,
                }
            }
            s::ExprKind::SyntaxIsland { path: names, tree } => {
                let names = self.body_names(names, span)?;
                CoreExprKind::SyntaxIsland {
                    path: names,
                    tree: self.body_tree(tree, depth + 1)?,
                }
            }
        };
        Ok(CoreExpr { id, kind, span })
    }
}

fn body_delimiter(delimiter: s::Delimiter) -> CoreSyntaxDelimiter {
    match delimiter {
        s::Delimiter::Parenthesis => CoreSyntaxDelimiter::Parenthesis,
        s::Delimiter::Bracket => CoreSyntaxDelimiter::Bracket,
        s::Delimiter::Brace => CoreSyntaxDelimiter::Brace,
    }
}

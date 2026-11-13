//! Preserve original operations before specialization and type inference.
//! Every returned node is minted here from the original syntax preorder. The
//! ledger is an emission correspondence, never a type or ownership proof.
use super::*;
use zil_base::NodeId;

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
        let mut mint = Mint {
            next: 0,
            nodes: Vec::new(),
        };
        let body = SemanticOriginalFunctionBody::Defined(self.body_arrow(
            body,
            &mut mint,
            &mut Vec::new(),
            0,
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
            s::StmtKind::Assign { .. }
            | s::StmtKind::Yield(_)
            | s::StmtKind::Loop(_)
            | s::StmtKind::With { .. } => {
                return Err(unsupported(
                    span,
                    "original statement needs its complete shared operation carrier",
                ));
            }
            s::StmtKind::Meta(_) => {
                return Err(unsupported(
                    span,
                    "original compile-time statement requires its complete staged syntax inventory",
                ));
            }
        };
        Ok(CoreStatement { id, kind, span })
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
            _ => {
                return Err(unsupported(
                    pattern.span,
                    "original constructor or literal pattern requires its explicit source carrier",
                ));
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
        if let s::ExprKind::Group(inner) = &value.kind {
            self.budget.node(span, depth)?;
            // Parentheses add no evaluation. Record a path alias to the one
            // actual child entity, never a duplicate Core node or second read.
            let result = self.body_child_expr(inner, 0, mint, path, depth + 1)?;
            self.budget.identity_work(path.len(), span)?;
            self.budget.lineage_bytes(
                path.len()
                    .checked_mul(std::mem::size_of::<u32>())
                    .ok_or_else(|| error(span, "original group alias path overflow"))?,
                span,
            )?;
            self.push(
                &mut mint.nodes,
                OriginalBodyNode {
                    path: path.to_vec(),
                    id: result.id,
                    kind: OriginalBodyNodeKind::Expression,
                },
                span,
            )?;
            return Ok(result);
        }
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
            s::ExprKind::Name(name) => CoreExprKind::Name(self.text(name, span)?),
            s::ExprKind::ReceiverMember(name) => {
                CoreExprKind::ReceiverMember(self.text(name, span)?)
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
                let op = crate::binary(*op).ok_or_else(|| {
                    unsupported(
                        span,
                        "original range needs its complete shared operation carrier",
                    )
                })?;
                let left = self.body_child_expr(left, 0, mint, path, depth + 1)?;
                let right = self.body_child_expr(right, 1, mint, path, depth + 1)?;
                CoreExprKind::Binary {
                    op,
                    left: self.body_box(left, span)?,
                    right: self.body_box(right, span)?,
                }
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                let callee = self.body_child_expr(callee, 0, mint, path, depth + 1)?;
                let mut type_arguments = Vec::new();
                for (index, argument) in arguments.iter().enumerate() {
                    self.budget.node(span, depth + 1)?;
                    if matches!(argument, s::GenericArg::AssociatedBinding(_)) {
                        return Err(error(
                            span,
                            "associated binding is not an original call actual",
                        ));
                    }
                    if let s::GenericArg::Type(ty) = argument {
                        self.body_path(path, index + 1, ty.span)?;
                        let ty = self.body_type(ty, path, depth + 1)?;
                        path.pop();
                        self.push(&mut type_arguments, ty, span)?;
                    }
                    // Life/Const operands belong to the complete original Call
                    // recipe linked by site_nodes. They are never fake Types.
                }
                CoreExprKind::TypeApply {
                    callee: self.body_box(callee, span)?,
                    type_arguments,
                }
            }
            s::ExprKind::Call { callee, arguments } => {
                let callee = self.body_child_expr(callee, 0, mint, path, depth + 1)?;
                let mut result = Vec::new();
                for (index, argument) in arguments.iter().enumerate() {
                    let value =
                        self.body_child_expr(&argument.value, index + 1, mint, path, depth + 1)?;
                    let value = if argument.spread {
                        // This node is source Call syntax, not a second
                        // evaluation of its actual argument expression.
                        self.budget.node(argument.span, depth + 1)?;
                        let spread_id = NodeId(mint.next);
                        mint.next = mint
                            .next
                            .checked_add(1)
                            .ok_or_else(|| error(span, "original spread namespace exhausted"))?;
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
                        self.budget.node(field.span, depth + 1)?;
                        let id = NodeId(mint.next);
                        mint.next = mint.next.checked_add(1).ok_or_else(|| {
                            error(field.span, "original shorthand namespace exhausted")
                        })?;
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
                    let ty = ty.unwrap_or(SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Expression(id),
                    });
                    CoreExprKind::Record { ty, fields: result }
                }
            }
            s::ExprKind::Closure {
                move_capture,
                params,
                return_type,
                body,
                ..
            } => {
                let mut parameters = Vec::new();
                for (index, parameter) in params.iter().enumerate() {
                    self.body_path(path, 0, parameter.span)?;
                    self.body_path(path, index, parameter.span)?;
                    let ty = parameter
                        .ty
                        .as_ref()
                        .map(|ty| self.body_type(ty, path, depth + 1))
                        .transpose()?;
                    path.pop();
                    path.pop();
                    let name = self.text(&parameter.name, parameter.span)?;
                    self.push(
                        &mut parameters,
                        CoreClosureParameter {
                            name,
                            ty,
                            mode: PassingMode::Infer,
                            span: parameter.span,
                        },
                        parameter.span,
                    )?;
                }
                self.body_path(path, 1, span)?;
                let return_type = return_type
                    .as_ref()
                    .map(|ty| self.body_type(ty, path, depth + 1))
                    .transpose()?;
                path.pop();
                self.body_path(path, 2, span)?;
                let body = self.body_arrow(body, mint, path, depth + 1)?;
                path.pop();
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
            _ => {
                return Err(unsupported(
                    span,
                    "original expression needs its complete source operation or staged syntax carrier",
                ));
            }
        };
        Ok(CoreExpr { id, kind, span })
    }
}

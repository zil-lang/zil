//! Translate nested expression operands in source preorder without borrowing
//! the host call stack for every group, call argument or binary operation.
use super::*;

enum Pending<'a> {
    Group,
    BinaryLeft {
        op: s::BinaryOp,
        right: &'a s::Expr,
    },
    BinaryRight {
        op: s::BinaryOp,
        left: SemanticOriginalExpr,
    },
    Call {
        callee: Option<SemanticOriginalExpr>,
        arguments: &'a [s::Argument],
        values: Vec<SemanticOriginalExpr>,
        spread_id: Option<NodeId>,
    },
}

impl Builder<'_, '_, '_, '_> {
    pub(super) fn body_expr(
        &mut self,
        value: &s::Expr,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticOriginalExpr> {
        let original_path = path.len();
        let result = self.body_expr_tree(value, mint, path, depth);
        path.truncate(original_path);
        result
    }

    fn body_expr_tree(
        &mut self,
        mut value: &s::Expr,
        mint: &mut Mint,
        path: &mut Vec<u32>,
        root_depth: usize,
    ) -> R<SemanticOriginalExpr> {
        let mut parents = Vec::new();
        'visit: loop {
            let span = value.span;
            let depth = root_depth + parents.len();
            let mut completed = match &value.kind {
                s::ExprKind::Group(inner) => {
                    let id =
                        self.body_id(mint, path, OriginalBodyNodeKind::Expression, span, depth)?;
                    parents.push((id, span, Pending::Group));
                    self.body_path(path, 0, inner.span)?;
                    value = inner;
                    continue;
                }
                s::ExprKind::Binary { op, left, right } => {
                    let id =
                        self.body_id(mint, path, OriginalBodyNodeKind::Expression, span, depth)?;
                    parents.push((id, span, Pending::BinaryLeft { op: *op, right }));
                    self.body_path(path, 0, left.span)?;
                    value = left;
                    continue;
                }
                s::ExprKind::Call { callee, arguments } => {
                    if let Some(literal) =
                        crate::foreign::literal_constructor(callee, arguments, span)
                    {
                        let literal = literal?;
                        if let Constant::WideString(units) = &literal {
                            self.budget.identity_work(units.len(), span)?;
                            self.budget
                                .lineage_bytes(std::mem::size_of_val(units.as_slice()), span)?;
                        }
                        let id = self.body_id(
                            mint,
                            path,
                            OriginalBodyNodeKind::ForeignLiteral,
                            span,
                            depth,
                        )?;
                        CoreExpr {
                            id,
                            span,
                            kind: CoreExprKind::Literal(literal),
                        }
                    } else {
                        let id = self.body_id(
                            mint,
                            path,
                            OriginalBodyNodeKind::Expression,
                            span,
                            depth,
                        )?;
                        parents.push((
                            id,
                            span,
                            Pending::Call {
                                callee: None,
                                arguments,
                                values: Vec::new(),
                                spread_id: None,
                            },
                        ));
                        self.body_path(path, 0, callee.span)?;
                        value = callee;
                        continue;
                    }
                }
                _ => self.body_expr_other(value, mint, path, depth)?,
            };
            while let Some((id, span, parent)) = parents.pop() {
                path.pop();
                let kind = match parent {
                    Pending::Group => CoreExprKind::Group(self.body_box(completed, span)?),
                    Pending::BinaryLeft { op, right } => {
                        parents.push((
                            id,
                            span,
                            Pending::BinaryRight {
                                op,
                                left: completed,
                            },
                        ));
                        self.body_path(path, 1, right.span)?;
                        value = right;
                        continue 'visit;
                    }
                    Pending::BinaryRight { op, left } => {
                        if op == s::BinaryOp::Range {
                            CoreExprKind::Range {
                                start: self.body_box(left, span)?,
                                end: self.body_box(completed, span)?,
                            }
                        } else {
                            let op = crate::binary(op).ok_or_else(|| {
                                error(span, "ordinary binary operation has no exact Core operator")
                            })?;
                            CoreExprKind::Binary {
                                op,
                                left: self.body_box(left, span)?,
                                right: self.body_box(completed, span)?,
                            }
                        }
                    }
                    Pending::Call {
                        callee,
                        arguments,
                        mut values,
                        spread_id,
                    } => {
                        let callee = if let Some(callee) = callee {
                            let argument = &arguments[values.len()];
                            let argument_value = if let Some(id) = spread_id {
                                CoreExpr {
                                    id,
                                    kind: CoreExprKind::Spread(self.body_box(completed, span)?),
                                    span: argument.span,
                                }
                            } else {
                                completed
                            };
                            self.push(&mut values, argument_value, span)?;
                            callee
                        } else {
                            completed
                        };
                        if let Some(argument) = arguments.get(values.len()) {
                            let field = values.len() + 1;
                            let spread_id = if argument.spread {
                                Some(self.body_generated_id(
                                    mint,
                                    argument.span,
                                    root_depth + parents.len() + 1,
                                )?)
                            } else {
                                None
                            };
                            parents.push((
                                id,
                                span,
                                Pending::Call {
                                    callee: Some(callee),
                                    arguments,
                                    values,
                                    spread_id,
                                },
                            ));
                            self.body_path(path, field, argument.value.span)?;
                            value = &argument.value;
                            continue 'visit;
                        }
                        CoreExprKind::Call {
                            callee: self.body_box(callee, span)?,
                            arguments: values,
                        }
                    }
                };
                completed = CoreExpr { id, kind, span };
            }
            return Ok(completed);
        }
    }
}

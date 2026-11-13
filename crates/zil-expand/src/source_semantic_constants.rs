use super::*;

impl Builder<'_, '_, '_, '_> {
    fn carrier_copy(&mut self, value: &SemanticTypeRecipe, span: Span) -> R<SemanticTypeRecipe> {
        self.budget.node(span, 0)?;
        self.budget
            .lineage_bytes(std::mem::size_of::<SemanticTypeRecipe>(), span)?;
        match value {
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => Ok(value.clone()),
            SemanticTypeRecipe::Formal(_) => Ok(value.clone()),
            _ => Err(unsupported(
                span,
                "Const operation needs its original scalar carrier recipe",
            )),
        }
    }
    fn constant_carrier(
        &mut self,
        value: &s::Expr,
        expected: &SemanticTypeRecipe,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.budget.node(value.span, depth)?;
        if let s::ExprKind::Name(name) = &value.kind {
            if let Some(reference) = self.formal(name, 1, value.span)? {
                let SemanticScopeRef::Ancestor(distance) = reference.scope;
                let saved = self.owner;
                let mut owner = saved;
                for _ in 0..distance {
                    owner = self.inventory.declarations[owner]
                        .parent
                        .ok_or_else(|| error(value.span, "Const lexical ancestor is absent"))?;
                }
                let s::GenericParamKind::Const(carrier) = &self.inventory.declarations[owner]
                    .parameters()[reference.ordinal as usize]
                    .kind
                else {
                    unreachable!()
                };
                self.owner = owner;
                let normalized = self.ty(carrier, path, depth + 1, false);
                self.owner = saved;
                let mut normalized = normalized?;
                if let SemanticTypeRecipe::Formal(reference) = &mut normalized {
                    let SemanticScopeRef::Ancestor(old) = reference.scope;
                    reference.scope = SemanticScopeRef::Ancestor(
                        old.checked_add(distance)
                            .ok_or_else(|| error(value.span, "Const carrier ancestor overflow"))?,
                    );
                }
                return Ok(normalized);
            }
        }
        match &value.kind {
            s::ExprKind::Literal(s::Literal::Bool(_)) => Ok(SemanticTypeRecipe::Atom(Type::Bool)),
            s::ExprKind::Literal(s::Literal::Integer(text)) => {
                self.budget.text(text, value.span)?;
                for suffix in [
                    "usize", "isize", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
                ] {
                    if text.ends_with(suffix) {
                        return Ok(SemanticTypeRecipe::Atom(crate::type_ref(&s::TypeRef {
                            kind: s::TypeKind::Named {
                                path: vec![suffix.into()],
                                args: Vec::new(),
                            },
                            span: value.span,
                        })));
                    }
                }
                if matches!(expected, SemanticTypeRecipe::Atom(Type::Bool)) {
                    Ok(SemanticTypeRecipe::Atom(Type::I32))
                } else {
                    self.carrier_copy(expected, value.span)
                }
            }
            s::ExprKind::Literal(s::Literal::Float(text)) => {
                self.budget.text(text, value.span)?;
                if text.ends_with("f32") {
                    Ok(SemanticTypeRecipe::Atom(Type::F32))
                } else if text.ends_with("f64")
                    || matches!(expected, SemanticTypeRecipe::Atom(Type::Bool))
                {
                    Ok(SemanticTypeRecipe::Atom(Type::F64))
                } else {
                    self.carrier_copy(expected, value.span)
                }
            }
            s::ExprKind::Group(inner) => self.constant_carrier(inner, expected, path, depth + 1),
            _ => self.carrier_copy(expected, value.span),
        }
    }
    pub(super) fn constant(
        &mut self,
        value: &s::Expr,
        expected: &SemanticTypeRecipe,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticConstRecipe> {
        self.budget.node(value.span, depth)?;
        self.budget
            .lineage_bytes(std::mem::size_of::<SemanticConstRecipe>(), value.span)?;
        match &value.kind {
            s::ExprKind::Group(inner) => self.constant(inner, expected, path, depth + 1),
            s::ExprKind::Name(name) => self
                .formal(name, 1, value.span)?
                .map(SemanticConstRecipe::Formal)
                .ok_or_else(|| {
                    error(
                        value.span,
                        "Const expression has no original formal or associated declaration",
                    )
                }),
            s::ExprKind::Literal(s::Literal::Integer(text)) => Ok(
                SemanticConstRecipe::IntegerLiteral(self.text(text, value.span)?),
            ),
            s::ExprKind::Literal(s::Literal::Float(text)) => Ok(SemanticConstRecipe::FloatLiteral(
                self.text(text, value.span)?,
            )),
            s::ExprKind::Literal(s::Literal::Bool(value)) => {
                Ok(SemanticConstRecipe::Value(Constant::Bool(*value)))
            }
            s::ExprKind::Unary { op, operand } => {
                let op = match op {
                    s::UnaryOp::Negate => UnaryOp::Negate,
                    s::UnaryOp::BitNot => UnaryOp::BitNot,
                    s::UnaryOp::Not => UnaryOp::Not,
                    _ => {
                        return Err(unsupported(
                            value.span,
                            "Const recipe cannot borrow or move runtime storage",
                        ));
                    }
                };
                let carrier = self.constant_carrier(operand, expected, path, depth + 1)?;
                path.push(0);
                let value = self.constant(operand, &carrier, path, depth + 1)?;
                path.pop();
                Ok(SemanticConstRecipe::Unary {
                    op,
                    operand_type: Box::new(carrier),
                    value: Box::new(value),
                })
            }
            s::ExprKind::Binary { op, left, right } => {
                let op = crate::binary(*op).ok_or_else(|| {
                    unsupported(value.span, "Const range is not a scalar operation")
                })?;
                let carrier = self.constant_carrier(left, expected, path, depth + 1)?;
                path.push(0);
                let left = self.constant(left, &carrier, path, depth + 1)?;
                path.pop();
                path.push(1);
                let right = self.constant(right, &carrier, path, depth + 1)?;
                path.pop();
                Ok(SemanticConstRecipe::Binary {
                    op,
                    operand_type: Box::new(carrier),
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            s::ExprKind::Call { callee, arguments } => {
                let s::ExprKind::Name(name) = &callee.kind else {
                    return Err(unsupported(
                        value.span,
                        "Const recipe call needs its actual conversion target",
                    ));
                };
                if arguments.len() != 1 || arguments[0].spread {
                    return Err(error(
                        value.span,
                        "Const conversion needs one actual operand",
                    ));
                }
                let target = crate::type_ref(&s::TypeRef {
                    kind: s::TypeKind::Named {
                        path: vec![self.text(name, value.span)?],
                        args: Vec::new(),
                    },
                    span: value.span,
                });
                if !target.is_b0_scalar() {
                    return Err(unsupported(
                        value.span,
                        "Const recipe call is not a scalar conversion",
                    ));
                }
                let source =
                    self.constant_carrier(&arguments[0].value, expected, path, depth + 1)?;
                let operand = self.constant(&arguments[0].value, &source, path, depth + 1)?;
                Ok(SemanticConstRecipe::Convert {
                    source_type: Box::new(source),
                    target_type: Box::new(SemanticTypeRecipe::Atom(target)),
                    value: Box::new(operand),
                })
            }
            _ => Err(unsupported(
                value.span,
                "Const expression needs its complete original scalar/projection recipe",
            )),
        }
    }
}

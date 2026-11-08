//! Keep ordinary recursive array/projection callers independent of unrelated
//! scalar instruction and callable literal temporaries in host debug frames.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn scalar_expr_integer_convert(
        &mut self,
        e: &CoreExpr,
        _expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let CoreExprKind::IntegerConvert { value, target } = &e.kind else {
            unreachable!("scalar expression variant dispatch")
        };
        let result = {
            if !target.is_integer() {
                return Err(error(
                    "E-INTEGER-CONVERSION",
                    e.span,
                    "conversion target must be u8, u16, i32 or i64",
                ));
            }
            // The conversion target never changes the operand's type.
            let (value, source) = self.expr(value, None)?;
            if !source.is_integer() {
                return Err(error(
                    "E-INTEGER-CONVERSION",
                    e.span,
                    "conversion operand must be an integer",
                ));
            }
            let result = self.value(
                InstructionKind::ConvertInteger {
                    value,
                    target: target.clone(),
                },
                target.clone(),
                e.span,
            )?;
            (result, target.clone())
        };
        Ok(result)
    }
    pub(super) fn scalar_expr_name(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let CoreExprKind::Name(name) = &e.kind else {
            unreachable!("scalar expression variant dispatch")
        };
        let result = {
            if let Some(local) = self.lookup(name) {
                if self.closure_slots.contains_key(&local) {
                    return self.take_closure_slot(local, e.span);
                }
                if let Some(value) = self.take_sum_slot(local, expected, e.span, false)? {
                    return Ok(value);
                }
                let actual = &self.function.locals[local.0 as usize].ty;
                if stored::shared_array_related(actual) {
                    self.closures.snapshot_type(actual, 1, 0, e.span)?;
                }
                let ty = self.function.locals[local.0 as usize].ty.clone();
                if let Type::Managed(payload) = ty {
                    let desc = Type::Desc(payload);
                    return Ok((
                        self.value(
                            InstructionKind::DescribeLocal { local },
                            desc.clone(),
                            e.span,
                        )?,
                        desc,
                    ));
                }
                let kind = if let Some(index) = self.stored_copy_captures.get(&local) {
                    InstructionKind::ReadCapture { index: *index }
                } else if matches!(ty, Type::Package { .. })
                    || matches!(&ty, Type::BorrowedStorage { storage, .. }
                        if matches!(storage.as_ref(), Type::Package { .. }))
                    || packages::owned_array_supported(&ty)
                    || errors::owned(&ty)
                    || owned_shapes::sum(&ty)
                {
                    InstructionKind::TakeLocal { local }
                } else {
                    InstructionKind::Load(local)
                };
                if stored::shared_array_related(&ty) {
                    self.closures.snapshot_type(&ty, 1, 0, e.span)?;
                }
                let value = self.value(kind, ty.clone(), e.span)?;
                if let Some(inner) = self.borrowed_captures.get(&local).cloned() {
                    (
                        self.value(
                            InstructionKind::ReadAddress { address: value },
                            inner.clone(),
                            e.span,
                        )?,
                        inner,
                    )
                } else {
                    (value, ty)
                }
            } else {
                self.function_reference(e, expected)?
            }
        };
        if let Some(expected) = expected {
            self.same(&result.1, expected, e.span)?;
        }
        Ok(result)
    }
    pub(super) fn scalar_expr_dereference(
        &mut self,
        e: &CoreExpr,
        _expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let CoreExprKind::Unary {
            op: UnaryOp::Dereference,
            operand,
        } = &e.kind
        else {
            unreachable!("scalar expression variant dispatch")
        };
        let result = {
            let (address, ty) = self.expr(operand, None)?;
            let Type::Address { inner, .. } = ty else {
                return Err(error(
                    "E-OPERATOR-TYPE",
                    e.span,
                    "dereference requires an address",
                ));
            };
            if !inner.is_copy_value() {
                return Err(unsupported(
                    e.span,
                    "dereference requires a finite Copy pointee",
                ));
            }
            let inner = *inner;
            (
                self.value(
                    InstructionKind::ReadAddress { address },
                    inner.clone(),
                    e.span,
                )?,
                inner,
            )
        };
        Ok(result)
    }
    pub(super) fn scalar_expr_unary(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let CoreExprKind::Unary { op, operand } = &e.kind else {
            unreachable!("scalar expression variant dispatch")
        };
        let result = {
            if !matches!(op, UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot) {
                return Err(unsupported(e.span, "address/resource unary operation"));
            }
            let want = if *op == UnaryOp::Not {
                Some(&Type::Bool)
            } else {
                expected
            };
            let (v, t) = self.expr(operand, want)?;
            if (*op == UnaryOp::Not && t != Type::Bool)
                || (*op != UnaryOp::Not
                    && !t.is_integer()
                    && !(*op == UnaryOp::Negate && t.is_float()))
            {
                return Err(error(
                    "E-OPERATOR-TYPE",
                    e.span,
                    "unary operator operand type",
                ));
            }
            (
                self.value(
                    InstructionKind::Unary {
                        op: *op,
                        operand: v,
                    },
                    t.clone(),
                    e.span,
                )?,
                t,
            )
        };
        Ok(result)
    }
    pub(super) fn scalar_expr_binary(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let CoreExprKind::Binary { op, left, right } = &e.kind else {
            unreachable!("scalar expression variant dispatch")
        };
        let result = {
            let comparison = matches!(
                op,
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
            );
            let operand_hint = self.known_type(left).or_else(|| self.known_type(right));
            let numeric_expected = if comparison {
                operand_hint.as_ref()
            } else {
                expected.or(operand_hint.as_ref())
            };
            let (l, lt) = self.expr(left, numeric_expected)?;
            let (r, rt) = self.expr(right, Some(&lt))?;
            self.same(&rt, &lt, e.span)?;
            let equality = matches!(op, BinaryOp::Equal | BinaryOp::NotEqual);
            if !lt.is_integer()
                && !(lt.is_float() && floats::binary(*op))
                && !(equality && matches!(lt, Type::Bool | Type::ForeignHandle))
            {
                return Err(error(
                    "E-OPERATOR-TYPE",
                    e.span,
                    "binary operator operand type",
                ));
            }
            let t = if comparison { Type::Bool } else { lt };
            (
                self.value(
                    InstructionKind::Binary {
                        op: *op,
                        left: l,
                        right: r,
                    },
                    t.clone(),
                    e.span,
                )?,
                t,
            )
        };
        Ok(result)
    }
    pub(super) fn scalar_expr_closure(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let CoreExprKind::Closure {
            move_capture,
            parameters,
            return_type,
            body,
        } = &e.kind
        else {
            unreachable!("scalar expression variant dispatch")
        };
        let result = {
            let input = closures::Input {
                defer_action: false,
                move_capture: *move_capture,
                parameters,
                result: return_type.as_ref(),
                body,
                span: e.span,
            };
            self.closure_expression(input, expected, Some(e.id))?
        };
        Ok(result)
    }
}

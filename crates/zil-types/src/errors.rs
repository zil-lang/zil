//! Finite Copy/complete-Package/owned-success error unions. The operand is one
//! SSA value; cleanup on the failure edge never mutates the success-edge
//! builder state or turns a recoverable failure into a trap.
use super::*;

pub(super) fn owned(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if matches!(error.as_ref(), Type::Package { .. })
            || matches!(result.as_ref(), Type::Package { .. } | Type::Managed(_) | Type::OwnedDyn { .. })
            || (matches!(error.as_ref(), Type::Enum { .. }) && !error.is_copy_value())
            || (matches!(result.as_ref(), Type::Enum { .. }) && !result.is_copy_value()))
        || (matches!(ty, Type::ErrorUnion { .. }) && owned_shapes::sum(ty))
}

/// Structural classification only. `supported` separately admits the finite
/// Copy error and the actual direct resource payload.
pub(super) fn managed_success(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::Managed(_)))
}

pub(super) fn supported(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if (((error.is_copy_value() || stored::sum_value(error)) && (result.is_copy_value() || stored::sum_value(result)))
            || ((error.is_copy_value() || packages::supported(error))
            && (result.is_copy_value() || packages::supported(result)))
            || (error.is_copy_value() && matches!(result.as_ref(), Type::Managed(_))
                && resources::supported(result))
            || (*error.as_ref() == super::dyn_dispatch::create_error()
                && matches!(result.as_ref(), Type::OwnedDyn { .. }))))
        || dynamic_store::product_protocol(ty)
        || owned_shapes::error_union(ty)
}

pub(super) fn injection(actual: &Type, union: &Type, span: Span) -> R<Option<bool>> {
    if actual == union {
        return Ok(None);
    }
    let Type::ErrorUnion {
        error: failure,
        result,
    } = union
    else {
        unreachable!("expected error union");
    };
    match (actual == failure.as_ref(), actual == result.as_ref()) {
        (true, false) => Ok(Some(true)),
        (false, true) => Ok(Some(false)),
        (true, true) => Err(error(
            "E-ERROR-INJECTION-AMBIGUOUS",
            span,
            "the value matches both error and success; equal payload types need an explicit tagged value",
        )),
        _ => Err(error(
            "E-ERROR-INJECTION",
            span,
            format!(
                "{} matches neither payload of {}",
                actual.name(),
                union.name()
            ),
        )),
    }
}

impl Builder<'_, '_> {
    pub(super) fn explicit_error_constructor(
        &mut self,
        name: &str,
        type_arguments: &[Type],
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let [error_type, result_type] = type_arguments else {
            return Err(error(
                "E-ERROR-CONSTRUCTOR",
                span,
                "error constructor needs explicit error and result types",
            ));
        };
        let [payload] = arguments else {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "error constructor needs one complete payload",
            ));
        };
        let union = Type::ErrorUnion {
            error: Box::new(error_type.clone()),
            result: Box::new(result_type.clone()),
        };
        if !supported(&union) {
            return Err(unsupported(
                span,
                "error constructor payloads require an implemented complete ErrorUnion shape",
            ));
        }
        let is_error = name == "error_err";
        let selected = if is_error { error_type } else { result_type };
        let (value, actual) = self.expr(payload, Some(selected))?;
        self.same(&actual, selected, payload.span)?;
        let value = self.value(
            InstructionKind::MakeErrorUnion { is_error, value },
            union.clone(),
            span,
        )?;
        if let Some(expected) = expected {
            self.same(&union, expected, span)?;
        }
        Ok((value, union))
    }

    pub(super) fn inject_error_union(
        &mut self,
        value: ValueId,
        actual: Type,
        union: &Type,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let Some(is_error) = injection(&actual, union, span)? else {
            return Ok((value, actual));
        };
        let value = self.value(
            InstructionKind::MakeErrorUnion { is_error, value },
            union.clone(),
            span,
        )?;
        Ok((value, union.clone()))
    }

    pub(super) fn return_value(&mut self, value: Option<ValueId>, span: Span) -> R<()> {
        self.end_unsafe_regions(0, span)?;
        self.end_scopes_kind(0, CleanupExitKind::Return, span)?;
        self.terminate(Terminator::Return(value));
        Ok(())
    }

    pub(super) fn try_expr(&mut self, operand: &CoreExpr, span: Span) -> R<(ValueId, Type)> {
        let enclosing = self.function.return_type.clone();
        let Type::ErrorUnion {
            error: expected_error,
            ..
        } = &enclosing
        else {
            return Err(error(
                "E-TRY-RETURN-TYPE",
                span,
                "? requires an enclosing error-union result",
            ));
        };
        let (value, ty) = self.expr(operand, None)?;
        let consuming = owned(&ty);
        let Type::ErrorUnion {
            error: failure,
            result,
        } = ty
        else {
            return Err(error(
                "E-TRY-NOT-ERROR",
                span,
                "? requires an error-union operand",
            ));
        };
        if &failure != expected_error {
            return Err(error(
                "E-TRY-ERROR-TYPE",
                span,
                "? requires exactly the enclosing error type; error conversion is explicit",
            ));
        }
        let condition = self.value(InstructionKind::ErrorTag { value }, Type::Bool, span)?;
        let failed = self.block_id()?;
        let succeeded = self.block_id()?;
        self.terminate(Terminator::Branch {
            condition,
            then_block: failed,
            else_block: succeeded,
        });
        self.current = Some(failed);
        let payload = self.value(
            if consuming {
                InstructionKind::TakeErrorUnion {
                    value,
                    is_error: true,
                }
            } else {
                InstructionKind::ReadErrorUnion {
                    value,
                    is_error: true,
                }
            },
            *failure,
            span,
        )?;
        let propagated = self.value(
            InstructionKind::MakeErrorUnion {
                value: payload,
                is_error: true,
            },
            enclosing,
            span,
        )?;
        // end_scopes only reads the lexical/local/temporary lists. Their live
        // success-path state remains intact while failure emits its own drops.
        self.end_unsafe_regions(0, span)?;
        self.end_scopes_kind(0, CleanupExitKind::Error, span)?;
        self.terminate(Terminator::Return(Some(propagated)));
        self.current = Some(succeeded);
        let value = self.value(
            if consuming {
                InstructionKind::TakeErrorUnion {
                    value,
                    is_error: false,
                }
            } else {
                InstructionKind::ReadErrorUnion {
                    value,
                    is_error: false,
                }
            },
            *result.clone(),
            span,
        )?;
        Ok((value, *result))
    }
}

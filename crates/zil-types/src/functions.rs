//! Ordinary function values preserve the exact, environment-free signature.
use super::*;

impl Signature {
    pub(super) fn pointer_type(&self) -> Option<Type> {
        if self.type_parameter.is_some()
            || !self.callable.is_empty()
            || self.foreign
            || self.cleanup
            || !self.parameters.iter().all(Type::is_copy_value)
            || !self.result.is_copy_value()
        {
            return None;
        }
        Some(Type::FunctionPointer {
            parameters: self.parameters.clone(),
            result: Box::new(self.result.clone()),
        })
    }
}

impl Builder<'_, '_> {
    pub(super) fn function_reference(&mut self, expression: &CoreExpr) -> R<(ValueId, Type)> {
        let signature = self.resolve_callee(expression)?;
        if signature.cleanup {
            return Err(error(
                "E-DROP-CALL",
                expression.span,
                "resource teardown cannot become a function value",
            ));
        }
        if signature.foreign {
            return Err(error(
                "E-FFI-FUNCTION-POINTER",
                expression.span,
                "foreign function values require an explicit unsafe C ABI pointer type, which is not implemented",
            ));
        }
        let ty = signature.pointer_type().ok_or_else(|| {
            unsupported(expression.span,
                "function pointer parameters and result require pure Copy values; indirect address contracts are not implemented")
        })?;
        let instruction = InstructionKind::FunctionRef {
            function: signature.concrete_id(expression.span)?,
        };
        let value = self.value(instruction, ty.clone(), expression.span)?;
        Ok((value, ty))
    }

    pub(super) fn call_indirect(
        &mut self,
        callee: ValueId,
        parameters: &[Type],
        result: Type,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        if let Some(expected) = expected {
            self.same(&result, expected, span)?;
        }
        if arguments.len() != parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                format!(
                    "{} arguments required, {} supplied",
                    parameters.len(),
                    arguments.len()
                ),
            ));
        }
        let mut values = Vec::with_capacity(arguments.len());
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let (value, actual) = self.expr(argument, Some(parameter))?;
            self.same(&actual, parameter, argument.span)?;
            values.push(value);
        }
        let instruction = InstructionKind::CallIndirect {
            callee,
            arguments: values,
        };
        if result == Type::Unit {
            self.emit(instruction, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            let value = self.value(instruction, result.clone(), span)?;
            Ok((value, result))
        }
    }
}

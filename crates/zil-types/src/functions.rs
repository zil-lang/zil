//! Ordinary function values preserve the exact, environment-free signature.
use super::*;

impl Signature {
    pub(super) fn pointer_type(&self) -> Option<Type> {
        if self.id.is_none()
            || self.type_parameter.is_some()
            || !self.callable.is_empty()
            || self.foreign
            || self.cleanup
        {
            return None;
        }
        if let Some(definition) = &self.storage_life {
            // A static, environment-free definition is checked independently
            // under rigid incoming variables. Its code value abstracts those
            // variables; it never captures a declaration's Fixed namespace.
            if self.receiver.is_some()
                || self.state_parameter.is_some()
                || !self.result.is_copy_value()
            {
                return None;
            }
            let life =
                borrowed_fp::static_pointer_life(&self.parameters, &self.result, definition)?;
            return Some(Type::BorrowedFunctionPointer {
                parameters: self.parameters.clone(),
                result: Box::new(self.result.clone()),
                life,
            });
        }
        if self
            .parameters
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. }))
            || matches!(self.result, Type::Address { .. })
        {
            let life = self
                .life
                .clone()
                .unwrap_or_else(|| borrowed_fp::default_life(&self.parameters, &self.result));
            borrowed_fp::relation(&self.parameters, &self.result, &life, Span::default()).ok()?;
            return Some(Type::BorrowedFunctionPointer {
                parameters: self.parameters.clone(),
                result: Box::new(self.result.clone()),
                life,
            });
        }
        if !self.parameters.iter().all(Type::is_copy_value) || !self.result.is_copy_value() {
            return None;
        }
        Some(Type::FunctionPointer {
            parameters: self.parameters.clone(),
            result: Box::new(self.result.clone()),
        })
    }
}

impl Builder<'_, '_> {
    pub(super) fn function_reference(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        if let Some(Type::BorrowedFunctionPointer { life, .. }) = expected {
            self.closures
                .snapshot(borrowed_fp::metadata_work(life), expression.span)?;
        }
        let signature = self.resolve_callee(expression)?;
<<<<<<< HEAD
=======
        self.closures
            .snapshot(borrowed_fp::pointer_work(signature), expression.span)?;
        if let Some(expected @ Type::BorrowedFunctionPointer { life, .. }) = expected
            && !life.classes.is_empty()
        {
            self.closures
                .snapshot_type(expected, 2, 0, expression.span)?;
            for ty in signature
                .parameters
                .iter()
                .chain(std::iter::once(&signature.result))
            {
                self.closures.snapshot_type(ty, 2, 0, expression.span)?;
            }
        }
        if signature.storage_life.is_some() {
            for ty in signature
                .parameters
                .iter()
                .chain(std::iter::once(&signature.result))
            {
                self.closures.snapshot_type(ty, 3, 0, expression.span)?;
            }
        }
>>>>>>> 034fbc80d (Infer static qualified function values from complete fresh invocation relations)
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
        let pointer = borrowed_fp::function_reference(signature, expected, expression.span)?;
        let ty = pointer.or_else(|| signature.pointer_type()).ok_or_else(|| {
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
            let (value, actual) = self.argument_expr(argument, parameter)?;
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

//! Finite static C values and synchronous borrowed POD. Verify is independent.
use super::*;

pub(super) fn callback_scalar(ty: &Type) -> bool {
    matches!(ty, Type::I32 | Type::I64 | Type::ForeignHandle)
}

pub(super) fn callback_signature(parameters: &[Type], result: &Type) -> bool {
    parameters.iter().all(callback_scalar) && (result == &Type::Unit || callback_scalar(result))
}

pub(super) fn callback_constructor(expression: &CoreExpr) -> bool {
    matches!(&expression.kind, CoreExprKind::Name(name) if name == "c_callback")
}

pub(super) fn check_literal(constant: &Constant, span: Span) -> R<()> {
    if let Constant::WideString(units) = constant {
        let Some((&0, content)) = units.split_last() else {
            return Err(error(
                "E-FFI-WIDE-STRING",
                span,
                "static UTF-16 requires its trailing NUL",
            ));
        };
        if content.contains(&0) || char::decode_utf16(content.iter().copied()).any(|ch| ch.is_err())
        {
            return Err(error(
                "E-FFI-WIDE-STRING",
                span,
                "static UTF-16 is invalid or contains an embedded NUL",
            ));
        }
    }
    Ok(())
}

impl Signature {
    pub(super) fn foreign_callback_type(&self) -> Option<Type> {
        if self.foreign
            || self.cleanup
            || !self.callable.is_empty()
            || self.callable_return.is_some()
            || self.template.is_some()
            || self.receiver.is_some()
            || !callback_signature(&self.parameters, &self.result)
        {
            return None;
        }
        Some(Type::ForeignCallback {
            parameters: self.parameters.clone(),
            result: Box::new(self.result.clone()),
        })
    }
}

pub(super) fn check_declaration(
    function: &CoreFunction,
    exported: bool,
    remaining: &mut usize,
) -> R<()> {
    let Some(foreign) = &function.foreign else {
        return Ok(());
    };
    let fail = |message| error("E-FFI-DECLARATION", function.span, message);
    let mut spend = |amount: usize| -> R<()> {
        *remaining = remaining.checked_sub(amount).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "foreign declaration checking budget exceeded",
                Some(function.span),
            )
        })?;
        Ok(())
    };
    spend(
        foreign
            .abi
            .len()
            .saturating_add(foreign.symbol.len())
            .saturating_add(function.parameters.len())
            .saturating_add(foreign.noescape_parameters.len().saturating_mul(2))
            .saturating_add(1),
    )?;
    if foreign.abi != "C" {
        return Err(fail("only the explicit C ABI is implemented"));
    }
    let mut bytes = foreign.symbol.bytes();
    if !bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || foreign.symbol == "main"
        || foreign.symbol != function.name
    {
        return Err(fail(
            "foreign symbol must be its source C identifier and cannot be main",
        ));
    }
    if exported || function.receiver.is_some() || !function.body.statements.is_empty() {
        return Err(fail(
            "foreign imports are private bodyless declarations without receivers",
        ));
    }
    if !matches!(
        function.return_type,
        Type::Unit | Type::U8 | Type::U16 | Type::I32 | Type::I64 | Type::ForeignHandle
    ) {
        return Err(fail(
            "C ABI results require unit/u8/u16/i32/i64/c_handle; borrowed or static pointer results are forbidden",
        ));
    }
    let mut noescape = Vec::new();
    for (index, parameter) in function.parameters.iter().enumerate() {
        if !(matches!(parameter.mode, PassingMode::Copy | PassingMode::Infer)
            || matches!(
                (&parameter.ty, parameter.mode),
                (Type::Address { mutable: false, .. }, PassingMode::Borrow)
                    | (Type::Address { mutable: true, .. }, PassingMode::BorrowMut)
            ))
        {
            return Err(fail(
                "C parameters do not accept source resource passing modes",
            ));
        }
        match &parameter.ty {
            Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::ForeignHandle
            | Type::CWideString => {}
            Type::ForeignCallback { parameters, result }
                if foreign.static_callbacks && callback_signature(parameters, result) =>
            {
                spend(parameters.len().saturating_add(1))?;
            }
            Type::Address { inner, mutable } => {
                let mut pending = vec![inner.as_ref()];
                while let Some(target) = pending.pop() {
                    spend(1)?;
                    match target {
                        Type::U8 | Type::U16 | Type::I32 | Type::I64 | Type::ForeignHandle => {}
                        Type::CWideString if !mutable => {}
                        Type::ForeignCallback { parameters, result }
                            if !mutable
                                && foreign.static_callbacks
                                && callback_signature(parameters, result) =>
                        {
                            spend(parameters.len().saturating_add(1))?;
                        }
                        Type::Array { element, length } if *length > 0 => pending.push(element),
                        Type::Record { name, fields } if !fields.is_empty() => {
                            spend(name.len().saturating_add(fields.len()))?;
                            for (name, ty) in fields {
                                spend(name.len())?;
                                pending.push(ty);
                            }
                        }
                        _ => {
                            return Err(fail(
                                "C borrowed targets require the strict nonempty natural-layout POD profile; mutable payloads cannot contain static strings or callbacks",
                            ));
                        }
                    }
                }
                noescape
                    .push(u32::try_from(index).map_err(|_| fail("C parameter index overflow"))?);
            }
            _ => {
                return Err(fail(
                    "C value parameters require integer/handle/static-string values or a static callback with static_callbacks enabled",
                ));
            }
        }
    }
    if noescape != foreign.noescape_parameters {
        return Err(fail(
            "noescape indices must exactly cover address parameters in canonical order",
        ));
    }
    Ok(())
}

impl Builder<'_, '_> {
    pub(super) fn foreign_callback(
        &mut self,
        arguments: &[CoreExpr],
        span: Span,
    ) -> R<(ValueId, Type)> {
        let [target] = arguments else {
            return Err(error(
                "E-FFI-CALLBACK",
                span,
                "c_callback requires exactly one static function symbol",
            ));
        };
        let signature = self.resolve_callee(target).map_err(|_| {
            error(
                "E-FFI-CALLBACK",
                target.span,
                "c_callback requires a static ordinary function symbol",
            )
        })?;
        let ty = signature.foreign_callback_type().ok_or_else(|| {
            error("E-FFI-CALLBACK", target.span, "C callbacks require an ordinary environment-free function with exact i32/i64/handle signature")
        })?;
        let function = signature.id;
        let value = self.value(
            InstructionKind::ForeignFunctionRef { function },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }

    pub(super) fn direct_call(
        &self,
        signature: &Signature,
        arguments: Vec<ValueId>,
        span: Span,
    ) -> R<InstructionKind> {
        if signature.cleanup {
            return Err(error(
                "E-DROP-CALL",
                span,
                "resource teardown requires a cleanup activation",
            ));
        }
        if signature.foreign {
            let region = self.unsafe_regions.last().copied().ok_or_else(|| {
                error(
                    "E-FFI-UNSAFE",
                    span,
                    "foreign calls require an explicit lexical unsafe block",
                )
            })?;
            Ok(InstructionKind::ForeignCall {
                function: signature.id,
                arguments,
                region,
            })
        } else {
            Ok(InstructionKind::Call {
                function: signature.id,
                arguments,
            })
        }
    }

    pub(super) fn unsafe_block(&mut self, body: &CoreBlock) -> R<()> {
        let region = self.next_unsafe_region;
        self.next_unsafe_region = region.checked_add(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "unsafe region identity overflow",
                Some(body.span),
            )
        })?;
        self.emit(InstructionKind::UnsafeEnter { region }, None, body.span)?;
        self.unsafe_regions.push(region);
        self.block(body)?;
        if self.current.is_some() {
            self.emit(InstructionKind::UnsafeExit { region }, None, body.span)?;
        }
        self.unsafe_regions.pop();
        Ok(())
    }

    /// Emit exits for this control-flow edge; retain lexical state for siblings.
    pub(super) fn end_unsafe_regions(&mut self, depth: usize, span: Span) -> R<()> {
        for region in self.unsafe_regions[depth..]
            .iter()
            .rev()
            .copied()
            .collect::<Vec<_>>()
        {
            self.emit(InstructionKind::UnsafeExit { region }, None, span)?;
        }
        Ok(())
    }
}

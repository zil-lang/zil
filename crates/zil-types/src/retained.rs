//! Resolve one self-contained concrete environment behind a public opaque
//! callable return. This module does not instantiate or inline factory bodies
//! at call sites; direct calls retain their ordinary function identities.
use super::*;

pub(super) fn validate_factory(function: &CoreFunction) -> R<()> {
    let Some(bound) = &function.callable_return else {
        return Ok(());
    };
    let signature = Type::FunctionPointer {
        parameters: bound.parameters.clone(),
        result: Box::new(bound.result.clone()),
    };
    if function.foreign.is_some()
        || function.receiver.is_some()
        || function.callable_parameter.is_some()
        || function.result_mode != PassingMode::Infer
        || bound.kind == CallableKind::FunctionPointer
        || !bound.parameters.iter().all(Type::is_copy_value)
        || !bound.result.is_copy_value()
        || function.return_type
            != (Type::Opaque {
                identity: function.id,
                hidden: Box::new(signature),
            })
        || function.parameters.iter().any(|parameter| {
            !(parameter.ty.is_copy_value()
                || matches!(&parameter.ty, Type::Managed(_) if resources::supported(&parameter.ty))
                || packages::supported(&parameter.ty))
        })
    {
        return Err(unsupported(
            function.span,
            "opaque callable factories require ordinary Copy/owned Managed/Package inputs, one finite Copy signature and a matching opaque identity",
        ));
    }
    Ok(())
}

impl Builder<'_, '_> {
    pub(super) fn factory_return(&mut self, expression: Option<&CoreExpr>, span: Span) -> R<()> {
        let expression = expression.ok_or_else(|| {
            error(
                "E-TYPE-MISMATCH",
                span,
                "callable factory must return its concrete environment",
            )
        })?;
        let (value, ty) = self.transfer_expr(expression, None)?;
        let Type::Closure(id) = ty else {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "opaque callable return requires a closure environment",
            ));
        };
        if (id.0 as usize) < self.first_closure {
            return Err(unsupported(
                span,
                "forwarded or recursive callable factory returns are not implemented",
            ));
        }
        let bound = self.factory_bound.as_ref().expect("factory return").clone();
        let pending = &mut self.closures.pending[id.0 as usize];
        if pending.inference.arity() != bound.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "returned closure differs from the public callable signature",
            ));
        }
        for (index, ty) in bound.parameters.iter().enumerate() {
            pending.inference.argument(index, ty, span)?;
        }
        pending.inference.expect_result(&bound.result, span)?;
        if pending.callable_kind() != bound.kind {
            return Err(unsupported(
                span,
                "opaque callable kind adaptation is not implemented; declared and actual Fn/FnMut/FnOnce kinds must match",
            ));
        }
        let mut cleanup = Vec::new();
        for (index, capture) in pending.captures.iter().enumerate() {
            let ty = capture.stored_type();
            if matches!(&ty, Type::Managed(_) if resources::supported(&ty))
                || packages::supported(&ty)
            {
                cleanup.push(index as u32);
            } else if !ty.is_copy_value() {
                return Err(unsupported(
                    span,
                    "returned closure captures must be self-contained Copy snapshots or owned Managed/Package values; escaping Address/Desc captures are not implemented",
                ));
            }
        }
        if let Some(previous) = &self.function.callable_return_contract
            && previous.closure != id
        {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "all opaque callable returns must have one hidden closure identity",
            ));
        }
        self.function.return_type = Type::Closure(id);
        self.function.callable_return_contract = Some(CallableReturnContract {
            closure: id,
            kind: bound.kind,
            hold: Vec::new(),
            call: Vec::new(),
            writes: Vec::new(),
            cleanup,
        });
        self.materialize_closure(id)?;
        self.return_value(Some(value), span)
    }
}

//! Explicit sealed byte storage. These operations produce untrusted Typed IR.
use super::*;

pub(super) fn resource(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) | Type::Desc(inner) if **inner == Type::ByteBuffer)
}

pub(super) fn primitive(expression: &CoreExpr) -> Option<&str> {
    let CoreExprKind::Name(name) = &expression.kind else {
        return None;
    };
    matches!(
        name.as_str(),
        "bytes_new"
            | "bytes_reserve"
            | "bytes_truncate"
            | "bytes_push"
            | "bytes_len"
            | "bytes_capacity"
            | "bytes_view"
            | "bytes_view_mut"
    )
    .then_some(name)
}

pub(super) fn result_type(name: &str) -> Option<Type> {
    Some(match name {
        "bytes_new" => Type::Managed(Box::new(Type::ByteBuffer)),
        "bytes_reserve" | "bytes_truncate" | "bytes_push" => Type::Bool,
        "bytes_len" | "bytes_capacity" => Type::I64,
        "bytes_view" | "bytes_view_mut" => Type::Address {
            mutable: name == "bytes_view_mut",
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U8),
            }),
        },
        _ => return None,
    })
}

pub(super) fn parameters(name: &str) -> Vec<Type> {
    if name == "bytes_new" {
        return vec![];
    }
    let mut result = vec![Type::Desc(Box::new(Type::ByteBuffer))];
    match name {
        "bytes_reserve" | "bytes_truncate" => result.push(Type::I64),
        "bytes_push" => result.push(Type::U8),
        _ => {}
    }
    result
}

pub(super) fn add_definition(
    functions: &[TypedFunction],
    definitions: &mut Vec<ResourceDefinition>,
    limits: &Limits,
) -> R<()> {
    let mut work = limits.max_analysis_steps;
    let limit = || {
        Diagnostic::limit(
            Phase::Types,
            "byte storage definition scan exceeded budget",
            None,
        )
    };
    for function in functions {
        // Reserve roots first and child visits before growing the worklist.
        // An error result can be the only mention of its Package's byte owner.
        let amount = function
            .values
            .len()
            .checked_add(function.locals.len())
            .and_then(|n| n.checked_add(2))
            .ok_or_else(limit)?;
        work = work.checked_sub(amount).ok_or_else(limit)?;
        let mut found = false;
        for ty in function
            .values
            .iter()
            .chain(function.locals.iter().map(|l| &l.ty))
            .chain(std::iter::once(&function.return_type))
        {
            let mut pending = vec![ty];
            while let Some(ty) = pending.pop() {
                match ty {
                    Type::Package { fields, .. } => {
                        work = work.checked_sub(fields.len()).ok_or_else(limit)?;
                        pending.extend(fields.iter().map(|(_, ty)| ty));
                    }
                    Type::OwnedStore { element }
                    | Type::Array { element, .. }
                    | Type::Opaque {
                        hidden: element, ..
                    } => {
                        work = work.checked_sub(1).ok_or_else(limit)?;
                        pending.push(element);
                    }
                    Type::Managed(payload) | Type::Desc(payload)
                        if matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
                    {
                        work = work.checked_sub(1).ok_or_else(limit)?;
                        pending.push(payload);
                    }
                    Type::ErrorUnion { error, result } => {
                        work = work.checked_sub(2).ok_or_else(limit)?;
                        pending.push(error);
                        pending.push(result);
                    }
                    _ => found |= resource(ty),
                }
            }
        }
        if !found {
            continue;
        }
        work.checked_sub(BYTE_BUFFER_RESOURCE_NAME.len() + 2)
            .ok_or_else(limit)?;
        definitions.push(ResourceDefinition {
            name: BYTE_BUFFER_RESOURCE_NAME.into(),
            payload: Type::ByteBuffer,
            teardown: None,
        });
        break;
    }
    Ok(())
}

impl Builder<'_, '_> {
    pub(super) fn byte_buffer_call(
        &mut self,
        name: &str,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let arity = match name {
            "bytes_new" => 0,
            "bytes_reserve" | "bytes_truncate" | "bytes_push" => 2,
            _ => 1,
        };
        if arguments.len() != arity {
            return Err(error(
                "E-CALL-ARITY",
                span,
                format!("{name} requires {arity} arguments"),
            ));
        }
        let ty = result_type(name).expect("resolved byte primitive");
        let instruction = if name == "bytes_new" {
            InstructionKind::ByteBufferNew
        } else {
            // Evaluate the actual descriptor before later arguments; its use is
            // checked again at the operation after any RHS effects.
            let desc = Type::Desc(Box::new(Type::ByteBuffer));
            let (buffer, actual) = self.argument_expr(&arguments[0], &desc)?;
            self.same(&actual, &desc, arguments[0].span)?;
            match name {
                "bytes_reserve" => {
                    let (capacity, actual) = self.expr(&arguments[1], Some(&Type::I64))?;
                    self.same(&actual, &Type::I64, arguments[1].span)?;
                    InstructionKind::ByteBufferReserve { buffer, capacity }
                }
                "bytes_truncate" => {
                    let (length, actual) = self.expr(&arguments[1], Some(&Type::I64))?;
                    self.same(&actual, &Type::I64, arguments[1].span)?;
                    InstructionKind::ByteBufferTruncate { buffer, length }
                }
                "bytes_push" => {
                    let (value, actual) = self.expr(&arguments[1], Some(&Type::U8))?;
                    self.same(&actual, &Type::U8, arguments[1].span)?;
                    InstructionKind::ByteBufferPush { buffer, value }
                }
                "bytes_len" => InstructionKind::ByteBufferLen { buffer },
                "bytes_capacity" => InstructionKind::ByteBufferCapacity { buffer },
                "bytes_view" | "bytes_view_mut" => InstructionKind::ByteBufferBorrow {
                    buffer,
                    mutable: name == "bytes_view_mut",
                },
                _ => unreachable!(),
            }
        };
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        let value = self.value(instruction, ty.clone(), span)?;
        Ok((value, ty))
    }
}

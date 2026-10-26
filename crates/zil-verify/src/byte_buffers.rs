//! Independent admission of the sealed, explicitly owning byte storage.
//! A raw payload tag or an external token cannot establish an allocation head.
use super::*;

pub(super) const NAME: &str = "@builtin:ByteBuffer";

pub(super) fn is_desc(ty: &Type) -> bool {
    matches!(ty, Type::Desc(inner) if **inner == Type::ByteBuffer)
}

pub(super) fn is_owned(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) if **inner == Type::ByteBuffer)
}

pub(super) fn is_carrier(ty: &Type) -> bool {
    is_desc(ty) || is_owned(ty)
}

pub(super) fn target(target: &TargetProfile, span: Option<Span>) -> Check<()> {
    if !target.is_supported_x64_llvm() {
        return Err(unsupported(
            "byte storage requires the checked little-endian x64 LLVM Linux or Win64 profile",
            span,
        ));
    }
    Ok(())
}

pub(super) fn check_carrier(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    target(&module.target, span)?;
    budget.spend(module.resources.len())?;
    if !is_carrier(ty)
        || !module.resources.iter().any(|resource| {
            resource.name == NAME
                && resource.payload == Type::ByteBuffer
                && resource.teardown.is_none()
        })
    {
        return Err(reject(
            "E-VERIFY-BYTE-BUFFER",
            span,
            "byte storage requires its canonical sealed resource definition",
        ));
    }
    Ok(())
}

pub(super) fn check_instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    target(&module.target, span)?;
    let bad = |message| reject("E-VERIFY-BYTE-BUFFER", span, message);
    if matches!(instruction.kind, InstructionKind::ByteBufferNew) {
        let ty = Type::Managed(Box::new(Type::ByteBuffer));
        check_carrier(module, &ty, span, budget)?;
        return Ok(Some(ty));
    }
    let (buffer, extra) = match &instruction.kind {
        InstructionKind::ByteBufferReserve { buffer, capacity } => {
            (*buffer, Some((*capacity, Type::I64)))
        }
        InstructionKind::ByteBufferPush { buffer, value } => (*buffer, Some((*value, Type::U8))),
        InstructionKind::ByteBufferLen { buffer }
        | InstructionKind::ByteBufferCapacity { buffer }
        | InstructionKind::ByteBufferBorrow { buffer, .. } => (*buffer, None),
        _ => unreachable!("only byte buffer operations enter this checker"),
    };
    if !is_desc(value_type(function, buffer, span)?) {
        return Err(bad(
            "byte storage operation requires a real ByteBuffer description",
        ));
    }
    if let Some((operand, expected)) = extra
        && value_type(function, operand, span)? != &expected
    {
        return Err(bad(
            "byte storage operation has an invalid capacity or element operand",
        ));
    }
    Ok(Some(match instruction.kind {
        InstructionKind::ByteBufferReserve { .. } | InstructionKind::ByteBufferPush { .. } => {
            Type::Bool
        }
        InstructionKind::ByteBufferLen { .. } | InstructionKind::ByteBufferCapacity { .. } => {
            Type::I64
        }
        InstructionKind::ByteBufferBorrow { mutable, .. } => Type::Address {
            mutable,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U8),
            }),
        },
        _ => unreachable!("checked byte storage operation"),
    }))
}

/// The operation really reads or writes the backing even when its runtime
/// result is failure or the requested reserve is already satisfied.
pub(super) fn access(kind: &InstructionKind) -> Option<(ValueId, bool)> {
    match *kind {
        InstructionKind::ByteBufferReserve { buffer, .. }
        | InstructionKind::ByteBufferPush { buffer, .. } => Some((buffer, true)),
        InstructionKind::ByteBufferBorrow { buffer, mutable } => Some((buffer, mutable)),
        InstructionKind::ByteBufferLen { buffer }
        | InstructionKind::ByteBufferCapacity { buffer } => Some((buffer, false)),
        _ => None,
    }
}

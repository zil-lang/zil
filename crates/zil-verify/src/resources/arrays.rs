//! Untrusted owning-array types and operations are admitted independently here.
//! Slot responsibility and provenance are reconstructed by the resource checker.
use super::*;

pub(crate) fn is_owned_type(ty: &Type) -> bool {
    matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. }))
}

pub(crate) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    let Type::Array { element, length } = ty else {
        return Err(reject(
            "E-VERIFY-OWNED-ARRAY",
            span,
            "operation requires an owning array",
        ));
    };
    let Type::Package { fields, .. } = element.as_ref() else {
        return Err(unsupported(
            "owning arrays currently require finite Package elements",
            span,
        ));
    };
    if *length == 0 {
        return Err(unsupported(
            "owning arrays require a nonzero fixed length",
            span,
        ));
    }
    budget.spend((*length as usize).saturating_mul(fields.len().saturating_add(1)))?;
    packages::check_type_for(module, element, span, budget)
}

pub(crate) fn check_instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    let bad = |message| reject("E-VERIFY-OWNED-ARRAY", span, message);
    if let InstructionKind::MakeOwnedArray { elements } = &instruction.kind {
        let result = instruction
            .result
            .ok_or_else(|| bad("owning array construction needs a result"))?;
        let ty = value_type(function, result, span)?;
        check_type_for(module, ty, span, budget)?;
        let Type::Array { element, length } = ty else {
            unreachable!()
        };
        if elements.len() != *length as usize {
            return Err(bad(
                "owning array construction must provide every element exactly once",
            ));
        }
        budget.spend(elements.len())?;
        for value in elements {
            charge_payload(element, budget)?;
            if value_type(function, *value, span)? != element.as_ref() {
                return Err(bad("owning array construction element type differs"));
            }
        }
        return Ok(Some(ty.clone()));
    }
    let (local, index) = match instruction.kind {
        InstructionKind::ProjectOwnedArrayField { local, index, .. }
        | InstructionKind::TakeOwnedArrayElement { local, index }
        | InstructionKind::DropOwnedArrayElement { local, index }
        | InstructionKind::StoreOwnedArrayElement { local, index, .. }
        | InstructionKind::BorrowOwnedArrayElement { local, index, .. } => (local, Some(index)),
        InstructionKind::DropOwnedArray { local } => (local, None),
        _ => unreachable!("only owning array operations enter this checker"),
    };
    let ty = local_type(function, local, span)?;
    check_type_for(module, ty, span, budget)?;
    let Type::Array { element, length } = ty else {
        unreachable!()
    };
    if index.is_some_and(|index| index >= *length) {
        return Err(bad(
            "owning array element index is outside the declared fixed length",
        ));
    }
    match &instruction.kind {
        InstructionKind::ProjectOwnedArrayField { field, .. } => {
            let Type::Package { fields, .. } = element.as_ref() else {
                unreachable!()
            };
            let (_, ty) = fields.get(*field as usize).ok_or_else(|| {
                bad("owning array field index is outside its Package declaration")
            })?;
            charge_payload(ty, budget)?;
            Ok(Some(match ty {
                Type::Managed(payload) => Type::Desc(payload.clone()),
                ty => {
                    check_type(ty, span)?;
                    ty.clone()
                }
            }))
        }
        InstructionKind::TakeOwnedArrayElement { .. } => Ok(Some(element.as_ref().clone())),
        InstructionKind::StoreOwnedArrayElement { value, .. } => {
            if !function.locals[local.0 as usize].mutable {
                return Err(reject(
                    "E-VERIFY-MUTABILITY",
                    span,
                    "owning array refill requires mutable storage",
                ));
            }
            charge_payload(element, budget)?;
            if value_type(function, *value, span)? != element.as_ref() {
                return Err(bad(
                    "owning array refill input type differs from its element type",
                ));
            }
            Ok(None)
        }
        InstructionKind::DropOwnedArrayElement { .. } | InstructionKind::DropOwnedArray { .. } => {
            Ok(None)
        }
        InstructionKind::BorrowOwnedArrayElement { .. } => Err(unsupported(
            "owning array environment borrowing requires the concrete closure array slice",
            span,
        )),
        _ => unreachable!("owning array local operation"),
    }
}

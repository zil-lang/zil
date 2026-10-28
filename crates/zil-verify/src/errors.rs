//! Independent admission and tag evidence for finite owning error unions.
//! A selected payload is not a second responsibility: Take consumes its union.
use super::*;

pub(super) fn owned(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if matches!(error.as_ref(), Type::Package { .. })
            || matches!(result.as_ref(), Type::Package { .. } | Type::Managed(_)))
}

/// Classifies a raw shape; admission and responsibility are checked elsewhere.
pub(super) fn managed_success(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::Managed(_)))
}

pub(super) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    let Type::ErrorUnion { error, result } = ty else {
        return Err(reject(
            "E-VERIFY-ERROR",
            span,
            "expected an error-union type",
        ));
    };
    budget.spend(2)?;
    if matches!(result.as_ref(), Type::Managed(_)) {
        // Only the success arm is opened, with a finite Copy error. Check the
        // nominal/sealed resource table independently of the source resolver.
        resources::charge_payload(error, budget)?;
        check_type(error, span)?;
        return resources::check_type_for(module, result, span, budget);
    }
    for payload in [error.as_ref(), result.as_ref()] {
        if matches!(payload, Type::Package { .. }) {
            packages::check_type_for(module, payload, span, budget)?;
        } else {
            // The Copy checker rejects nested owners, addresses, descriptions,
            // opaque environments and forged resource carriers independently.
            resources::charge_payload(payload, budget)?;
            check_type(payload, span)?;
        }
    }
    Ok(())
}

pub(super) fn check_instruction(
    function: &TypedFunction,
    instruction: &Instruction,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    match instruction.kind {
        InstructionKind::TakeErrorUnion { value, is_error } => {
            let ty = value_type(function, value, span)?;
            let Type::ErrorUnion { error, result } = ty else {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "TakeErrorUnion requires an error union",
                ));
            };
            if !owned(ty) {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "TakeErrorUnion requires a non-Copy union",
                ));
            }
            Ok(Some(if is_error {
                (**error).clone()
            } else {
                (**result).clone()
            }))
        }
        InstructionKind::DropErrorUnion { local } => {
            if !owned(local_type(function, local, span)?) {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "DropErrorUnion requires an owned union local",
                ));
            }
            Ok(None)
        }
        _ => unreachable!("error instruction dispatch"),
    }
}

/// The first owned profile admits a statically constructed tag or the direct
/// selected edge of ErrorTag. A unique predecessor and a non-entry block make
/// that edge unavoidable; merely dominating with a Boolean test is not enough
/// if its opposite edge can also enter the payload block.
pub(super) fn check_take_tag(
    function: &TypedFunction,
    block: usize,
    instruction: &Instruction,
    predecessors: &[Vec<usize>],
    definitions: &[Option<(usize, usize)>],
    budget: &mut Budget,
) -> Check<()> {
    let InstructionKind::TakeErrorUnion { value, is_error } = instruction.kind else {
        return Ok(());
    };
    let fail = || {
        reject(
            "E-VERIFY-ERROR-TAG",
            Some(instruction.span),
            "owning payload extraction requires its exact active tag on every incoming path",
        )
    };
    budget.spend(4)?;
    let (source_block, source_index) =
        definitions[value.0 as usize].expect("checked SSA definition");
    if let InstructionKind::MakeErrorUnion {
        is_error: actual, ..
    } = function.blocks[source_block].instructions[source_index].kind
    {
        return if actual == is_error {
            Ok(())
        } else {
            Err(fail())
        };
    }
    if block == function.entry.0 as usize || predecessors[block].len() != 1 {
        return Err(fail());
    }
    let predecessor = predecessors[block][0];
    let Terminator::Branch {
        condition,
        then_block,
        else_block,
    } = function.blocks[predecessor].terminator
    else {
        return Err(fail());
    };
    if then_block == else_block
        || (if is_error { then_block } else { else_block }).0 as usize != block
    {
        return Err(fail());
    }
    let (test_block, test_index) =
        definitions[condition.0 as usize].expect("checked SSA definition");
    if !matches!(function.blocks[test_block].instructions[test_index].kind,
        InstructionKind::ErrorTag { value: tested } if tested == value)
    {
        return Err(fail());
    }
    Ok(())
}

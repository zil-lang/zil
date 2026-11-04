//! Untrusted owning-array types and operations are admitted independently here.
//! Slot responsibility and provenance are reconstructed by the resource checker.
use super::*;

pub(crate) fn is_owned_type(ty: &Type) -> bool {
    matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_)))
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
    if *length == 0 {
        return Err(unsupported(
            "owning arrays require a nonzero fixed length",
            span,
        ));
    }
    match element.as_ref() {
        Type::Package { .. } => {
            if packages::has_callable_fields(element) {
                return Err(unsupported(
                    "callback State arrays require a separate nested-field protocol",
                    span,
                ));
            }
            let element_work = if packages::has_array_fields(element) {
                packages::layout_work(element, budget)?
            } else {
                closures::type_layout_work(element, budget)?
            };
            let work = element_work
                .checked_mul(*length as usize)
                .ok_or_else(|| limit("owning array layout expansion overflow"))?;
            budget.spend(work)?;
            packages::check_type_for(module, element, span, budget)
        }
        Type::Closure(id) => {
            closures::definition(module, *id, span)?;
            closures::old_container_signature(module, *id, span, budget)?;
            budget.spend(*length as usize)?;
            // The global independent closure-layout pass also charges the
            // length multiplied by fully expanded inline child work.
            closures::self_contained(module, *id, budget)
        }
        _ => Err(unsupported(
            "owning arrays require finite Package or self-contained Closure elements",
            span,
        )),
    }
}

/// The existing lowering for a mutable owning closure local uses one private
/// array cell. Preserve the old public/container gate, then admit only this
/// grounded local carrier when its actual callback signature is the closed
/// owning-data profile. Function parameters and results still call the old
/// `check_type_for` directly.
pub(crate) fn check_internal_single_slot_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<bool> {
    let rejected = match check_type_for(module, ty, span, budget) {
        Ok(()) => return Ok(false),
        Err(rejected) => rejected,
    };
    if rejected.category != zil_base::ResultCategory::Unsupported {
        return Err(rejected);
    }
    let Type::Array { element, length: 1 } = ty else {
        return Err(rejected);
    };
    let Type::Closure(id) = element.as_ref() else {
        return Err(rejected);
    };
    let definition = closures::definition(module, *id, span)?;
    if !owned_callbacks::signature(module, definition, budget)? {
        return Err(rejected);
    }
    budget.spend(1)?;
    closures::self_contained(module, *id, budget)?;
    Ok(true)
}

fn owning_slot(module: &TypedModule, ty: &Type, budget: &mut Budget) -> Check<Option<ClosureId>> {
    let Type::Array { element, length: 1 } = ty else {
        return Ok(None);
    };
    let Type::Closure(id) = element.as_ref() else {
        return Ok(None);
    };
    let entry = closures::definition(module, *id, None)?;
    budget.spend(entry.parameters.len().saturating_add(1))?;
    for ty in entry
        .parameters
        .iter()
        .chain(std::iter::once(&entry.result))
    {
        let work = closures::type_layout_work(ty, budget)?;
        budget.spend(work)?;
    }
    Ok(
        (entry.parameters.iter().any(owned_callbacks::root)
            || owned_callbacks::root(&entry.result))
        .then_some(*id),
    )
}

/// Classify every operation that names a private Array<Closure; 1> local.
/// A new local opcode cannot borrow the shell or put it in an aggregate by
/// accident: it must be added here only after its boundary is independently
/// checked.
fn private_slot_access(kind: &InstructionKind, slot: LocalId) -> Option<bool> {
    match kind {
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::Store { local, .. }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::TakeOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArrayElement { local, .. }
        | InstructionKind::StoreOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::BorrowOwnedArrayElement { local, .. }
            if *local == slot =>
        {
            Some(true)
        }
        InstructionKind::RegisterDefer { local, .. }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::ProjectOwnedArrayField { local, .. }
        | InstructionKind::BorrowPackageField { local, .. }
        | InstructionKind::ReadPackageField { local, .. }
        | InstructionKind::DescribePackageField { local, .. }
        | InstructionKind::DropPackage { local }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
        | InstructionKind::BindPackageCapture { local, .. }
        | InstructionKind::EndPackageCapture { local, .. }
        | InstructionKind::DescribeLocal { local }
        | InstructionKind::DropManaged { local }
        | InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::VariantTagLocal { local, .. }
        | InstructionKind::ReadVariantLocal { local, .. }
        | InstructionKind::BorrowVariantLocal { local, .. }
        | InstructionKind::BorrowFieldLocal { local, .. }
        | InstructionKind::UnpackVariant { local, .. }
        | InstructionKind::DropEnum { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. }
        | InstructionKind::DropClosure { local }
            if *local == slot =>
        {
            Some(false)
        }
        InstructionKind::UnpackVariant { fields, .. } if fields.contains(&slot) => Some(false),
        _ => None,
    }
}

/// The mutable-closure lowering has exactly one private owning cell. This
/// gate classifies its carrier and allowed operations; it never grants source
/// identity from a nearby helper call. Responsibility, element liveness and
/// old borrowed addresses are independently checked by resources and borrows.
/// In a Parameter-return function retained::provenance additionally proves
/// the exact F instance on every Return. Public array, capture, State and
/// Store signatures still use the older callback container gate.
pub(crate) fn check_internal_slot_uses(
    module: &TypedModule,
    f: &TypedFunction,
    _graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(f.locals.len().saturating_add(f.values.len()))?;
    let mut slots = vec![false; f.locals.len()];
    let mut values = vec![false; f.values.len()];
    for local in &f.locals {
        slots[local.id.0 as usize] = owning_slot(module, &local.ty, budget)?.is_some();
    }
    for (index, ty) in f.values.iter().enumerate() {
        values[index] = owning_slot(module, ty, budget)?.is_some();
    }
    if !slots.iter().any(|slot| *slot) && !values.iter().any(|value| *value) {
        return Ok(());
    }
    budget.spend(
        slots
            .len()
            .saturating_mul(2)
            .saturating_add(values.len().saturating_mul(3))
            .saturating_add(f.parameters.len()),
    )?;
    let bad = |message| unsupported(message, Some(f.span));
    if f.foreign.is_some()
        || f.parameters.iter().any(|id| slots[id.0 as usize])
        || owning_slot(module, &f.return_type, budget)?.is_some()
    {
        return Err(bad(
            "owning callback array cannot cross a function boundary",
        ));
    }
    let instructions = f.blocks.iter().try_fold(0usize, |n, block| {
        n.checked_add(block.instructions.len())
            .ok_or_else(|| limit("private callback slot instruction count overflow"))
    })?;
    budget.spend(
        instructions
            .checked_add(f.blocks.len().saturating_mul(2))
            .ok_or_else(|| limit("private callback slot preflight overflow"))?,
    )?;
    let mut stored = vec![false; values.len()];
    for block in &f.blocks {
        if let Terminator::Return(Some(value)) = block.terminator
            && values[value.0 as usize]
        {
            return Err(bad("private callback array cannot be returned"));
        }
        for instruction in &block.instructions {
            // The iterator's upper size hint is O(1) from the already
            // bounded operand vectors. Charge it before traversing values;
            // the extra field term covers UnpackVariant's local-id scan.
            let operand_bound = instruction_operands(&instruction.kind)
                .size_hint()
                .1
                .ok_or_else(|| limit("private callback operand bound unavailable"))?;
            let fields = match &instruction.kind {
                InstructionKind::UnpackVariant { fields, .. } => fields.len(),
                _ => 0,
            };
            budget.spend(
                slots
                    .len()
                    .saturating_mul(fields.saturating_add(1))
                    .saturating_add(operand_bound)
                    .saturating_add(3),
            )?;
            for (index, &slot) in slots.iter().enumerate() {
                if slot
                    && let Some(false) =
                        private_slot_access(&instruction.kind, LocalId(index as u32))
                {
                    return Err(bad(
                        "private callback array uses an unsupported local operation",
                    ));
                }
            }
            if let Some(result) = instruction.result
                && values[result.0 as usize]
                && !matches!(
                    instruction.kind,
                    InstructionKind::MakeOwnedArray { .. } | InstructionKind::TakeLocal { .. }
                )
            {
                return Err(bad(
                    "private callback array has an unsupported SSA producer",
                ));
            }
            for operand in instruction_operands(&instruction.kind) {
                if !values[operand.0 as usize] {
                    continue;
                }
                let InstructionKind::Store { local, value } = instruction.kind else {
                    return Err(bad(
                        "private callback array cannot enter a call, capture or aggregate",
                    ));
                };
                if operand != value || !slots[local.0 as usize] {
                    return Err(bad(
                        "private callback array must be stored in a matching local cell",
                    ));
                }
                stored[value.0 as usize] = true;
            }
        }
    }
    // A declared but uninitialized private local has no callable instance or
    // cleanup debt. It needs no construction or forwarding witness. An owned
    // Array SSA, however, must still transfer into a checked local below.
    if values
        .iter()
        .enumerate()
        .any(|(i, &value)| value && !stored[i])
    {
        return Err(bad(
            "private callback array must have an actual local carrier",
        ));
    }
    Ok(())
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
        check_internal_single_slot_type_for(module, ty, span, budget)?;
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
    check_internal_single_slot_type_for(module, ty, span, budget)?;
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
                return Err(bad("field projection requires an actual Package element"));
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
        InstructionKind::BorrowOwnedArrayElement { mutable, .. } => {
            if !matches!(element.as_ref(), Type::Closure(_)) {
                return Err(bad(
                    "environment borrowing requires a concrete Closure array element",
                ));
            }
            if *mutable && !function.locals[local.0 as usize].mutable {
                return Err(reject(
                    "E-VERIFY-MUTABILITY",
                    span,
                    "exclusive callback array borrow requires mutable storage",
                ));
            }
            Ok(Some(Type::Address {
                mutable: *mutable,
                inner: element.clone(),
            }))
        }
        _ => unreachable!("owning array local operation"),
    }
}

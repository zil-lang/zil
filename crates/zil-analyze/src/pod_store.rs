//! Producer-only POD Store admission. Verify reconstructs its own proof.
use std::collections::BTreeSet;

use zil_ir::{Instruction, InstructionKind, Type, TypedFunction, TypedModule, ValueId};

use super::{Budget, CompileResult, Diagnostic, Phase, bad};

const WORK: &str = "POD Store admission";
const PREFIX: &str = "@builtin:OwnedStore<";

fn unsupported(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Analyze, message, None)]
}

fn bounded(value: Option<u64>) -> CompileResult<u64> {
    value
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Analyze,
                "POD Store layout exceeds the target address range",
                None,
            )]
        })
}

fn align(size: u64, alignment: u64) -> CompileResult<u64> {
    bounded(size.checked_add(alignment - 1)).map(|n| n / alignment * alignment)
}

fn fields<'a>(
    entries: impl IntoIterator<Item = &'a Type>,
    budget: &mut Budget,
) -> CompileResult<(u64, u64)> {
    let (mut size, mut alignment) = (0, 1);
    for field in entries {
        let (bytes, field_alignment) = layout(field, budget)?;
        size = bounded(align(size, field_alignment)?.checked_add(bytes))?;
        alignment = alignment.max(field_alignment);
    }
    Ok((align(size, alignment)?, alignment))
}

// Complete raw shape/depth preflight precedes this recursion. Do not use the
// broader Copy predicate: callable, foreign and Sum leaves are not POD here.
fn layout(ty: &Type, budget: &mut Budget) -> CompileResult<(u64, u64)> {
    budget.spend(1, WORK)?;
    Ok(match ty {
        // Actual aggregate-field Unit representation is the zero-byte struct.
        Type::Unit => (0, 1),
        Type::Bool | Type::U8 => (1, 1),
        Type::U16 => (2, 2),
        Type::I32 | Type::F32 => (4, 4),
        Type::I64 | Type::U64 | Type::Usize | Type::F64 => (8, 8),
        Type::Record {
            name,
            fields: entries,
        } => {
            budget.spend(
                name.len().saturating_add(entries.len()).saturating_add(1),
                WORK,
            )?;
            if name.is_empty() {
                return Err(bad("POD Store record requires a nonempty nominal identity"));
            }
            let mut names = BTreeSet::new();
            for (name, _) in entries {
                budget.product(
                    name.len().saturating_add(1),
                    entries.len().saturating_add(1),
                    1,
                    WORK,
                )?;
                if name.is_empty() || !names.insert(name) {
                    return Err(bad("POD Store fields require unique nonempty identities"));
                }
            }
            fields(entries.iter().map(|(_, ty)| ty), budget)?
        }
        Type::Tuple(entries) => {
            budget.spend(entries.len(), WORK)?;
            fields(entries, budget)?
        }
        Type::Array { element, length } => {
            // Even an empty array must qualify its actual leaf.
            let (size, alignment) = layout(element, budget)?;
            (bounded(size.checked_mul(u64::from(*length)))?, alignment)
        }
        _ => {
            return Err(unsupported(
                "POD Store fields require only finite numeric Record, Tuple and Array values",
            ));
        }
    })
}

/// Every new Store occurrence, including unused metadata and constructor-only
/// elements, needs its exact complete sealed resource, independently of Types.
pub(super) fn carrier(
    module: &TypedModule,
    element: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    if !module.target.is_supported_x64_llvm() {
        return Err(unsupported(
            "POD Store requires checked Linux x64 or Win64 LLVM",
        ));
    }
    let Type::Record { name, .. } = element else {
        return Err(unsupported("POD Store element requires an ordinary Record"));
    };
    let (size, _) = layout(element, budget)?;
    if size == 0 {
        return Err(unsupported(
            "POD Store element requires nonzero actual inline layout",
        ));
    }
    budget.spend(
        PREFIX.len().saturating_add(name.len()).saturating_add(1),
        WORK,
    )?;
    let canonical = format!("{PREFIX}{name}>");
    budget.spend(module.resources.len(), WORK)?;
    let mut found = 0usize;
    for resource in &module.resources {
        budget.spend(resource.name.len().saturating_add(canonical.len()), WORK)?;
        let related = if let Type::OwnedStore { element: actual } = &resource.payload {
            match actual.as_ref() {
                Type::Record { name: actual, .. } | Type::Package { name: actual, .. } => {
                    budget.spend(actual.len().saturating_add(name.len()), WORK)?;
                    actual == name
                }
                _ => false,
            }
        } else {
            false
        };
        if resource.name != canonical && !related {
            continue;
        }
        budget.type_work(&resource.payload, false, WORK)?;
        budget.type_work(element, false, WORK)?;
        if resource.name != canonical
            || resource.teardown.is_some()
            || !matches!(&resource.payload, Type::OwnedStore { element: actual } if actual.as_ref() == element)
        {
            return Err(bad(
                "POD Store requires its exact canonical definition and no user teardown",
            ));
        }
        found += 1;
    }
    if found != 1 {
        return Err(bad("POD Store requires exactly one canonical definition"));
    }
    Ok(())
}

fn value(function: &TypedFunction, value: ValueId) -> CompileResult<&Type> {
    function
        .values
        .get(value.0 as usize)
        .ok_or_else(|| bad("POD Store operand outside value table"))
}

/// Called during the existing instruction visit. Legacy Package operations
/// return without a new traversal or charge; new POD operands/results are exact.
pub(super) fn instruction(
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> CompileResult<()> {
    let element = match &instruction.kind {
        InstructionKind::OwnedStoreNew { element } => element,
        InstructionKind::OwnedStoreReserve { store, .. }
        | InstructionKind::OwnedStorePush { store, .. }
        | InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store }
        | InstructionKind::OwnedStoreBorrow { store, .. }
        | InstructionKind::OwnedStoreBorrowMut { store, .. } => {
            let carrier = value(function, *store)?;
            let (Type::Desc(payload) | Type::Managed(payload)) = carrier else {
                return Err(bad("Store operation requires a Store carrier"));
            };
            let Type::OwnedStore { element } = payload.as_ref() else {
                return Err(bad("Store operation requires a Store payload"));
            };
            if matches!(element.as_ref(), Type::Package { .. }) {
                return Ok(());
            }
            if !matches!(carrier, Type::Desc(_)) {
                return Err(bad("POD Store operation requires a real description"));
            }
            element.as_ref()
        }
        _ => return Ok(()),
    };
    if matches!(element, Type::Package { .. }) {
        return Ok(());
    }
    budget.spend(1, "POD Store instruction")?;
    let result = instruction
        .result
        .ok_or_else(|| bad("POD Store instruction requires an exact result"))?;
    let result = value(function, result)?;
    budget.type_work(element, false, "POD Store complete operand")?;
    budget.type_work(result, false, "POD Store complete result")?;
    let valid = match &instruction.kind {
        InstructionKind::OwnedStoreNew { .. } => matches!(result, Type::Managed(payload)
            if matches!(payload.as_ref(), Type::OwnedStore { element: actual } if actual.as_ref() == element)),
        InstructionKind::OwnedStoreReserve { capacity, .. } => {
            value(function, *capacity)? == &Type::I64 && result == &Type::Bool
        }
        InstructionKind::OwnedStoreLen { .. } | InstructionKind::OwnedStoreCapacity { .. } => {
            result == &Type::I64
        }
        InstructionKind::OwnedStorePush { value: input, .. } => {
            let input = value(function, *input)?;
            budget.type_work(input, false, "POD Store complete push value")?;
            input == element
                && matches!(result, Type::ErrorUnion { error, result } if error.as_ref() == element && **result == Type::Unit)
        }
        InstructionKind::OwnedStorePop { .. } => {
            matches!(result, Type::ErrorUnion { error, result } if **error == Type::Unit && result.as_ref() == element)
        }
        InstructionKind::OwnedStoreBorrow { index, .. } => {
            value(function, *index)? == &Type::I64
                && matches!(result, Type::Address { mutable: false, inner } if inner.as_ref() == element)
        }
        InstructionKind::OwnedStoreBorrowMut { index, .. } => {
            value(function, *index)? == &Type::I64
                && matches!(result, Type::Address { mutable: true, inner } if inner.as_ref() == element)
        }
        _ => unreachable!("Store instruction dispatch"),
    };
    if !valid {
        return Err(bad(
            "POD Store instruction requires exact complete operands and result",
        ));
    }
    Ok(())
}

//! Frozen complete snapshot planner from c04d10aad (also exact 42b7 backend).
//! Accounting oracle only: never used by a production compilation.
use super::*;
use std::cell::Cell;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn enabled() -> bool {
    ENABLED.get()
}
pub(super) fn with_reference<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            ENABLED.set(self.0);
        }
    }
    let _reset = Reset(ENABLED.replace(true));
    f()
}
pub(super) fn plan(
    lir: &LirView<'_>,
    limits: &Limits,
    remaining_work: usize,
) -> CompileResult<super::large_copy_abi::Plans> {
    let mut work = remaining_work;
    let mut plans = BTreeMap::new();
    let store_protocols = dynamic_store::plan(lir.resources, limits, &mut work)?;
    for function in lir.functions {
        dynamic_store::reserve_operations(
            function,
            lir.resources.len(),
            &store_protocols,
            &mut work,
        )?;
        let mut values = Plan::new();
        for (index, ty) in function.values.iter().enumerate() {
            if !matches!(
                enums::storage_shape(ty),
                Type::Array { .. }
                    | Type::Tuple(_)
                    | Type::Record { .. }
                    | Type::Package { .. }
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
            ) {
                continue;
            }
            // Reserve all recursive predicates, layout construction and bounded
            // metadata clones before examining or retaining the complete shape.
            let before = work;
            reserve(ty, &mut work, 0, limits)?;
            if !matches!(enums::storage_shape(ty), Type::Package { .. })
                && !ty.is_copy_value()
                && !stored_borrows::pure_composite(ty)
                && !stored_borrows::shared_array_snapshot(ty)
            {
                continue;
            }
            let machine = llvm_type(ty)?;
            if unresolved_layout(&machine) {
                continue;
            }
            let (bytes, align) = layout(&machine)?;
            if bytes >= MIN_BYTES {
                if matches!(enums::storage_shape(ty), Type::Enum { .. }) {
                    // make_variant/read_variant clone the source enum as well
                    // as its machine layout. Source names are absent from the
                    // machine type; pay their bytes before retaining the slot,
                    // then include this fee in repeated emission reservations.
                    reserve_enum_metadata(ty, &mut work, 0, limits)?;
                }
                values.insert(
                    ValueId(index as u32),
                    Slot {
                        ty: machine,
                        bytes,
                        align,
                        storage: None,
                        work: before - work,
                    },
                );
            }
        }
        if !values.is_empty() {
            // Reserve repeated machine-type clones, slot accesses and emitted
            // operations before growing function event buffers. Use a paid
            // shape upper bound; wide construction/call operands count too.
            let shape = values.values().map(|slot| slot.work).max().unwrap();
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                let operands = match &instruction.kind {
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakePackage { fields }
                    | InstructionKind::MakeVariant { fields, .. } => fields.len(),
                    InstructionKind::Call { arguments, .. }
                    | InstructionKind::CallIndirect { arguments, .. }
                    | InstructionKind::CallDynSlot { arguments, .. }
                    | InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. } => arguments.len(),
                    _ => 2,
                };
                let cost = shape
                    .saturating_mul(operands.saturating_add(4))
                    .saturating_add(32);
                work = work.checked_sub(cost).ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Codegen,
                        "Copy aggregate snapshot emission budget exceeded",
                        Some(instruction.span),
                    )]
                })?;
                if let InstructionKind::MakeVariant { fields, .. } = &instruction.kind
                    && instruction
                        .result
                        .is_some_and(|id| values.contains_key(&id))
                {
                    // Small fields retain canonical enum payload stores. Their
                    // recursive scalar expansion (including array cardinality)
                    // must be paid before store_payload grows event buffers.
                    // Large fields use one already-paid snapshot memcpy.
                    for field in fields {
                        if !values.contains_key(field) {
                            let cost = canonical_payload_work(&function.values[field.0 as usize])
                                .saturating_mul(16);
                            work = work.checked_sub(cost).ok_or_else(|| {
                                vec![Diagnostic::limit(
                                    Phase::Codegen,
                                    "Copy enum canonical payload budget exceeded",
                                    Some(instruction.span),
                                )]
                            })?;
                        }
                    }
                }
            }
            // Ownership snapshots stay independent. The Copy slot pool has
            // no inventory for ownership/cleanup transfers.
            if !store_protocols.is_empty()
                && !values.keys().any(|id| {
                    matches!(
                        enums::storage_shape(&function.values[id.0 as usize]),
                        Type::Package { .. }
                    )
                })
            {
                super::copy_slots::coalesce(function, &mut values, &mut work)?;
            }
            plans.insert(function.id, values);
        }
    }
    let abi = super::large_copy_abi::plan(lir, limits, &mut work)?;
    if !plans.is_empty()
        || abi.functions.values().any(|s| s.large())
        || store_protocols
            .values()
            .any(dynamic_store::Protocols::large)
    {
        work = work.checked_sub(256).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "Copy helper emission budget exceeded",
                None,
            )]
        })?;
    }
    Ok(super::large_copy_abi::Plans {
        remaining_work: work,
        values: plans,
        functions: abi.functions,
        indirect: abi.indirect,
        store_protocols,
    })
}

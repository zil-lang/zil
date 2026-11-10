// Frozen complete bool ownership, consumers and affine qualification.
// Only names and the test-only full-round snapshot hook differ from the source.
fn old_bits_join(target: &mut [bool], source: &[bool], budget: &mut Budget) -> Check<bool> {
    budget.spend(target.len())?;
    let mut changed = false;
    for (a, b) in target.iter_mut().zip(source) {
        if *b && !*a {
            *a = true;
            changed = true;
        }
    }
    Ok(changed)
}
fn old_affine_type(module: &TypedModule, ty: &Type) -> bool {
    match ty {
        Type::MutexGuard { .. } => true,
        Type::BorrowedStorage { storage, life } => {
            life.slots
                .iter()
                .any(|slot| slot.mode == StoredBorrowMode::ExclusiveAddress)
                || old_affine_type(module, storage)
        }
        Type::Tuple(fields) => fields.iter().any(|field| old_affine_type(module, field)),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields
            .iter()
            .any(|(_, field)| old_affine_type(module, field)),
        Type::Array { element, .. } => old_affine_type(module, element),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(|field| old_affine_type(module, field))),
        Type::ErrorUnion { error, result } => {
            old_affine_type(module, error) || old_affine_type(module, result)
        }
        Type::Address { mutable: true, .. } => true,
        Type::Closure(id) => module.closures[id.0 as usize]
            .captures
            .iter()
            .any(|t| matches!(t, Type::Address { mutable: true, .. })),
        _ => false,
    }
}
fn old_consumers(kind: &InstructionKind, affine_values: bool) -> impl Iterator<Item = &ValueId> {
    let (callee, values): (Option<&ValueId>, &[ValueId]) = match kind {
        InstructionKind::Store { value, .. }
        | InstructionKind::StoreOwnedArrayElement { value, .. }
        | InstructionKind::StorePackageField { value, .. } => (None, std::slice::from_ref(value)),
        InstructionKind::MakeOwnedArray { elements }
        | InstructionKind::MakePackage { fields: elements } => (None, elements),
        InstructionKind::MakeManaged { fields: elements }
        | InstructionKind::MakeAggregate { fields: elements }
        | InstructionKind::MakeOwnedTuple { fields: elements }
        | InstructionKind::MakeVariant {
            fields: elements, ..
        } if affine_values => (None, elements),
        InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::ExtractField {
            aggregate: value, ..
        }
        | InstructionKind::ExtractIndex {
            aggregate: value, ..
        } if affine_values => (None, std::slice::from_ref(value)),
        InstructionKind::UpdateField {
            aggregate, value, ..
        }
        | InstructionKind::UpdateIndex {
            aggregate, value, ..
        } if affine_values => (Some(aggregate), std::slice::from_ref(value)),
        InstructionKind::CallClosureOnce { callee, arguments } => (Some(callee), arguments),
        InstructionKind::WithForeignCallback { body, .. }
        | InstructionKind::WithScopedThread { body, .. }
        | InstructionKind::WithSyncCell { body, .. } => (Some(body), &[]),
        InstructionKind::SuspendCall { arguments, .. }
        | InstructionKind::TaskCreate { arguments, .. }
            if affine_values =>
        {
            (None, arguments)
        }
        InstructionKind::Call { arguments, .. }
        | InstructionKind::CallDynSlot { arguments, .. }
        | InstructionKind::CallIndirect { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallClosure { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. }
        | InstructionKind::CallClosureBorrowed { arguments, .. } => (None, arguments),
        InstructionKind::MakeClosure { captures, .. } => (None, captures),
        _ => (None, &[]),
    };
    callee.into_iter().chain(values)
}

fn old_ownership(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<Vec<bool>> {
    let n = f.blocks.len();
    let v = f.values.len();
    reserve(budget, &[n, v, 2])?;
    budget.spend(v.saturating_mul(2))?;
    // Record eligibility during the existing paid classification and later
    // State/callback marking. With no affine SSA value, the extra aggregate
    // consumers cannot change the move state and retain their old fast path.
    let mut affine_values = false;
    let mut affine: Vec<_> = f
        .values
        .iter()
        .map(|t| {
            let value = old_affine_type(module, t);
            affine_values |= value;
            value
        })
        .collect();
    // Owning array/State publication consumes a resource-free concrete callback.
    // This is a move obligation, not a fabricated resource cleanup obligation.
    // Preserve the established ordinary closure Copy profile outside this lane.
    for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
        budget.spend(1)?;
        let elements: &[ValueId] = match &instruction.kind {
            InstructionKind::MakeOwnedArray { elements }
            | InstructionKind::MakePackage { fields: elements } => elements,
            InstructionKind::Call {
                function,
                arguments,
            } if module.functions[function.0 as usize]
                .callable_contract
                .is_none()
                && packages::has_callable_fields(
                    &module.functions[function.0 as usize].return_type,
                ) =>
            {
                arguments
            }
            InstructionKind::StoreOwnedArrayElement { value, .. }
            | InstructionKind::StorePackageField { value, .. } => std::slice::from_ref(value),
            _ => &[],
        };
        budget.spend(elements.len())?;
        for value in elements {
            if matches!(f.values[value.0 as usize], Type::Closure(_)) {
                affine[value.0 as usize] = true;
                affine_values = true;
            }
        }
        if matches!(
            instruction.kind,
            InstructionKind::TakeOwnedArrayElement { .. }
                | InstructionKind::TakePackageField { .. }
        ) && let Some(value) = instruction.result
            && matches!(f.values[value.0 as usize], Type::Closure(_))
        {
            affine[value.0 as usize] = true;
            affine_values = true;
        }
    }
    let mut view = vec![false; v];
    for block in &f.blocks {
        for instruction in &block.instructions {
            if let Some(value) = instruction.result {
                view[value.0 as usize] = affine[value.0 as usize]
                    && matches!(
                        instruction.kind,
                        InstructionKind::Load(_)
                            | InstructionKind::AddressField { .. }
                            | InstructionKind::AddressSlice { .. }
                            | InstructionKind::AddressIndex { .. }
                    );
            }
        }
    }
    let mut inputs = vec![vec![false; v]; n];
    let mut outputs = inputs.clone();
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            if !graph.reachable[b] {
                continue;
            }
            budget.spend(v)?;
            let mut state = vec![false; v];
            if b != f.entry.0 as usize {
                for &p in &graph.predecessors[b] {
                    if graph.reachable[p] {
                        old_bits_join(&mut state, &outputs[p], budget)?;
                    }
                }
            }
            inputs[b].clone_from(&state);
            for instruction in &block.instructions {
                budget.spend(
                    old_consumers(&instruction.kind, affine_values)
                        .count()
                        .saturating_add(1),
                )?;
                if let Some(value) = instruction.result {
                    state[value.0 as usize] = false;
                }
                for value in old_consumers(&instruction.kind, affine_values) {
                    if affine[value.0 as usize] {
                        state[value.0 as usize] = true;
                    }
                }
            }
            if state != outputs[b] {
                outputs[b] = state;
                changed = true;
            }
        }
        TRACE.with(|trace| {
            if let Some(trace) = trace.borrow_mut().as_mut() {
                trace.push((inputs.clone(), outputs.clone()));
            }
        });
        if !changed {
            break;
        }
    }
    for (b, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[b] {
            continue;
        }
        budget.spend(v)?;
        let mut state = inputs[b].clone();
        let check_use = |value: ValueId, state: &[bool], span: Span| -> Check<()> {
            if affine[value.0 as usize] && state[value.0 as usize] {
                return Err(reject(
                    "E-VERIFY-MOVE",
                    Some(span),
                    "non-Copy SSA carrier was already transferred",
                ));
            }
            Ok(())
        };
        for instruction in &block.instructions {
            budget.spend(
                instruction_operands(&instruction.kind)
                    .count()
                    .saturating_add(1),
            )?;
            for value in instruction_operands(&instruction.kind) {
                check_use(value, &state, instruction.span)?;
            }
            if let Some(value) = instruction.result {
                state[value.0 as usize] = false;
            }
            for value in old_consumers(&instruction.kind, affine_values) {
                if !affine[value.0 as usize] {
                    continue;
                }
                check_use(*value, &state, instruction.span)?;
                if view[value.0 as usize] {
                    return Err(reject(
                        "E-VERIFY-MOVE",
                        Some(instruction.span),
                        "Load/projection is an access view; transfer requires TakeLocal or an explicit child reborrow",
                    ));
                }
                state[value.0 as usize] = true;
            }
        }
        if let Terminator::Return(Some(value)) = block.terminator {
            check_use(value, &state, f.span)?;
            if affine[value.0 as usize] && view[value.0 as usize] {
                return Err(reject(
                    "E-VERIFY-MOVE",
                    Some(f.span),
                    "return cannot copy an exclusive access view",
                ));
            }
        }
    }
    Ok(view)
}

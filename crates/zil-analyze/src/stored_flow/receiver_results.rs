//! Returned data permissions and invocation heads remain separate. A caller
//! substitutes actual environment leaves and independently retains the real
//! borrowed receiver as a structural parent, never as a data address.
use super::*;

pub(super) fn applies(d: &ClosureDefinition) -> bool {
    (super::super::stored_callables::lending(d) || super::super::stored_callables::transferred(d))
        && d.storage_life.as_ref().is_some_and(|life| {
            super::super::signature_life::outer(life, StorageBase::Result).is_some()
        })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn invoke(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &State,
    values: &[Value],
    summaries: &[Facts],
    ids: &SlotLoanIds,
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<Value> {
    let (d, arguments) = super::super::stored_callables::called(module, f, &i.kind)
        .ok_or_else(|| failure("returned view has no actual callable entry"))?;
    let needs_head = super::super::stored_callables::result_is_receiver(d);
    let (environment, head) = match &i.kind {
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => {
            let actual = state
                .locals
                .get(callee)
                .ok_or_else(|| failure("returned view receiver is unavailable"))?;
            let head = Permission {
                targets: Targets::from([Target {
                    base: local_capture_base(state, *callee, b)?,
                    stored: Vec::new(),
                    place: Vec::new(),
                    heap_element: None,
                    heap_parents: Vec::new(),
                    retained_port: None,
                }]),
                loans: Loans::new(),
            };
            (environment_memory::current(actual, state, b)?, head)
        }
        InstructionKind::CallClosureBorrowed { callee, .. } => {
            let actual = &values[callee.0 as usize];
            (
                environment_memory::current(actual, state, b)?,
                copy_permission(&actual.pointer, b)?,
            )
        }
        InstructionKind::CallClosure { callee, .. }
        | InstructionKind::CallClosureOnce { callee, .. } => {
            if needs_head {
                return Err(failure(
                    "a consuming invocation cannot preserve an escaping receiver view",
                ));
            }
            let actual = &values[callee.0 as usize];
            (
                environment_memory::current(actual, state, b)?,
                Permission::default(),
            )
        }
        _ => return Err(failure("returned view invocation has no actual receiver")),
    };
    let mut out = Value::default();
    if let Some(sources) = summaries[d.function.0 as usize].returned.get(&Vec::new()) {
        for source in sources {
            let actual = if let HoldRoot::Capture(index) = source.base {
                let ty = d
                    .captures
                    .get(index as usize)
                    .ok_or_else(|| failure("returned view capture is absent"))?;
                if super::super::stored_callables::scalar(ty) && source.stored.is_empty() {
                    let mut actual = copy_permission(&head, b)?;
                    let mut targets = Targets::new();
                    for root in &actual.targets {
                        fee(
                            b,
                            target_width(root)
                                .saturating_add(source.place.len())
                                .saturating_add(1),
                        )?;
                        let mut target = root.clone();
                        target.place.push(PlaceProjection::Field(index));
                        target.place.extend(&source.place);
                        targets.insert(target);
                    }
                    actual.targets = targets;
                    actual
                } else {
                    let key = environment_path(index as usize, &source.stored, b)?;
                    let mut actual = Permission::default();
                    if let Some(leaf) = environment.get(&key) {
                        union(&mut actual, leaf, b)?;
                    }
                    let mut targets = Targets::new();
                    for root in &actual.targets {
                        fee(b, target_width(root).saturating_add(source.place.len()))?;
                        let mut target = root.clone();
                        if let Some(heap) = &mut target.heap_element {
                            heap.extend(&source.place);
                        } else {
                            target.place.extend(&source.place);
                        }
                        targets.insert(target);
                    }
                    actual.targets = targets;
                    actual
                }
            } else {
                substitute(source, arguments, values, state, b)?
            };
            union(&mut out.pointer, &actual, b)?;
        }
    }
    if needs_head {
        let retained = retain_slot(&head, i, u32::MAX - 2, &Vec::new(), ids, loans, b)?;
        for id in &retained.loans {
            loans[*id].receiver_head = true;
            ordered(b, out.pointer.loans.len(), 0)?;
            out.pointer.loans.insert(*id);
        }
    }
    Ok(out)
}

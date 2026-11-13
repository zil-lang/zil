//! Actual closure invocation loans. Public Receiver rows constrain the result;
//! only independently reconstructed instructions and receiver operands supply it.
use super::*;
const HEAD_SITE: u32 = u32::MAX - 1;

pub(super) fn declared(definition: &ClosureDefinition) -> bool {
    definition
        .storage_life
        .as_ref()
        .is_some_and(|life| signature_life::outer(life, StorageBase::Receiver).is_some())
}
fn entry<'a>(module: &'a TypedModule, f: &TypedFunction) -> Option<&'a ClosureDefinition> {
    f.environment
        .and_then(|id| module.closures.get(id.0 as usize))
}
pub(super) fn needs_head(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<bool> {
    let Some(d) = entry(module, f) else {
        return Ok(false);
    };
    b.spend(d.captures.len().saturating_add(1))?;
    Ok(matches!(
        d.kind,
        CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
    ) && (declared(d)
        || d.captures
            .iter()
            .any(|ty| matches!(ty, Type::Address { mutable: true, .. }))))
}
pub(super) fn call_site(module: &TypedModule, f: &TypedFunction, i: &Instruction) -> bool {
    stored_callables::called(module, f, &i.kind).is_some_and(|(d, _)| declared(d))
        && !matches!(
            i.kind,
            InstructionKind::CallClosure { .. } | InstructionKind::CallClosureOnce { .. }
        )
}
pub(super) fn add_site(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    sites: &mut WriteSites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    if !call_site(module, f, i) {
        return Ok(());
    }
    b.spend(sites.len().saturating_add(2))?;
    let exclusive = matches!(
        i.kind,
        InstructionKind::CallClosureMut { .. }
            | InstructionKind::CallClosureBorrowed {
                kind: CallableKind::FnMut,
                ..
            }
    );
    let id = loans.len();
    loans.push(Loan {
        receiver_head: true,
        ..Loan::default()
    });
    sites.push((
        i as *const Instruction as usize,
        HEAD_SITE,
        Vec::new(),
        id,
        exclusive,
    ));
    b.stored_exclusive_loans |= exclusive;
    Ok(())
}
pub(super) fn read(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &Frame,
    out: &mut Payload,
    b: &mut Budget,
) -> Check<bool> {
    if let InstructionKind::BorrowCapture { .. } = i.kind
        && entry(module, f).is_some_and(declared)
    {
        let head = state
            .receiver
            .ok_or_else(|| err("inline environment borrow lacks its invocation head"))?;
        add_loans(&mut out.address.loans, &[head], b)?;
        return Ok(false);
    }
    let InstructionKind::ReadCapture { index } = i.kind else {
        return Ok(false);
    };
    let Some(d) = entry(module, f) else {
        return Ok(false);
    };
    let ty = d
        .captures
        .get(index as usize)
        .ok_or_else(|| err("capture reborrow field is absent"))?;
    if !matches!(ty, Type::Address { mutable: true, .. }) {
        return Ok(false);
    }
    let result = i
        .result
        .ok_or_else(|| err("capture reborrow has no SSA result"))?;
    let Type::Address {
        mutable: to,
        inner: actual,
    } = vt(f, result)?
    else {
        return Err(err("capture reborrow has no actual Address result"));
    };
    let Type::Address {
        inner: expected, ..
    } = ty
    else {
        unreachable!()
    };
    type_work(actual, &b.limits.clone(), b)?;
    type_work(expected, &b.limits.clone(), b)?;
    if actual != expected || *to && d.kind == CallableKind::Fn {
        return Err(err(
            "read-only invocation cannot grant an exclusive captured reborrow",
        ));
    }
    // Consuming entries may read through internal children, but that child
    // still carries this private invocation marker and cannot escape as Fixed.
    // Only a real Take transports the original capture without this marker.
    let head = state
        .receiver
        .ok_or_else(|| err("exclusive capture has no actual invocation head"))?;
    add_loans(&mut out.address.loans, &[head], b)?;
    Ok(true)
}
pub(super) fn returned_head(
    permission: &Permission,
    solution: &Solution,
    b: &mut Budget,
) -> Check<bool> {
    let Some(head) = solution.entry.receiver else {
        return Ok(false);
    };
    let parents = ancestry(&permission.loans, &solution.loans, b)?;
    b.spend(parents.len().saturating_add(1))?;
    Ok(parents.contains(&head))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn receiver_result(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &WriteSites,
    loans: &mut [Loan],
    out: &mut Payload,
    b: &mut Budget,
) -> Check<bool> {
    let Some((d, arguments)) = stored_callables::called(module, f, &i.kind) else {
        return Ok(false);
    };
    if d.storage_life.as_ref().is_none_or(|life| {
        signature_life::outer(life, StorageBase::Result).is_none()
            && !stored_callables::static_result(d)
    }) || !matches!(d.result, Type::Address { .. })
    {
        return Ok(false);
    }
    if declared(d) && !call_site(module, f, i) {
        return Err(err(
            "a consuming call cannot let a receiver-bounded result escape",
        ));
    }
    if static_borrows::stateless_result(module, d, b)? {
        let sources = static_borrows::result_source(d, b)?;
        add_sources(&mut out.address.sources, &sources, b)?;
        return Ok(true);
    }
    let summary = &summaries[d.function.0 as usize];
    // Unknown callback facts never become an identity result or empty lease.
    if !summary.complete {
        return Ok(true);
    }
    let consumed = captured_owner::consuming_value(&i.kind);
    let receiver = if consumed.is_some() {
        Permission::default()
    } else {
        captured_owner::receiver(f, i, values, b)?
    };
    for (path, sources) in &summary.result {
        if !path.is_empty() {
            let (_, shape) = addressed(&d.result)
                .ok_or_else(|| err("returned head lacks its complete pointee directory"))?;
            b.spend(
                shape
                    .slots
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            if !shape.slots.iter().any(|slot| slot.path == *path) {
                return Err(err("returned head selects no actual pointee slot"));
            }
        }
        for source in sources {
            let actual = if let Some(value) = consumed {
                captured_owner::substitute_value(
                    module,
                    f,
                    source,
                    &d.captures,
                    &values[value.0 as usize],
                    arguments,
                    values,
                    state,
                    b,
                )?
            } else {
                captured_owner::substitute(
                    module,
                    f,
                    source,
                    &d.captures,
                    &receiver,
                    arguments,
                    values,
                    state,
                    b,
                )?
            };
            if path.is_empty() {
                join_permission(&mut out.address, &actual, b)?;
            } else {
                add_leaf(out, path, &actual, b)?;
            }
        }
    }
    let life = d
        .storage_life
        .as_ref()
        .ok_or_else(|| err("receiver result relation absent"))?;
    let result_is_head = declared(d)
        && signature_life::outer(life, StorageBase::Result)
            == signature_life::outer(life, StorageBase::Receiver);
    if result_is_head || !summary.receiver_results.is_empty() {
        let retained = retain_written_slot(&receiver, i, HEAD_SITE, &Vec::new(), sites, loans, b)?;
        // Preserve the head as a non-target parent, never add it to data roots.
        add_loans(&mut out.address.loans, &retained.loans, b)?;
    }
    Ok(true)
}

/// A borrowed concrete F parameter supplies a real head parent for a short
/// result. A data target/class row alone cannot establish this obligation.
pub(super) fn parameter_head_result(
    f: &TypedFunction,
    summary: &Summary,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<bool> {
    if !path.is_empty() {
        return Ok(false);
    }
    let Some(life) = &f.storage_life else {
        return Ok(false);
    };
    let Some(StorageBinder::Invocation(bound)) = signature_life::outer(life, StorageBase::Result)
    else {
        return Ok(false);
    };
    source_work(&summary.dyn_result_parents, b)?;
    for parent in &summary.dyn_result_parents {
        let HoldRoot::Parameter(index) = parent.base else {
            continue;
        };
        if parent.retained_port.is_some()
            || !parent.leaf.is_empty()
            || !parent.inline.is_empty()
            || parent.heap_element.is_some()
            || !parent.heap_parents.is_empty()
        {
            continue;
        }
        let actual = f
            .parameters
            .get(index as usize)
            .and_then(|id| f.locals.get(id.0 as usize));
        if matches!(actual.map(|local| &local.ty), Some(Type::Address { inner, .. }) if matches!(inner.as_ref(), Type::Closure(_)))
            && signature_life::outer(life, StorageBase::Parameter(index))
                == Some(StorageBinder::Invocation(bound))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Public Hold includes the real receiver debt, while pointer sources remain
/// the data targets. Only independently created head loans contribute here.
pub(super) fn held_sources(p: &Payload, solution: &Solution, b: &mut Budget) -> Check<Vec<Source>> {
    let mut starts = Vec::new();
    for permission in std::iter::once(&p.address)
        .chain(std::iter::once(&p.store_description))
        .chain(p.leaves.iter().map(|(_, permission)| permission))
        .chain(p.task_ports.iter().map(|(_, permission)| permission))
        .chain(p.dyn_ports.0.iter().map(|(_, row)| &row.permission))
    {
        add_loans(&mut starts, &permission.loans, b)?;
    }
    let parents = ancestry(&starts, &solution.loans, b)?;
    let mut sources = Vec::new();
    for parent in parents {
        b.spend(1)?;
        let loan = solution
            .loans
            .get(parent)
            .ok_or_else(|| err("receiver Hold ancestry exceeds actual loan arena"))?;
        if loan.receiver_head {
            add_sources(&mut sources, &loan.sources, b)?;
        }
    }
    Ok(sources)
}

pub(super) fn helper_head(callee: &TypedFunction, source: &Source) -> bool {
    let HoldRoot::Parameter(index) = source.base else {
        return false;
    };
    source.leaf.is_empty()
        && source.inline.is_empty()
        && source.retained_port.is_none()
        && source.heap_element.is_none()
        && source.heap_parents.is_empty()
        && callee
            .parameters
            .get(index as usize)
            .and_then(|local| callee.locals.get(local.0 as usize))
            .is_some_and(|local| {
                matches!(&local.ty, Type::Address { inner, .. }
                if matches!(inner.as_ref(), Type::Closure(_)))
            })
}

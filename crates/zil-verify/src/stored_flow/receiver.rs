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
    Ok(matches!(d.kind, CallableKind::Fn | CallableKind::FnMut)
        && (declared(d)
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
    loans.push(Loan::default());
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
    // A consuming entry must use a unique Take to transfer an original loan.
    if d.kind == CallableKind::FnOnce {
        return Err(err(
            "consuming exclusive capture requires an actual Take, not ReadCapture",
        ));
    }
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
    if !declared(d) || !matches!(d.result, Type::Address { .. }) {
        return Ok(false);
    }
    if !call_site(module, f, i) {
        return Err(err(
            "a consuming call cannot let a receiver-bounded result escape",
        ));
    }
    let summary = &summaries[d.function.0 as usize];
    // Unknown callback facts never become an identity result or empty lease.
    if !summary.complete {
        return Ok(true);
    }
    let receiver = captured_owner::receiver(f, i, values, b)?;
    for (path, sources) in &summary.result {
        if !path.is_empty() {
            return Err(err("receiver result has unsupported nested leaf transport"));
        }
        for source in sources {
            let actual = captured_owner::substitute(
                module,
                f,
                source,
                &d.captures,
                &receiver,
                arguments,
                values,
                state,
                b,
            )?;
            join_permission(&mut out.address, &actual, b)?;
        }
    }
    let life = d
        .storage_life
        .as_ref()
        .ok_or_else(|| err("receiver result relation absent"))?;
    let result_is_head = signature_life::outer(life, StorageBase::Result)
        == signature_life::outer(life, StorageBase::Receiver);
    if result_is_head || !summary.receiver_results.is_empty() {
        let retained = retain_written_slot(&receiver, i, HEAD_SITE, &[], sites, loans, b)?;
        // Preserve the head as a non-target parent, never add it to data roots.
        add_loans(&mut out.address.loans, &retained.loans, b)?;
    }
    Ok(true)
}

//! Context ports inside a real environment remain logical Capture routes.
//! Moving the owning carrier preserves its existing complete loan graph.
use super::*;

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-SOURCE", None, message)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Option<Payload>> {
    let mut out = Payload::default();
    if let InstructionKind::MakeClosure { closure, captures } = &i.kind {
        let d = crate::closures::definition(module, *closure, Some(i.span))?;
        b.spend(captures.len())?;
        if captures.len() != d.captures.len() {
            return Err(bad("Context environment omits an actual capture"));
        }
        for (index, (value, ty)) in captures.iter().zip(&d.captures).enumerate() {
            if vt(f, *value)? != ty {
                return Err(bad("Context capture differs from its complete actual Type"));
            }
            let actual = values
                .get(value.0 as usize)
                .ok_or_else(|| bad("Context capture actual value is absent"))?;
            for (path, row) in &actual.dyn_ports.0 {
                b.spend(path.len().saturating_add(1))?;
                let mut destination = vec![ContractProjection::Capture(index as u32)];
                destination.extend(path);
                dyn_ports::insert(&mut out.dyn_ports, &destination, row, b)?;
            }
        }
        return Ok(Some(out));
    }
    let (index, take, borrowed) = match i.kind {
        InstructionKind::ReadCapture { index } => (index, false, None),
        InstructionKind::TakeCapture { index } => (index, true, None),
        InstructionKind::BorrowCapture { index, mutable } => (index, false, Some(mutable)),
        _ => return Ok(None),
    };
    let id = f
        .environment
        .ok_or_else(|| bad("Context capture has no actual environment entry"))?;
    let d = crate::closures::definition(module, id, Some(i.span))?;
    if d.function != f.id {
        return Err(bad(
            "Context capture entry belongs to a different environment",
        ));
    }
    let ty = d
        .captures
        .get(index as usize)
        .ok_or_else(|| bad("Context capture position is absent"))?;
    if dyn_ports::shape(module, ty, b)?.is_empty() {
        return Ok(None);
    }
    let actual = state
        .captures
        .get(index as usize)
        .ok_or_else(|| bad("Context capture has no actual current payload"))?;
    dyn_ports::join(&mut out, actual, b)?;
    if let Some(mutable) = borrowed {
        if mutable && d.kind == CallableKind::Fn {
            return Err(bad(
                "shared invocation cannot borrow its owning capture exclusively",
            ));
        }
        let result = i
            .result
            .ok_or_else(|| bad("Context captured borrow has no result"))?;
        if !matches!(vt(f, result)?, Type::Address { mutable: mode, inner } if *mode == mutable && inner.as_ref() == ty)
        {
            return Err(bad(
                "Context captured borrow changes complete Type or access mode",
            ));
        }
        b.spend(1)?;
        out.address.sources.push(Source {
            static_dependency: None,
            base: HoldRoot::Capture(index),
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_element: None,
            heap_parents: vec![],
        });
    }
    if take {
        state.captures[index as usize].dyn_ports = dyn_ports::Rows::default();
    }
    Ok(Some(out))
}

/// Reconstruct a captured port from this actual environment, never another
/// value with the same ClosureId. Physical capture heads and port data use
/// separate paths and preserve all actual loan parents.
#[allow(clippy::too_many_arguments)]
pub(super) fn substitute(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    source: &Source,
    d: &ClosureDefinition,
    arguments: &[ValueId],
    values: &[Payload],
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    let HoldRoot::Capture(index) = source.base else {
        return instantiate(module, f, source, arguments, values, state, b);
    };
    let ty = d
        .captures
        .get(index as usize)
        .ok_or_else(|| bad("actual entry refers to an absent captured port"))?;
    if let Some(PortOrigin::Dyn { carrier, port }) = &source.retained_port {
        if !source.leaf.is_empty()
            || source.heap_element.is_some()
            || !source.heap_parents.is_empty()
        {
            return Err(bad(
                "captured Dyn source mixes logical and physical target namespaces",
            ));
        }
        let endpoint = dyn_ports::carrier_type(module, ty, carrier, b)?;
        let (Type::OwnedDyn { interface } | Type::Dyn { interface }) = endpoint else {
            return Err(bad("captured port has no complete actual Dyn endpoint"));
        };
        if crate::dynamic_ports::interface(module, *interface, b)?
            .retained_life
            .ports
            .get(*port as usize)
            .is_none()
        {
            return Err(bad("captured port exceeds its complete actual directory"));
        }
        b.spend(carrier.len().saturating_add(1))?;
        let mut path = vec![ContractProjection::Capture(index)];
        path.extend(carrier);
        let mut actual = match &i.kind {
            InstructionKind::CallClosure { callee, .. }
            | InstructionKind::CallClosureOnce { callee, .. } => {
                let payload = values
                    .get(callee.0 as usize)
                    .ok_or_else(|| bad("consumed Context environment value is absent"))?;
                dyn_ports::select(payload, &path, *port, b)?
            }
            InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. } => {
                let payload = state
                    .locals
                    .get(callee.0 as usize)
                    .ok_or_else(|| bad("Context environment local is absent"))?;
                dyn_ports::select(payload, &path, *port, b)?
            }
            InstructionKind::CallClosureBorrowed { callee, .. } => {
                let payload = values
                    .get(callee.0 as usize)
                    .ok_or_else(|| bad("borrowed Context environment value is absent"))?;
                dyn_ports::select_actual(module, f, *callee, payload, state, &path, *port, b)?
            }
            _ => return Err(bad("captured port has no actual callable invocation")),
        };
        for target in &mut actual.sources {
            b.spend(source.inline.len())?;
            target.inline.extend(&source.inline);
        }
        return Ok(actual);
    }
    if let Some(value) = captured_owner::consuming_value(&i.kind) {
        let environment = values
            .get(value.0 as usize)
            .ok_or_else(|| bad("consumed environment has no actual payload"))?;
        captured_owner::substitute_value(
            module,
            f,
            source,
            &d.captures,
            environment,
            arguments,
            values,
            state,
            b,
        )
    } else {
        let receiver = captured_owner::receiver(f, i, values, b)?;
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
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn after(
    module: &TypedModule,
    f: &TypedFunction,
    entry: &Frame,
    state: &Frame,
    summary: &mut Summary,
    loans: &[Loan],
    sites: &dyn_leases::Sites,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    let Some(id) = f.environment else {
        return Ok(());
    };
    let d = crate::closures::definition(module, id, Some(f.span))?;
    // A consuming invocation has no surviving environment to update. Its
    // returned value and cleanup are independently checked in their own paths.
    if d.kind == CallableKind::FnOnce {
        return Ok(());
    }
    for (index, ty) in d.captures.iter().enumerate() {
        let shape = dyn_ports::shape(module, ty, b)?;
        if shape.is_empty() {
            continue;
        }
        let original = entry
            .captures
            .get(index)
            .ok_or_else(|| bad("actual capture entry state is absent"))?;
        let actual = state
            .captures
            .get(index)
            .ok_or_else(|| bad("actual capture after state is absent"))?;
        let borrowed = matches!(ty, Type::Address { .. });
        let mut next = dyn_ports::Rows::default();
        for (path, _) in shape {
            let Some(at) = dyn_ports::find(&actual.dyn_ports, &path, b)? else {
                summary.complete = false;
                continue;
            };
            let row = &actual.dyn_ports.0[at].1;
            summary.complete &= row.known;
            if borrowed || d.kind == CallableKind::Fn {
                let old = dyn_ports::find(&original.dyn_ports, &path, b)?
                    .ok_or_else(|| bad("actual capture after has no entry dependency"))?;
                permission_work(&row.permission, b)?;
                permission_work(&original.dyn_ports.0[old].1.permission, b)?;
                if row.permission != original.dyn_ports.0[old].1.permission
                    || row.known != original.dyn_ports.0[old].1.known
                {
                    return Err(unsupported(
                        "changed borrowed/shared Context capture needs its complete pointee after-state transport",
                        Some(f.span),
                    ));
                }
                continue;
            }
            let mut row = row.clone();
            permission_work(&row.permission, b)?;
            for loan in ancestry(&row.permission.loans, loans, b)? {
                b.spend(1)?;
                if loans.get(loan).is_none_or(|loan| loan.receiver_head) {
                    return Err(bad(
                        "persistent Context capture after-state retains its invocation head",
                    ));
                }
            }
            add_sources(
                &mut row.parent_sources,
                &parents(&row.permission, loans, b)?,
                b,
            )?;
            add_sources(
                &mut row.head_sources,
                &sites.head_sources(&row.permission, loans, b)?,
                b,
            )?;
            for source in row
                .permission
                .sources
                .iter()
                .chain(&row.parent_sources)
                .chain(&row.head_sources)
            {
                source_work(std::slice::from_ref(source), b)?;
                if matches!(source.base, HoldRoot::Local(_)) {
                    return Err(bad(
                        "Context capture after-state retains activation-local storage",
                    ));
                }
            }
            row.permission.loans.clear();
            dyn_ports::insert(&mut next, &path, &row, b)?;
        }
        if borrowed || d.kind == CallableKind::Fn {
            continue;
        }
        b.spend(summary.dyn_capture_after.len())?;
        if let Some((_, old)) = summary
            .dyn_capture_after
            .iter_mut()
            .find(|(capture, _)| *capture == index as u32)
        {
            for (path, row) in next.0 {
                dyn_ports::insert(old, &path, &row, b)?;
            }
        } else {
            b.spend(1)?;
            summary.dyn_capture_after.push((index as u32, next));
        }
    }
    Ok(())
}

pub(super) struct Prepared(Vec<(Permission, dyn_ports::Rows)>);

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_after(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Prepared> {
    let mut prepared = Vec::new();
    if !b.dyn_contexts {
        return Ok(Prepared(prepared));
    }
    let Some((d, arguments)) = stored_callables::called(module, f, &i.kind) else {
        return Ok(Prepared(prepared));
    };
    let summary = summaries
        .get(d.function.0 as usize)
        .ok_or_else(|| bad("actual captured after summary is absent"))?;
    if summary.dyn_capture_after.is_empty() {
        return Ok(Prepared(prepared));
    }
    if captured_owner::consuming_value(&i.kind).is_some() {
        return Ok(Prepared(prepared));
    }
    let receiver = captured_owner::receiver(f, i, values, b)?;
    let mut rows = dyn_ports::Rows::default();
    b.spend(summary.dyn_capture_after.len())?;
    for (index, actual) in &summary.dyn_capture_after {
        let ty = d
            .captures
            .get(*index as usize)
            .ok_or_else(|| bad("captured after position is absent"))?;
        for (path, row) in &actual.0 {
            if !row.head_sources.is_empty() {
                return Err(unsupported(
                    "new head-bounded Context capture needs an atomic head transition",
                    Some(i.span),
                ));
            }
            let mut permission = Permission::default();
            for source in &row.permission.sources {
                let mut current = substitute(module, f, i, source, d, arguments, values, state, b)?;
                dyn_returns::initialized(state, &current, b)?;
                let own_old = matches!(source.base, HoldRoot::Capture(capture) if capture == *index)
                    && matches!(&source.retained_port, Some(PortOrigin::Dyn { carrier, port }) if path.split_last().is_some_and(|(last, prefix)| *last == ContractProjection::RetainedPort(*port) && prefix == carrier))
                    && source.leaf.is_empty()
                    && source.inline.is_empty();
                if !own_old {
                    current = dyn_leases::retain(
                        sites,
                        loans,
                        i,
                        dyn_leases::Destination::After(StoragePosition {
                            base: StorageBase::Capture(*index),
                            path: path.clone(),
                        }),
                        dyn_leases::Component::Summary(source.clone()),
                        dyn_ports::mode_at(module, ty, path, b)?,
                        &current,
                        b,
                    )?;
                }
                join_permission(&mut permission, &current, b)?;
            }
            for source in &row.parent_sources {
                let current = substitute(module, f, i, source, d, arguments, values, state, b)?;
                add_loans(&mut permission.loans, &current.loans, b)?;
            }
            b.spend(path.len().saturating_add(1))?;
            let mut destination = vec![ContractProjection::Capture(*index)];
            destination.extend(path);
            dyn_ports::insert(
                &mut rows,
                &destination,
                &dyn_ports::Port {
                    permission,
                    parent_sources: vec![],
                    head_sources: vec![],
                    known: row.known && summary.complete,
                },
                b,
            )?;
        }
    }
    permission_work(&receiver, b)?;
    prepared.push((receiver, rows));
    Ok(Prepared(prepared))
}

pub(super) fn publish_after(state: &mut Frame, prepared: Prepared, b: &mut Budget) -> Check<()> {
    // All sources, new children and parents were prepared from one pre-call
    // snapshot. Multi-head receivers use the existing weak union publication.
    for (receiver, rows) in prepared.0 {
        dyn_ports::publish_owner(&receiver, state, &rows, b)?;
    }
    Ok(())
}

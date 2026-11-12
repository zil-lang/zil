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

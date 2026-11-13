//! Actual data-flow edges through owning erasure and callable environments.
//! Public Static selectors are deliberately absent: only actual instructions
//! and complete provider mappings can contribute a literal root.
use super::*;

#[allow(clippy::too_many_arguments)]
fn invoke(
    caller: &Context<'_>,
    function: FunctionId,
    arguments: &[ValueId],
    environment: Option<Result<LocalId, ValueId>>,
    path: &[ContractProjection],
    active: &mut BTreeSet<(FunctionId, ValueId, Path)>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<Sources> {
    pay(
        b,
        caller
            .module
            .functions
            .len()
            .saturating_add(arguments.len()),
    )?;
    let target = caller
        .module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| err("actual callable entry is absent"))?;
    let callee = context(caller.module, target, b)?;
    let mut out = Sources::new();
    for returned in &callee.returned {
        for source in super::resolve(&callee, *returned, path, active, depth + 1, b)? {
            let ActualSource::Input(source) = source else {
                out.insert(ActualSource::Static);
                continue;
            };
            let actual = match source.base {
                StorageBase::Parameter(index) => super::resolve(
                    caller,
                    *arguments
                        .get(index as usize)
                        .ok_or_else(|| err("actual callable argument absent"))?,
                    &source.path,
                    active,
                    depth + 1,
                    b,
                )?,
                StorageBase::Capture(index) => {
                    pay(b, source.path.len().saturating_add(1))?;
                    let mut capture = vec![ContractProjection::Capture(index)];
                    capture.extend(source.path);
                    match environment
                        .ok_or_else(|| err("actual capture has no called environment"))?
                    {
                        Ok(local) => local_sources(caller, local, &capture, active, depth + 1, b)?,
                        Err(value) => {
                            super::resolve(caller, value, &capture, active, depth + 1, b)?
                        }
                    }
                }
                _ => return Err(err("actual callable result is not an input/capture source")),
            };
            extend(&mut out, actual, b)?;
        }
    }
    Ok(out)
}

pub(super) fn resolve(
    c: &Context<'_>,
    kind: &InstructionKind,
    path: &[ContractProjection],
    active: &mut BTreeSet<(FunctionId, ValueId, Path)>,
    depth: usize,
    b: &mut Budget,
) -> Option<CompileResult<Sources>> {
    let result = (|| match kind {
        InstructionKind::MakeDynOwned { vtable, source, .. }
        | InstructionKind::MakeDynBorrow { vtable, source, .. } => {
            let tail = if matches!(kind, InstructionKind::MakeDynOwned { .. }) {
                let Some((ContractProjection::Place(PlaceProjection::ErrorArm(false)), tail)) =
                    path.split_first()
                else {
                    return Err(err(
                        "actual owning erasure source lacks success carrier path",
                    ));
                };
                tail
            } else {
                path
            };
            let [ContractProjection::RetainedPort(port)] = tail else {
                return Err(err("actual erasure source lacks an exact opaque port"));
            };
            pay(b, c.module.dyn_vtables.len())?;
            let table = c
                .module
                .dyn_vtables
                .iter()
                .find(|table| table.id == *vtable)
                .ok_or_else(|| err("actual erasure table absent"))?;
            pay(b, table.ports.len())?;
            let mut rows = table.ports.iter().filter(|row| row.port == *port);
            let row = rows
                .next()
                .ok_or_else(|| err("actual erasure source row is Unknown"))?;
            if rows.next().is_some() {
                return Err(err("actual erasure source row is repeated"));
            }
            let mut out = Sources::new();
            for position in &row.sources {
                if position.base != StorageBase::Result {
                    return Err(err("actual provider source is not in its complete Self"));
                }
                let actual = if matches!(kind, InstructionKind::MakeDynBorrow { .. }) {
                    stored_read_sources(c, *source, &position.path, active, depth + 1, b)?
                } else {
                    super::resolve(c, *source, &position.path, active, depth + 1, b)?
                };
                extend(&mut out, actual, b)?;
            }
            Ok(out)
        }
        InstructionKind::BorrowDynOwned { local, .. } if !path.is_empty() => {
            local_sources(c, *local, path, active, depth + 1, b)
        }
        InstructionKind::MakeErrorUnion { value, is_error } => {
            let Some((ContractProjection::Place(PlaceProjection::ErrorArm(arm)), tail)) =
                path.split_first()
            else {
                return Err(err("actual outcome source lacks its selected carrier path"));
            };
            if arm != is_error {
                Ok(Sources::new())
            } else {
                super::resolve(c, *value, tail, active, depth + 1, b)
            }
        }
        InstructionKind::CallClosureShared { callee, arguments }
        | InstructionKind::CallClosureMut { callee, arguments } => {
            let (definition, _) = crate::stored_callables::called(c.module, c.f, kind)
                .ok_or_else(|| err("actual callable source definition absent"))?;
            invoke(
                c,
                definition.function,
                arguments,
                Some(Ok(*callee)),
                path,
                active,
                depth + 1,
                b,
            )
        }
        InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallClosureBorrowed {
            callee, arguments, ..
        } => {
            let (definition, _) = crate::stored_callables::called(c.module, c.f, kind)
                .ok_or_else(|| err("actual borrowed callable source definition absent"))?;
            invoke(
                c,
                definition.function,
                arguments,
                Some(Err(*callee)),
                path,
                active,
                depth + 1,
                b,
            )
        }
        InstructionKind::CallDynSlot {
            interface,
            slot,
            callee,
            arguments,
        } => {
            if !path.is_empty() {
                return Err(err("actual getter result has an unsupported nested leaf"));
            }
            let mut out = Sources::new();
            let mut found = false;
            // Entry Return sources determine which ports are used. The public
            // slot upper may contain unused sources and cannot ground a cycle.
            for table in &c.module.dyn_vtables {
                pay(b, 1)?;
                if table.interface != *interface {
                    continue;
                }
                pay(b, table.entries.len())?;
                let entry = table
                    .entries
                    .iter()
                    .find(|entry| entry.slot == *slot)
                    .ok_or_else(|| err("actual getter table omits its entry"))?;
                let target = c
                    .module
                    .functions
                    .get(entry.function.0 as usize)
                    .ok_or_else(|| err("actual getter entry absent"))?;
                let provider = context(c.module, target, b)?;
                found = true;
                for value in &provider.returned {
                    for actual in super::resolve(&provider, *value, &[], active, depth + 1, b)? {
                        let ActualSource::Input(source) = actual else {
                            out.insert(ActualSource::Static);
                            continue;
                        };
                        match source.base {
                            StorageBase::Parameter(0) => {
                                let mut selected = false;
                                for row in &table.ports {
                                    pay(
                                        b,
                                        row.sources
                                            .len()
                                            .saturating_mul(source.path.len().saturating_add(1)),
                                    )?;
                                    if row.sources.iter().any(|p| {
                                        p.base == StorageBase::Result && p.path == source.path
                                    }) {
                                        selected = true;
                                        extend(
                                            &mut out,
                                            super::resolve(
                                                c,
                                                *callee,
                                                &[ContractProjection::RetainedPort(row.port)],
                                                active,
                                                depth + 1,
                                                b,
                                            )?,
                                            b,
                                        )?;
                                    }
                                }
                                if !selected {
                                    return Err(err(
                                        "actual getter returned leaf is not a provider retained port",
                                    ));
                                }
                            }
                            StorageBase::Parameter(index) => {
                                let argument = *arguments
                                    .get(index as usize - 1)
                                    .ok_or_else(|| err("actual getter argument source absent"))?;
                                extend(
                                    &mut out,
                                    super::resolve(
                                        c,
                                        argument,
                                        &source.path,
                                        active,
                                        depth + 1,
                                        b,
                                    )?,
                                    b,
                                )?;
                            }
                            _ => {
                                return Err(err(
                                    "actual getter source is not its actual Self or argument",
                                ));
                            }
                        }
                    }
                }
            }
            if !found {
                return Err(err("actual getter source has no concrete entry proof"));
            }
            Ok(out)
        }
        _ => return Err(err("actual dynamic Static source operation is unproved")),
    })();
    match kind {
        InstructionKind::MakeDynOwned { .. }
        | InstructionKind::MakeDynBorrow { .. }
        | InstructionKind::BorrowDynOwned { .. }
        | InstructionKind::MakeErrorUnion { .. }
        | InstructionKind::CallClosureShared { .. }
        | InstructionKind::CallClosureMut { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallClosureOnce { .. }
        | InstructionKind::CallClosureBorrowed { .. }
        | InstructionKind::CallDynSlot { .. } => Some(result),
        _ => None,
    }
}

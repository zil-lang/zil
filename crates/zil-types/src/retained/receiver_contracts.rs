//! Untrusted class proposals for actual external capture results. Sources and
//! invocation dependencies come from the completed Typed CFG; neither this bit
//! nor a lifetime row grants a target or a loan to either safety kernel.
use super::*;

fn candidate(d: &ClosureDefinition) -> bool {
    matches!(
        d.kind,
        CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
    ) && lending::transferred_result(&d.result)
        && d.parameters.iter().all(Type::is_copy_value)
        && d.captures
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. }))
}

pub(super) fn prepare_creators(module: &mut TypedModule, left: &mut usize) -> R<()> {
    for f in &mut module.functions {
        let Some(returned) = &f.callable_return_contract else {
            continue;
        };
        let d = &module.closures[returned.closure.0 as usize];
        if f.storage_life.is_some() || !candidate(d) {
            continue;
        }
        spend(left, f.parameters.len(), f.span)?;
        let mut life = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: Vec::new(),
            classes: Vec::new(),
        };
        for (index, local) in f.parameters.iter().enumerate() {
            match &f.locals[local.0 as usize].ty {
                Type::Address { inner, .. } if inner.is_address_pointee() => {
                    life.bindings.push(StorageLifeBinding {
                        position: StoragePosition {
                            base: StorageBase::Parameter(index as u32),
                            path: Vec::new(),
                        },
                        binder: StorageBinder::Invocation(life.invocation_binder_count),
                    });
                    life.invocation_binder_count =
                        life.invocation_binder_count.checked_add(1).ok_or_else(|| {
                            unsupported(f.span, "creator address class count overflow")
                        })?;
                }
                ty if ty.is_copy_value() => {}
                _ => {
                    return Err(unsupported(
                        f.span,
                        "external capture result requires its complete actual creator class header",
                    ));
                }
            }
        }
        if life.invocation_binder_count == 0 {
            return Err(unsupported(
                f.span,
                "external capture result has no actual creator Address input",
            ));
        }
        spend(left, borrowed_fp::metadata_work(&life), f.span)?;
        f.storage_life = Some(life);
    }
    Ok(())
}

pub(super) fn finish(module: &mut TypedModule, summaries: &[Summary], left: &mut usize) -> R<()> {
    let mut updates = Vec::new();
    for d in &module.closures {
        if !candidate(d) {
            continue;
        }
        let f = &module.functions[d.function.0 as usize];
        let Some(origin) = summaries[d.function.0 as usize].returned.get(&Vec::new()) else {
            return Err(unsupported(
                f.span,
                "external capture result has no complete Typed return sources",
            ));
        };
        spend(left, origin.inputs.len(), f.span)?;
        if origin.local || origin.unknown || origin.inputs.is_empty() {
            return Err(error(
                "E-LENDING-ORIGIN",
                f.span,
                "external capture result retains local or unknown storage",
            ));
        }
        let mut life = d.storage_life.clone().ok_or_else(|| {
            unsupported(
                f.span,
                "external result omits its complete capture class domain",
            )
        })?;
        spend(left, borrowed_fp::metadata_work(&life), f.span)?;
        let result = if origin.receiver {
            if d.kind == CallableKind::FnOnce {
                return Err(error(
                    "E-CLOSURE-RESULT-LIFE",
                    f.span,
                    "FnOnce must transfer the actual capture; a read cannot escape its receiver",
                ));
            }
            if life
                .bindings
                .iter()
                .any(|row| row.position.base == StorageBase::Receiver)
            {
                return Err(unsupported(f.span, "receiver proposal is already defined"));
            }
            let binder = StorageBinder::Invocation(life.invocation_binder_count);
            life.invocation_binder_count = life
                .invocation_binder_count
                .checked_add(1)
                .ok_or_else(|| unsupported(f.span, "receiver invocation count overflow"))?;
            let position = life
                .bindings
                .iter()
                .position(|row| row.position.base == StorageBase::Result)
                .unwrap_or(life.bindings.len());
            life.bindings.insert(
                position,
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Receiver,
                        path: Vec::new(),
                    },
                    binder,
                },
            );
            binder
        } else {
            let mut selected = None;
            for source in &origin.inputs {
                let StorageBase::Capture(index) = source.base else {
                    return Err(unsupported(
                        f.span,
                        "external capture result has no actual captured source",
                    ));
                };
                spend(left, source.path.len().saturating_add(1), f.span)?;
                let mut path = vec![ContractProjection::Capture(index)];
                path.extend(&source.path);
                let actual =
                    StorageBinder::Fixed(capture_class(module, d.id, &path, 0, left, f.span)?);
                if selected
                    .replace(actual)
                    .is_some_and(|previous| previous != actual)
                {
                    return Err(unsupported(
                        f.span,
                        "returned sources have no proved common capture class",
                    ));
                }
            }
            selected.ok_or_else(|| unsupported(f.span, "external result has no captured class"))?
        };
        if let Some(row) = life
            .bindings
            .iter()
            .find(|row| row.position.base == StorageBase::Result)
        {
            if row.binder != result || !row.position.path.is_empty() {
                return Err(error(
                    "E-CLOSURE-RESULT-LIFE",
                    f.span,
                    "declared result class differs from the actual receiver or captured data dependency",
                ));
            }
        } else {
            life.bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Result,
                    path: Vec::new(),
                },
                binder: result,
            });
        }
        updates.push((d.id, d.function, life));
    }
    for (id, entry, life) in updates {
        let span = module.functions[entry.0 as usize].span;
        spend(
            left,
            borrowed_fp::metadata_work(&life).saturating_mul(2),
            span,
        )?;
        module.closures[id.0 as usize].storage_life = Some(life.clone());
        module.functions[entry.0 as usize].storage_life = Some(life.clone());
        for f in &mut module.functions {
            if let Some(contract) = &mut f.callable_contract {
                for role in &mut contract.roles {
                    if role.closure == id {
                        spend(left, borrowed_fp::metadata_work(&life), f.span)?;
                        role.life = Some(life.clone());
                    }
                }
            }
        }
    }
    complete_helpers(module, summaries, left)?;
    complete_creators(module, left)?;
    Ok(())
}

fn complete_creators(module: &mut TypedModule, left: &mut usize) -> R<()> {
    let mut updates = Vec::new();
    for f in &module.functions {
        let Some(returned) = &f.callable_return_contract else {
            continue;
        };
        let d = &module.closures[returned.closure.0 as usize];
        if !candidate(d) {
            continue;
        }
        let life = d.storage_life.as_ref().ok_or_else(|| {
            unsupported(
                f.span,
                "returned borrowed callable has no complete class inventory",
            )
        })?;
        let header = f.storage_life.as_ref().ok_or_else(|| {
            unsupported(
                f.span,
                "borrowed callable creator has no actual input classes",
            )
        })?;
        spend(left, header.classes.len(), f.span)?;
        let existing = header
            .classes
            .iter()
            .filter(|row| row.position.base == StorageBase::Result)
            .count();
        if existing > 0 {
            if existing != life.fixed_binder_count as usize {
                return Err(unsupported(
                    f.span,
                    "creator result omits actual returned free classes",
                ));
            }
            continue;
        }
        let sources = returned
            .capture_sources
            .as_ref()
            .ok_or_else(|| unsupported(f.span, "creator capture source relation is unknown"))?;
        let mut rows = Vec::new();
        for class in 0..life.fixed_binder_count {
            let mut actual = None;
            for source in sources {
                let StorageBase::Capture(index) = source.position.base else {
                    return Err(unsupported(
                        f.span,
                        "creator class source is not an actual capture",
                    ));
                };
                spend(left, source.position.path.len().saturating_add(1), f.span)?;
                let mut path = vec![ContractProjection::Capture(index)];
                path.extend(&source.position.path);
                if capture_class(module, d.id, &path, 0, left, f.span)? != class {
                    continue;
                }
                for atom in &source.sources {
                    let input = crate::exact_storage_source(atom)?;
                    let found = source_class(module, f, input, left)?;
                    if actual.replace(found).is_some_and(|old| old != found) {
                        return Err(unsupported(
                            f.span,
                            "returned free class has no single proved creator relation",
                        ));
                    }
                }
            }
            if actual.is_none() {
                for row in &life.classes {
                    if row.binder != StorageBinder::Fixed(class) {
                        continue;
                    }
                    let StorageBase::Capture(index) = row.position.base else {
                        continue;
                    };
                    let input = captured_parameter(module, f, d.id, index as usize, left)?;
                    let found = header_class(
                        f,
                        input,
                        row.position.path.clone(),
                        row.position.class,
                        left,
                    )?;
                    if actual.replace(found).is_some_and(|old| old != found) {
                        return Err(unsupported(
                            f.span,
                            "returned phantom class has different actual creator inputs",
                        ));
                    }
                }
            }
            rows.push(SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Result,
                    path: Vec::new(),
                    class,
                },
                binder: actual.ok_or_else(|| {
                    unsupported(
                        f.span,
                        "returned free class has no complete actual creator source",
                    )
                })?,
            });
        }
        updates.push((f.id, rows));
    }
    for (function, rows) in updates {
        let f = &mut module.functions[function.0 as usize];
        spend(left, rows.len(), f.span)?;
        f.storage_life
            .as_mut()
            .ok_or_else(|| unsupported(f.span, "creator header disappeared"))?
            .classes
            .extend(rows);
    }
    Ok(())
}

fn complete_helpers(module: &mut TypedModule, summaries: &[Summary], left: &mut usize) -> R<()> {
    let mut updates = Vec::new();
    for f in &module.functions {
        if f.environment.is_some()
            || f.callable_contract.is_none()
            || !lending::transferred_result(&f.return_type)
        {
            continue;
        }
        let Some(origin) = summaries[f.id.0 as usize].returned.get(&Vec::new()) else {
            continue;
        };
        if origin.local || origin.unknown || origin.inputs.is_empty() {
            continue;
        }
        let mut binder = None;
        for source in &origin.inputs {
            let key = if origin.receiver {
                let StorageBase::Parameter(index) = source.base else {
                    return Err(unsupported(
                        f.span,
                        "helper receiver view is not grounded in an actual input",
                    ));
                };
                let local = *f
                    .parameters
                    .get(index as usize)
                    .ok_or_else(|| unsupported(f.span, "helper receiver input absent"))?;
                if !matches!(&f.locals[local.0 as usize].ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Closure(_)))
                {
                    return Err(error(
                        "E-LENDING-ORIGIN",
                        f.span,
                        "returned receiver view retains an activation-local environment",
                    ));
                }
                let header = f.storage_life.as_ref().ok_or_else(|| {
                    unsupported(
                        f.span,
                        "helper receiver view omits its complete actual input header",
                    )
                })?;
                spend(left, header.bindings.len(), f.span)?;
                header
                    .bindings
                    .iter()
                    .find(|r| {
                        r.position.base == StorageBase::Parameter(index)
                            && r.position.path.is_empty()
                    })
                    .map(|r| r.binder)
                    .ok_or_else(|| {
                        unsupported(f.span, "helper receiver input has no actual head class")
                    })?
            } else {
                source_class(module, f, source, left)?
            };
            if binder.replace(key).is_some_and(|old| old != key) {
                return Err(unsupported(
                    f.span,
                    "helper result has no declared common actual input scope",
                ));
            }
        }
        let binder = binder.ok_or_else(|| unsupported(f.span, "helper result scope absent"))?;
        let header = f
            .storage_life
            .as_ref()
            .ok_or_else(|| unsupported(f.span, "helper result has no complete actual header"))?;
        spend(left, header.bindings.len(), f.span)?;
        if let Some(row) = header
            .bindings
            .iter()
            .find(|r| r.position.base == StorageBase::Result)
        {
            if !row.position.path.is_empty() || row.binder != binder {
                return Err(error(
                    "E-CLOSURE-RESULT-LIFE",
                    f.span,
                    "helper result promise differs from its actual data or receiver scope",
                ));
            }
        } else {
            updates.push((f.id, binder));
        }
    }
    for (id, binder) in updates {
        let f = &mut module.functions[id.0 as usize];
        spend(left, 1, f.span)?;
        f.storage_life
            .as_mut()
            .ok_or_else(|| unsupported(f.span, "helper header disappeared"))?
            .bindings
            .push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Result,
                    path: Vec::new(),
                },
                binder,
            });
    }
    Ok(())
}

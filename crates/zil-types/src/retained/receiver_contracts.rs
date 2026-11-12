//! Untrusted class proposals for actual external capture results. Sources and
//! invocation dependencies come from the completed Typed CFG; neither this bit
//! nor a lifetime row grants a target or a loan to either safety kernel.
use super::*;

fn candidate(d: &ClosureDefinition) -> bool {
    matches!(d.kind, CallableKind::Fn | CallableKind::FnMut)
        && lending::supported_result(&d.result)
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
    Ok(())
}

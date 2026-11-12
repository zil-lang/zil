//! Late concrete helper metadata. Free classes belong to actual input values;
//! newly quantified helper classes never identify two runtime environments.
use super::*;

fn effects(
    module: &TypedModule,
    ty: &Type,
    left: &mut usize,
    span: Span,
    limit: usize,
) -> R<(bool, bool, bool)> {
    let mut todo = vec![(ty, 0usize)];
    let (mut held, mut called, mut written) = (false, false, false);
    while let Some((ty, depth)) = todo.pop() {
        spend(left, 1, span)?;
        if depth > limit {
            return Err(unsupported(
                span,
                "helper capture graph exceeds its finite depth",
            ));
        }
        match ty {
            Type::Address { mutable, .. } => {
                held = true;
                called = true;
                written |= *mutable;
            }
            Type::Desc(_) => {
                called = true;
                written = true;
            }
            Type::BorrowedStorage { life, .. } => {
                spend(left, life.slots.len(), span)?;
                called |= !life.slots.is_empty();
                held |= life.slots.iter().any(|slot| {
                    matches!(
                        slot.mode,
                        StoredBorrowMode::SharedAddress | StoredBorrowMode::ExclusiveAddress
                    )
                });
                written |= life.slots.iter().any(|slot| {
                    matches!(
                        slot.mode,
                        StoredBorrowMode::ExclusiveAddress | StoredBorrowMode::WritableDescription
                    )
                });
            }
            Type::Closure(id) => {
                let d = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| unsupported(span, "helper child definition absent"))?;
                spend(left, d.captures.len(), span)?;
                todo.extend(d.captures.iter().map(|ty| (ty, depth + 1)));
            }
            _ => {}
        }
    }
    Ok((held, called, written))
}

fn rename(
    key: StorageBinder,
    fixed: &mut BTreeMap<u32, u32>,
    invocation: &mut BTreeMap<u32, u32>,
    output: bool,
    span: Span,
) -> R<StorageBinder> {
    let (old, map) = match key {
        StorageBinder::Fixed(i) => (i, fixed),
        StorageBinder::Invocation(i) => (i, invocation),
    };
    let next = map.len() as u32;
    let id = if let Some(id) = map.get(&old) {
        *id
    } else {
        if output {
            return Err(unsupported(
                span,
                "helper output class has no actual input class",
            ));
        }
        map.insert(old, next);
        next
    };
    Ok(match key {
        StorageBinder::Fixed(_) => StorageBinder::Fixed(id),
        StorageBinder::Invocation(_) => StorageBinder::Invocation(id),
    })
}

pub(super) fn populate(module: &mut TypedModule, left: &mut usize, limits: &Limits) -> R<()> {
    spend(left, module.functions.len(), Span::default())?;
    let mut changes = Vec::new();
    for f in &module.functions {
        if f.environment.is_some() {
            continue;
        }
        let Some(claim) = &f.callable_contract else {
            continue;
        };
        spend(left, claim.roles.len(), f.span)?;
        let late =
            claim.roles.iter().any(|role| {
                (role.life.is_none()
                    || role.life.as_ref().is_some_and(|life| {
                        life.fixed_binder_count == 0 && life.classes.is_empty()
                    }) && module
                        .closures
                        .get(role.closure.0 as usize)
                        .is_some_and(|d| d.result.is_b0_scalar()))
                    && module
                        .closures
                        .get(role.closure.0 as usize)
                        .and_then(|d| d.storage_life.as_ref())
                        .is_some()
            });
        if !late {
            continue;
        }
        spend(
            left,
            f.parameters.len().saturating_add(claim.roles.len()),
            f.span,
        )?;
        if let Some(life) = &f.storage_life {
            spend(left, borrowed_fp::metadata_work(life), f.span)?;
        }
        for role in &claim.roles {
            spend(
                left,
                role.hold
                    .len()
                    .saturating_add(role.call.len())
                    .saturating_add(role.writes.len())
                    .saturating_add(1)
                    .saturating_add(role.life.as_ref().map_or(0, borrowed_fp::metadata_work)),
                f.span,
            )?;
        }
        let mut header = f.storage_life.clone().unwrap_or(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: Vec::new(),
            classes: Vec::new(),
        });
        let mut roles = claim.roles.clone();
        for role in &mut roles {
            let d = module
                .closures
                .get(role.closure.0 as usize)
                .ok_or_else(|| unsupported(f.span, "helper concrete definition absent"))?;
            let Some(life) = &d.storage_life else {
                continue;
            };
            if role.life.as_ref() == Some(life)
                || role.life.as_ref().is_some_and(|old| {
                    old.fixed_binder_count > 0
                        || !old.classes.is_empty()
                        || !d.result.is_b0_scalar()
                })
            {
                continue;
            }
            spend(
                left,
                borrowed_fp::metadata_work(life).saturating_add(d.captures.len().saturating_mul(3)),
                f.span,
            )?;
            role.life = Some(life.clone());
            let parameter = *f
                .parameters
                .get(role.parameter as usize)
                .ok_or_else(|| unsupported(f.span, "helper concrete input absent"))?;
            let ty = &f.locals[parameter.0 as usize].ty;
            let pointed = matches!(ty, Type::Address { .. });
            let base = StorageBase::Parameter(role.parameter);
            spend(left, header.bindings.len(), f.span)?;
            if pointed
                && !header
                    .bindings
                    .iter()
                    .any(|row| row.position.base == base && row.position.path.is_empty())
            {
                header.bindings.push(StorageLifeBinding {
                    position: StoragePosition {
                        base,
                        path: Vec::new(),
                    },
                    binder: StorageBinder::Invocation(header.invocation_binder_count),
                });
                header.invocation_binder_count = header
                    .invocation_binder_count
                    .checked_add(1)
                    .ok_or_else(|| unsupported(f.span, "helper invocation class overflow"))?;
            }
            let path = if pointed {
                vec![LifeTypeProjection::Pointee]
            } else {
                Vec::new()
            };
            for class in 0..life.fixed_binder_count {
                spend(
                    left,
                    header
                        .classes
                        .len()
                        .saturating_mul(path.len().saturating_add(1)),
                    f.span,
                )?;
                if !header.classes.iter().any(|row| {
                    row.position.base == base
                        && row.position.path == path
                        && row.position.class == class
                }) {
                    header.classes.push(SignatureLifeClassBinding {
                        position: SignatureLifeClass {
                            base,
                            path: path.clone(),
                            class,
                        },
                        binder: StorageBinder::Invocation(header.invocation_binder_count),
                    });
                    header.invocation_binder_count = header
                        .invocation_binder_count
                        .checked_add(1)
                        .ok_or_else(|| unsupported(f.span, "helper class overflow"))?;
                }
            }
            role.hold.clear();
            role.call.clear();
            role.writes.clear();
            for (index, ty) in d.captures.iter().enumerate() {
                let (held, called, writes) =
                    effects(module, ty, left, f.span, limits.max_call_depth)?;
                if held {
                    role.hold.push(index as u32);
                }
                if called {
                    role.call.push(index as u32);
                }
                if writes {
                    role.writes.push(index as u32);
                }
            }
        }
        if let Some(c) = &f.callable_return_contract
            && let CallableReturnSource::Parameter(parameter) = c.source
        {
            let role = roles
                .iter()
                .find(|role| role.parameter == parameter)
                .ok_or_else(|| unsupported(f.span, "forwarded concrete role absent"))?;
            let d = &module.closures[role.closure.0 as usize];
            if let Some(life) = &d.storage_life {
                for class in 0..life.fixed_binder_count {
                    spend(left, header.classes.len(), f.span)?;
                    let source = header
                        .classes
                        .iter()
                        .find(|row| {
                            row.position.base == StorageBase::Parameter(parameter)
                                && row.position.path.is_empty()
                                && row.position.class == class
                        })
                        .ok_or_else(|| {
                            unsupported(f.span, "forwarded class is not an actual by-value input")
                        })?
                        .binder;
                    header.classes.push(SignatureLifeClassBinding {
                        position: SignatureLifeClass {
                            base: StorageBase::Result,
                            path: Vec::new(),
                            class,
                        },
                        binder: source,
                    });
                }
            }
        }
        // Rebuild canonical occurrence order from actual parameter positions.
        // Original equalities are preserved, while distinct free classes stay
        // distinct unless the real source header already related them.
        let mut bindings = Vec::new();
        let mut classes = Vec::new();
        let mut fixed = BTreeMap::new();
        let mut invocation = BTreeMap::new();
        for position in 0..=f.parameters.len() {
            let output = position == f.parameters.len();
            let base = if output {
                StorageBase::Result
            } else {
                StorageBase::Parameter(position as u32)
            };
            spend(
                left,
                header.bindings.len().saturating_add(header.classes.len()),
                f.span,
            )?;
            for row in header
                .bindings
                .iter()
                .filter(|row| row.position.base == base)
            {
                spend(left, row.position.path.len().saturating_add(1), f.span)?;
                bindings.push(StorageLifeBinding {
                    position: row.position.clone(),
                    binder: rename(row.binder, &mut fixed, &mut invocation, output, f.span)?,
                });
            }
            for row in header
                .classes
                .iter()
                .filter(|row| row.position.base == base)
            {
                spend(left, row.position.path.len().saturating_add(1), f.span)?;
                classes.push(SignatureLifeClassBinding {
                    position: row.position.clone(),
                    binder: rename(row.binder, &mut fixed, &mut invocation, output, f.span)?,
                });
            }
        }
        header.bindings = bindings;
        header.classes = classes;
        header.fixed_binder_count = fixed.len() as u32;
        header.invocation_binder_count = invocation.len() as u32;
        changes.push((f.id, header, roles));
    }
    for (id, header, roles) in changes {
        let f = &mut module.functions[id.0 as usize];
        f.storage_life = Some(header);
        if let Some(c) = &mut f.callable_return_contract
            && let CallableReturnSource::Parameter(parameter) = c.source
        {
            let role = roles
                .iter()
                .find(|role| role.parameter == parameter)
                .unwrap();
            c.hold = role.hold.clone();
            c.call = role.call.clone();
            c.writes = role.writes.clone();
        }
        f.callable_contract.as_mut().unwrap().roles = roles;
    }
    Ok(())
}

/// Owning factories have no callback trace claim: their input is moved into a
/// new environment. Its free class domain still needs an exact input header.
pub(super) fn factory_inputs(module: &mut TypedModule, left: &mut usize) -> R<()> {
    spend(left, module.functions.len(), Span::default())?;
    for f in &mut module.functions {
        if f.callable_contract.is_some()
            || !f
                .callable_return_contract
                .as_ref()
                .is_some_and(|c| c.source == CallableReturnSource::Fresh)
        {
            continue;
        }
        spend(left, f.parameters.len(), f.span)?;
        let mut extra = Vec::new();
        for (position, local) in f.parameters.iter().enumerate() {
            let Type::Closure(id) = f.locals[local.0 as usize].ty else {
                continue;
            };
            let Some(life) = module.closures[id.0 as usize].storage_life.as_ref() else {
                continue;
            };
            for class in 0..life.fixed_binder_count {
                let selector = SignatureLifeClass {
                    base: StorageBase::Parameter(position as u32),
                    path: Vec::new(),
                    class,
                };
                spend(
                    left,
                    f.storage_life.as_ref().map_or(0, |c| c.classes.len()),
                    f.span,
                )?;
                if !f
                    .storage_life
                    .as_ref()
                    .is_some_and(|c| c.classes.iter().any(|row| row.position == selector))
                {
                    extra.push(selector);
                }
            }
        }
        if extra.is_empty() {
            continue;
        }
        let header = f.storage_life.get_or_insert(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: Vec::new(),
            classes: Vec::new(),
        });
        for selector in extra {
            let binder = StorageBinder::Invocation(header.invocation_binder_count);
            header.invocation_binder_count = header
                .invocation_binder_count
                .checked_add(1)
                .ok_or_else(|| unsupported(f.span, "owning factory input class overflow"))?;
            header.classes.push(SignatureLifeClassBinding {
                position: selector,
                binder,
            });
        }
        // Retain existing equalities, then alpha-renumber in actual header
        // occurrence order. No ClosureId identifies two runtime inputs.
        let mut fixed = BTreeMap::new();
        let mut invocation = BTreeMap::new();
        let mut bindings = Vec::new();
        let mut classes = Vec::new();
        for position in 0..=f.parameters.len() {
            let output = position == f.parameters.len();
            let base = if output {
                StorageBase::Result
            } else {
                StorageBase::Parameter(position as u32)
            };
            spend(
                left,
                header.bindings.len().saturating_add(header.classes.len()),
                f.span,
            )?;
            for row in header
                .bindings
                .iter()
                .filter(|row| row.position.base == base)
            {
                spend(left, row.position.path.len().saturating_add(1), f.span)?;
                bindings.push(StorageLifeBinding {
                    position: row.position.clone(),
                    binder: rename(row.binder, &mut fixed, &mut invocation, output, f.span)?,
                });
            }
            for row in header
                .classes
                .iter()
                .filter(|row| row.position.base == base)
            {
                spend(left, row.position.path.len().saturating_add(1), f.span)?;
                classes.push(SignatureLifeClassBinding {
                    position: row.position.clone(),
                    binder: rename(row.binder, &mut fixed, &mut invocation, output, f.span)?,
                });
            }
        }
        header.bindings = bindings;
        header.classes = classes;
        header.fixed_binder_count = fixed.len() as u32;
        header.invocation_binder_count = invocation.len() as u32;
    }
    Ok(())
}

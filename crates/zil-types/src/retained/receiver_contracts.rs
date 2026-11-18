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

fn inferred_receiver(d: &ClosureDefinition) -> bool {
    d.storage_life.is_none()
        && !d.lending_captures.is_empty()
        && matches!(d.kind, CallableKind::Fn | CallableKind::FnMut)
}

fn composed_closures(module: &TypedModule, left: &mut usize) -> R<BTreeSet<ClosureId>> {
    let mut composed = BTreeSet::new();
    for f in &module.functions {
        if let Some(contract) = &f.callable_contract {
            for role in &contract.roles {
                spend(left, helpers::plan::tree_work(composed.len()), f.span)?;
                composed.insert(role.closure);
            }
        }
        if let Some(returned) = &f.callable_return_contract
            && matches!(returned.source, CallableReturnSource::Parameter(_))
        {
            spend(left, helpers::plan::tree_work(composed.len()), f.span)?;
            composed.insert(returned.closure);
        }
        // A borrow or an actual aggregate carrier exposes the concrete entry
        // to another place. Inspect actual type trees, including uninvoked
        // fields that can cross the function's ordinary value boundary.
        for ty in f.locals.iter().map(|local| &local.ty).chain(&f.values) {
            let mut pending = vec![(ty, false)];
            while let Some((ty, nested)) = pending.pop() {
                spend(left, 1, f.span)?;
                match ty {
                    Type::Closure(id) if nested => {
                        spend(left, helpers::plan::tree_work(composed.len()), f.span)?;
                        composed.insert(*id);
                    }
                    Type::Address { inner, .. }
                    | Type::Managed(inner)
                    | Type::OwnedStore { element: inner }
                    | Type::Array { element: inner, .. } => pending.push((inner, true)),
                    Type::BorrowedStorage { storage, .. } => pending.push((storage, nested)),
                    Type::Tuple(fields) => {
                        spend(left, fields.len(), f.span)?;
                        pending.extend(fields.iter().map(|field| (field, true)));
                    }
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        spend(left, fields.len(), f.span)?;
                        pending.extend(fields.iter().map(|(_, field)| (field, true)));
                    }
                    Type::Enum { variants, .. } => {
                        spend(left, variants.len(), f.span)?;
                        for (_, fields) in variants {
                            spend(left, fields.len(), f.span)?;
                            pending.extend(fields.iter().map(|field| (field, true)));
                        }
                    }
                    Type::ErrorUnion { error, result } => {
                        spend(left, 2, f.span)?;
                        pending.extend([(error.as_ref(), true), (result.as_ref(), true)]);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(composed)
}

/// Form the complete receiver signature for an unannotated closure after
/// lending has classified the actual CFG's captured return roots.
pub(crate) fn complete_receivers(
    module: &mut TypedModule,
    left: &mut usize,
    limits: &Limits,
) -> R<()> {
    spend(left, module.closures.len(), Span::default())?;
    if !module.closures.iter().any(inferred_receiver) {
        return Ok(());
    }
    // Local direct calls have their own complete lending and loan proof. A
    // receiver directory transports that relation across a composition.
    let composed = composed_closures(module, left)?;
    let mut updates = Vec::new();
    'closures: for d in &module.closures {
        let entry = &module.functions[d.function.0 as usize];
        let span = entry.span;
        spend(left, helpers::plan::tree_work(composed.len()), span)?;
        if !inferred_receiver(d)
            || !composed.contains(&d.id)
            || entry.storage_life.is_some()
            || entry.call_kind != CallKind::Direct
        {
            continue;
        }
        let Type::Address {
            mutable: false,
            inner,
        } = &d.result
        else {
            continue;
        };
        let result = match inner.as_ref() {
            Type::Slice { element } => element,
            _ => inner,
        };
        // A capture's owned storage may lend, but an external target needs
        // its original Fixed directory instead of an inferred receiver head.
        for ty in d.captures.iter().chain(std::iter::once(result.as_ref())) {
            inference::charge_type_snapshot(ty, span, limits, left, 1)?;
            if !local_scopes::closed(ty, module, left, span, 0, limits.max_call_depth)? {
                continue 'closures;
            }
        }
        let mut life = StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: Vec::new(),
            classes: Vec::new(),
        };
        for (index, ty) in d.parameters.iter().enumerate() {
            inference::charge_type_snapshot(ty, span, limits, left, 1)?;
            if !local_scopes::add(
                ty,
                StorageBase::Parameter(index as u32),
                &mut life,
                module,
                left,
                span,
                limits,
            )? {
                continue 'closures;
            }
        }
        let receiver = StorageBinder::Invocation(life.invocation_binder_count);
        life.invocation_binder_count = life
            .invocation_binder_count
            .checked_add(1)
            .ok_or_else(|| unsupported(span, "receiver invocation count overflow"))?;
        for base in [StorageBase::Receiver, StorageBase::Result] {
            life.bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base,
                    path: Vec::new(),
                },
                binder: receiver,
            });
        }
        borrowed_fp::charge_contract(&life, |work| spend(left, work, span), 2)?;
        updates.push((d.id, d.function, life));
    }
    if updates.is_empty() {
        return Ok(());
    }
    for (id, entry, life) in updates {
        module.closures[id.0 as usize].storage_life = Some(life.clone());
        module.functions[entry.0 as usize].storage_life = Some(life);
    }
    helper_classes::populate(module, left, limits)
}

pub(super) fn prepare_creators(
    module: &mut TypedModule,
    left: &mut usize,
    limits: &Limits,
) -> R<()> {
    fn captures_data(
        module: &TypedModule,
        id: ClosureId,
        active: &mut BTreeSet<ClosureId>,
        left: &mut usize,
        limit: usize,
    ) -> R<bool> {
        let d = module.closures.get(id.0 as usize).ok_or_else(|| {
            unsupported(Span::default(), "creator capture selects an absent closure")
        })?;
        spend(left, d.captures.len().saturating_add(1), Span::default())?;
        if active.len() >= limit || !active.insert(id) {
            return Err(unsupported(
                Span::default(),
                "creator capture graph is cyclic",
            ));
        }
        let mut data = false;
        for ty in &d.captures {
            data |= match ty {
                Type::Address { .. } | Type::BorrowedStorage { .. } => true,
                Type::Closure(child) => captures_data(module, *child, active, left, limit)?,
                Type::Managed(payload) => {
                    matches!(payload.as_ref(), Type::Task { retained_life, .. } if retained_life.binder_count != 0)
                }
                _ => false,
            };
        }
        active.remove(&id);
        Ok(data)
    }
    let mut data_captures = BTreeMap::new();
    for f in &module.functions {
        if let Some(returned) = &f.callable_return_contract {
            data_captures.insert(
                f.id,
                captures_data(
                    module,
                    returned.closure,
                    &mut BTreeSet::new(),
                    left,
                    limits.max_call_depth,
                )?,
            );
        }
    }
    for f in &mut module.functions {
        let Some(returned) = &f.callable_return_contract else {
            continue;
        };
        let d = &module.closures[returned.closure.0 as usize];
        let data_capture = data_captures.get(&f.id).copied().unwrap_or(false);
        if f.storage_life.is_some() || !(candidate(d) || data_capture) {
            continue;
        }
        // No creator input can ground a data class here. Only the later
        // actual StaticUtf8/capture proof may publish a constant result.
        if !f
            .parameters
            .iter()
            .any(|local| matches!(f.locals[local.0 as usize].ty, Type::Address { .. }))
        {
            continue;
        }
        spend(left, f.parameters.len(), f.span)?;
        let mut life = StorageLifeContract {
            static_bindings: Vec::new(),
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
                ty => {
                    zil_ir::source_projection::closed_with_candidates(ty,
                        zil_ir::source_projection::ScopeCandidates { closures: &module.closures, interfaces: &module.dyn_interfaces },
                        limits, left).map_err(|error| match error {
                            zil_ir::source_projection::ProjectionError::Limit => Diagnostic::limit(Phase::Types, "creator closed class geometry work exceeded", Some(f.span)),
                            _ => unsupported(f.span, "external capture result requires its complete actual creator class header"),
                        })?;
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

pub(super) fn finish(
    module: &mut TypedModule,
    summaries: &[Summary],
    left: &mut usize,
    limits: &Limits,
) -> R<()> {
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
        if let Some(life) = &d.storage_life
            && life
                .static_bindings
                .iter()
                .any(|row| row.base == StorageBase::Result)
        {
            if origin.local
                || origin.unknown
                || origin.receiver
                || !origin.places.is_empty()
                || !origin.static_utf8 && origin.inputs.is_empty()
                || origin
                    .inputs
                    .iter()
                    .any(|source| !life.static_bindings.contains(source))
            {
                return Err(error(
                    "E-LENDING-ORIGIN",
                    f.span,
                    "constant result retains a nonconstant capture or activation",
                ));
            }
            continue;
        }
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
        if let Type::Address { inner, .. } = &d.result
            && let Type::BorrowedStorage { life: shape, .. } = inner.as_ref()
        {
            // The structural candidate above grants no classes. Pay the full
            // type comparison before projecting the actual captured head.
            inference::charge_type_snapshot(&d.result, f.span, limits, left, 1)?;
            for source in &origin.inputs {
                let StorageBase::Capture(index) = source.base else {
                    return Err(unsupported(
                        f.span,
                        "qualified result class has no actual captured head",
                    ));
                };
                let capture = d
                    .captures
                    .get(index as usize)
                    .ok_or_else(|| unsupported(f.span, "qualified result capture is absent"))?;
                inference::charge_type_snapshot(capture, f.span, limits, left, 1)?;
                if !source.path.is_empty() || capture != &d.result {
                    return Err(unsupported(
                        f.span,
                        "qualified result requires its complete actual pointee class projection",
                    ));
                }
            }
            for class in 0..shape.binder_count {
                let mut selected = None;
                for source in &origin.inputs {
                    let StorageBase::Capture(index) = source.base else {
                        return Err(unsupported(
                            f.span,
                            "qualified result class has no actual captured head",
                        ));
                    };
                    spend(left, life.classes.len(), f.span)?;
                    let actual = life
                        .classes
                        .iter()
                        .find(|row| {
                            row.position.base == StorageBase::Capture(index)
                                && row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                                && row.position.class == class
                        })
                        .map(|row| row.binder)
                        .ok_or_else(|| {
                            unsupported(
                                f.span,
                                "qualified result omits an actual captured pointee class",
                            )
                        })?;
                    if selected
                        .replace(actual)
                        .is_some_and(|previous| previous != actual)
                    {
                        return Err(error(
                            "E-CLOSURE-RESULT-LIFE",
                            f.span,
                            "qualified result joins distinct original pointee classes",
                        ));
                    }
                }
                let position = SignatureLifeClass {
                    base: StorageBase::Result,
                    path: vec![LifeTypeProjection::Pointee],
                    class,
                };
                let binder = selected.ok_or_else(|| {
                    unsupported(f.span, "qualified result class has no actual source")
                })?;
                if let Some(row) = life.classes.iter().find(|row| row.position == position) {
                    if row.binder != binder {
                        return Err(error(
                            "E-CLOSURE-RESULT-LIFE",
                            f.span,
                            "qualified result changes its declared pointee class",
                        ));
                    }
                } else {
                    spend(left, 2, f.span)?;
                    life.classes
                        .push(SignatureLifeClassBinding { position, binder });
                }
            }
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
        // A fully inventoried zero free domain has no creator class rows to
        // map. Constant occurrence proofs remain separate obligations.
        if life.fixed_binder_count == 0 {
            continue;
        }
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

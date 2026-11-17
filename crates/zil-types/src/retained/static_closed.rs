//! Deferred compiler obligations retain actual environment identities. A
//! proposed zero domain is never itself a ClosedEnvironment proof.
use super::*;

#[allow(clippy::too_many_arguments)]
fn field(
    module: &TypedModule,
    owner: ClosureId,
    capture: u32,
    ty: &Type,
    path: &mut Vec<ContractProjection>,
    life: &StorageLifeContract,
    proven: &sources::StaticOrigins,
    pending: &mut Vec<(ClosureId, Span)>,
    left: &mut usize,
    limits: &Limits,
    span: Span,
    depth: usize,
) -> R<()> {
    spend(left, path.len().saturating_add(1), span)?;
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(Phase::Types,
            "closed environment physical forest exceeds depth", Some(span)));
    }
    match ty {
        Type::Closure(child) => { pending.push((*child, span)); }
        Type::Address { mutable: false, .. } => {
            spend(left, life.static_bindings.len().saturating_mul(path.len().saturating_add(1)), span)?;
            let position = StoragePosition { base: StorageBase::Capture(capture), path: path.clone() };
            if !life.static_bindings.contains(&position) || !proven.capture(owner, &position, left, span)? {
                return Err(error("E-CLOSED-ENVIRONMENT", span,
                    "constant environment leaf lacks complete actual constructor and write proofs"));
            }
        }
        Type::Address { mutable: true, .. } | Type::Desc(_) => {
            return Err(error("E-CLOSED-ENVIRONMENT", span,
                "actual environment retains an external or exclusive borrowed leaf"));
        }
        Type::BorrowedStorage { storage, life: domain } => {
            spend(left, domain.slots.len().saturating_add(domain.retained.len()).saturating_add(domain.nominal_arguments.len()).saturating_add(1), span)?;
            if domain.binder_count != 0 || !domain.slots.is_empty()
                || !domain.retained.is_empty() || !domain.nominal_arguments.is_empty()
            {
                return Err(error("E-CLOSED-ENVIRONMENT", span,
                    "qualified environment retains its complete nonzero or phantom domain"));
            }
            field(module, owner, capture, storage, path, life, proven, pending, left, limits, span, depth + 1)?;
        }
        Type::Tuple(fields) => {
            spend(left, fields.len(), span)?;
            for (index, child) in fields.iter().enumerate() {
                let index = u32::try_from(index).map_err(|_| unsupported(span, "closed Tuple ordinal overflows"))?;
                path.push(ContractProjection::Place(PlaceProjection::Field(index)));
                field(module, owner, capture, child, path, life, proven, pending, left, limits, span, depth + 1)?;
                path.pop();
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            spend(left, fields.len(), span)?;
            for (index, (_, child)) in fields.iter().enumerate() {
                let index = u32::try_from(index).map_err(|_| unsupported(span, "closed Product ordinal overflows"))?;
                path.push(ContractProjection::Place(PlaceProjection::Field(index)));
                field(module, owner, capture, child, path, life, proven, pending, left, limits, span, depth + 1)?;
                path.pop();
            }
        }
        Type::Array { element, length } => {
            spend(left, *length as usize, span)?;
            for index in 0..*length {
                path.push(ContractProjection::Place(PlaceProjection::Field(index)));
                field(module, owner, capture, element, path, life, proven, pending, left, limits, span, depth + 1)?;
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            spend(left, variants.len(), span)?;
            for (variant, (_, fields)) in variants.iter().enumerate() {
                let variant = u32::try_from(variant).map_err(|_| unsupported(span, "closed variant ordinal overflows"))?;
                spend(left, fields.len(), span)?;
                for (index, child) in fields.iter().enumerate() {
                    let index = u32::try_from(index).map_err(|_| unsupported(span, "closed variant field ordinal overflows"))?;
                    path.push(ContractProjection::Place(PlaceProjection::Variant(VariantProjection { variant, index })));
                    field(module, owner, capture, child, path, life, proven, pending, left, limits, span, depth + 1)?;
                    path.pop();
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                path.push(ContractProjection::Place(PlaceProjection::ErrorArm(arm)));
                field(module, owner, capture, child, path, life, proven, pending, left, limits, span, depth + 1)?;
                path.pop();
            }
        }
        _ => {
            if !local_scopes::closed(ty, module, left, span, depth, limits.max_call_depth)? {
                return Err(error("E-CLOSED-ENVIRONMENT", span,
                    "actual environment retains an unproved borrowed or nominal domain"));
            }
            // Owning child environments remain independently checked; their
            // fields are not flattened into this parent's physical positions.
            if let Type::Managed(child) | Type::OwnedStore { element: child }
                | Type::Mutex { payload: child } = ty
            {
                field(module, owner, capture, child, path, life, proven, pending, left, limits, span, depth + 1)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn check(
    module: &TypedModule,
    obligations: &BTreeMap<ClosureId, Span>,
    proven: &sources::StaticOrigins,
    left: &mut usize,
    limits: &Limits,
) -> R<()> {
    spend(left, obligations.len(), Span::default())?;
    let mut pending = obligations.iter().map(|(id, span)| (*id, *span)).collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    while let Some((id, span)) = pending.pop() {
        spend(left, seen.len().saturating_add(1), span)?;
        if !seen.insert(id) { continue; }
        let d = module.closures.get(id.0 as usize)
            .ok_or_else(|| unsupported(span, "deferred actual ClosureDefinition is absent"))?;
        let life = d.storage_life.as_ref().ok_or_else(|| unsupported(span,
            "ClosedEnvironment requires a complete actual domain; None is unknown"))?;
        if life.fixed_binder_count != 0 {
            return Err(error("E-CLOSED-ENVIRONMENT", span,
                "actual environment retains free input classes"));
        }
        spend(left, d.captures.len(), span)?;
        for (capture, ty) in d.captures.iter().enumerate() {
            inference::charge_type_snapshot(ty, span, limits, left, 1)?;
            let capture = u32::try_from(capture).map_err(|_| unsupported(span, "capture ordinal overflows"))?;
            field(module, id, capture, ty, &mut Vec::new(), life, proven,
                &mut pending, left, limits, span, 0)?;
        }
    }
    Ok(())
}

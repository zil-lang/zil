//! Select constructor inputs from independently reconstructed SSA payloads.
//! Capture selectors address private environment snapshots, not public GEPs.
use super::*;

pub(super) fn permission(
    module: &TypedModule,
    f: &TypedFunction,
    value: ValueId,
    path: &[ContractProjection],
    values: &[Payload],
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    let root = vt(f, value)?;
    type_work(root, &b.limits.clone(), b)?;
    let mut ty = root;
    let mut physical = Vec::new();
    let mut logical = Vec::new();
    for (index, step) in path.iter().enumerate() {
        b.spend(
            physical
                .len()
                .saturating_add(logical.len())
                .saturating_add(1),
        )?;
        while let Type::BorrowedStorage { storage, .. } = ty {
            ty = storage;
        }
        match step {
            ContractProjection::Place(projection) => {
                ty=match(ty,projection){
                    (Type::Tuple(fields),PlaceProjection::Field(index))=>fields.get(*index as usize),
                    (Type::Record{fields,..}|Type::Package{fields,..},PlaceProjection::Field(index))=>fields.get(*index as usize).map(|(_,ty)|ty),
                    (Type::Array{element,length},PlaceProjection::Field(index))if index<length=>Some(element.as_ref()),
                    (Type::Enum{variants,..},PlaceProjection::Variant(index))=>variants.get(index.variant as usize).and_then(|(_,fields)|fields.get(index.index as usize)),
                    (Type::ErrorUnion{error,result},PlaceProjection::ErrorArm(failed))=>Some(if *failed{error.as_ref()}else{result.as_ref()}),
                    _=>None,
                }.ok_or_else(||err("Task input source is not an actual inline field; a manager pointer supplies no GEP"))?;
                physical.push(*projection);
                logical.push(*step);
            }
            ContractProjection::Capture(index) => {
                let Type::Closure(id) = ty else {
                    return Err(err(
                        "Task logical capture source has another actual carrier",
                    ));
                };
                b.spend(module.closures.len())?;
                let mut rows = module.closures.iter().filter(|row| row.id == *id);
                let closure = rows
                    .next()
                    .ok_or_else(|| err("Task logical capture source has no actual definition"))?;
                if rows.next().is_some() {
                    return Err(err(
                        "Task logical capture source repeats an opaque identity",
                    ));
                }
                ty = closure
                    .captures
                    .get(*index as usize)
                    .ok_or_else(|| err("Task logical capture source has no actual capture"))?;
                // This prefix indexes the private capture payload that the
                // ordinary MakeClosure/call kernels reconstructed already.
                physical.push(PlaceProjection::Field(*index));
                logical.push(*step);
            }
            ContractProjection::RetainedPort(port) => {
                if index + 1 != path.len() {
                    return Err(err(
                        "Task input retained port must select its complete declared leaf",
                    ));
                }
                while let Type::Managed(inner)
                | Type::Desc(inner)
                | Type::BorrowedStorage { storage: inner, .. } = ty
                {
                    ty = inner;
                }
                let payload = values
                    .get(value.0 as usize)
                    .ok_or_else(|| err("Task input SSA payload is absent"))?;
                return match ty {
                    Type::Task { retained_life, .. } => {
                        let _ = retained_life.ports.get(*port as usize).ok_or_else(|| {
                            err("Task input selects a missing actual retained port")
                        })?;
                        select_carrier(payload, &[], &logical, *port, b)
                    }
                    Type::Dyn { .. } | Type::OwnedDyn { .. } => dyn_ports::select_actual(
                        module, f, value, payload, state, &logical, *port, b,
                    ),
                    _ => Err(err(
                        "Task input opaque port has no independently reconstructed manager lease",
                    )),
                };
            }
            ContractProjection::RetainedClass(_) => {
                return Err(err("Task class metadata cannot supply a stored permission"));
            }
        }
    }
    // Reuse the ordinary call substitution, including current mutable slots,
    // parent loans, nested heap paths and conditional literal dependencies.
    let relative = Source {
        static_dependency: StaticProvenance::NotStatic,
        retained_port: None,
        base: HoldRoot::Parameter(0),
        leaf: physical,
        inline: vec![],
        heap_parents: vec![],
        heap_element: None,
    };
    instantiate(module, f, &relative, &[value], values, state, b)
}

pub(super) fn leaf(
    module: &TypedModule,
    root: &Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<Type> {
    let mut ty = crate::semantic::task_domains::contract_leaf(module, root, path, b)?;
    // The outer variable occurrence is represented by the port binder; the
    // complete pointee qualification remains part of this typed leaf.
    while let Type::BorrowedStorage { storage, .. } = ty {
        ty = *storage;
    }
    Ok(ty)
}

pub(super) fn inactive(
    f: &TypedFunction,
    value: ValueId,
    path: &[ContractProjection],
    values: &[Payload],
    b: &mut Budget,
) -> Check<bool> {
    let Some(payload) = values.get(value.0 as usize) else {
        return Err(err("Task input inactivity proof has no SSA payload"));
    };
    if path
        .iter()
        .any(|step| !matches!(step, ContractProjection::Place(_)))
    {
        return Ok(false);
    }
    let physical = path
        .iter()
        .map(|step| match step {
            ContractProjection::Place(projection) => *projection,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    b.spend(physical.len())?;
    if !matches!(
        physical.first(),
        Some(PlaceProjection::Variant(_) | PlaceProjection::ErrorArm(_))
    ) {
        return Ok(false);
    }
    payload.enum_variants.work(b)?;
    Ok(payload.enum_variants.grounded(vt(f, value)?, b)?
        && !payload.enum_variants.slot_active(&physical))
}

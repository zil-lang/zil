//! Independently recognize an invocation-only outer-head relation. This only
//! selects the existing ordinary verifier; it supplies no target or permission.
use super::*;

fn empty(shape: &zil_ir::RetainedLifeShape) -> bool {
    shape.binder_count == 0
        && shape.ports.is_empty()
        && shape.retained.is_empty()
        && shape.nominal_arguments.is_empty()
}

fn zero(
    module: &TypedModule,
    ty: &Type,
    active: &mut Vec<zil_ir::ClosureId>,
    depth: usize,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("ordinary head type domain exceeds selected depth"));
    }
    let mut child = |ty| zero(module, ty, active, depth + 1, b);
    match ty {
        Type::Address { .. } | Type::Desc(_) | Type::Named(_) | Type::Unsupported(_) => Ok(false),
        Type::BorrowedStorage { storage, life } => {
            if life.binder_count != 0
                || !life.slots.is_empty()
                || !life.retained.is_empty()
                || !life.nominal_arguments.is_empty()
            {
                return Ok(false);
            }
            child(storage)
        }
        Type::Managed(inner)
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::Opaque { hidden: inner, .. }
        | Type::OwnedStore { element: inner }
        | Type::Mutex { payload: inner }
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner } => child(inner),
        Type::Task {
            result,
            retained_life,
        }
        | Type::MutexGuard {
            payload: result,
            retained_life,
        } => {
            if !empty(retained_life) {
                return Ok(false);
            }
            // A zero frame input domain does not erase borrowed result data.
            child(result)
        }
        Type::Tuple(fields) => {
            for field in fields {
                if !child(field)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                if !child(field)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    if !child(field)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        Type::ErrorUnion { error, result } => Ok(child(error)? && child(result)?),
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            b.spend(module.dyn_interfaces.len())?;
            let mut found = module.dyn_interfaces.iter().filter(|d| d.id == *interface);
            let Some(actual) = found.next() else {
                return Ok(false);
            };
            if found.next().is_some() || !empty(&actual.retained_life) {
                return Ok(false);
            }
            // Interface inventory authenticates this complete Context elsewhere;
            // its absence alone does not discard a nonempty retained shape.
            if let Some(context) = &actual.context {
                return zero(module, context, active, depth + 1, b);
            }
            Ok(true)
        }
        Type::Closure(id) => {
            b.spend(active.len().saturating_add(1))?;
            if active.contains(id) {
                return Ok(false);
            }
            let Some(d) = module.closures.get(id.0 as usize) else {
                return Ok(false);
            };
            let Some(life) = &d.storage_life else {
                return Ok(false);
            };
            let Some(entry) = module.functions.get(d.function.0 as usize) else {
                return Ok(false);
            };
            crate::signature_life::charge(life, b)?;
            if let Some(entry_life) = &entry.storage_life {
                crate::signature_life::charge(entry_life, b)?;
            }
            if d.id != *id
                || entry.id != d.function
                || entry.environment != Some(*id)
                || entry.storage_life.as_ref() != Some(life)
                || life.fixed_binder_count != 0
                || !life.classes.is_empty()
                || life
                    .bindings
                    .iter()
                    .any(|r| matches!(r.position.base, StorageBase::Capture(_)))
            {
                return Ok(false);
            }
            b.spend(1)?;
            active.push(*id);
            for capture in &d.captures {
                if !zero(module, capture, active, depth + 1, b)? {
                    active.pop();
                    return Ok(false);
                }
            }
            active.pop();
            Ok(true)
        }
        // Thin code has no captured data. Its invocation signature is checked
        // separately; a borrowed code-pointer relation remains a richer domain.
        Type::FunctionPointer { .. } | Type::ForeignCallback { .. } => Ok(true),
        Type::BorrowedFunctionPointer { .. } => Ok(false),
        t if t.is_b0_scalar() => Ok(true),
        Type::ByteBuffer
        | Type::ForeignHandle
        | Type::SyncCell(_)
        | Type::TaskI32
        | Type::CWideString => Ok(true),
        _ => Ok(false),
    }
}

fn head_type(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let inner = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    zero(module, inner, &mut Vec::new(), 0, b)
}

/// The relation remains present and exact: every outer Address input appears
/// once in source order, with canonical invocation binders and no output row.
pub(super) fn check(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<bool> {
    let Some(c) = &f.storage_life else {
        return Ok(false);
    };
    if f.environment.is_some()
        || f.foreign.is_some()
        || f.call_kind != CallKind::Direct
        || f.reference_return_bound.is_some()
        || c.fixed_binder_count != 0
        || !c.classes.is_empty()
        || c.bindings.is_empty()
    {
        return Ok(false);
    }
    crate::signature_life::charge(c, b)?;
    if !zero(module, &f.return_type, &mut Vec::new(), 0, b)? {
        return Ok(false);
    }
    let mut row = 0;
    let mut next = 0;
    for (ordinal, local) in f.parameters.iter().enumerate() {
        b.spend(1)?;
        let Some(ty) = f.locals.get(local.0 as usize).map(|l| &l.ty) else {
            return Ok(false);
        };
        if !head_type(module, ty, b)? {
            return Ok(false);
        }
        if matches!(ty, Type::Address { .. }) {
            let Some(binding) = c.bindings.get(row) else {
                return Ok(false);
            };
            let StorageBinder::Invocation(id) = binding.binder else {
                return Ok(false);
            };
            if binding.position.base != StorageBase::Parameter(ordinal as u32)
                || !binding.position.path.is_empty()
                || id > next
                || id >= c.invocation_binder_count
            {
                return Ok(false);
            }
            if id == next {
                next += 1;
            }
            row += 1;
        }
    }
    Ok(row == c.bindings.len() && next == c.invocation_binder_count)
}

/// A body containing any actual retained/unknown data keeps its rich ledger,
/// even when its public parameter relation has only outer invocation heads.
pub(super) fn body(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<bool> {
    if !check(module, f, b)? {
        return Ok(false);
    }
    for ty in f.values.iter().chain(f.locals.iter().map(|l| &l.ty)) {
        if !head_type(module, ty, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn requires_stored(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
) -> Check<bool> {
    if body(module, f, b)? {
        return Ok(false);
    }
    super::stored_flow::qualified_function(f, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shape() -> zil_ir::RetainedLifeShape {
        zil_ir::RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        }
    }
    fn fixture(inner: Type) -> (TypedModule, TypedFunction) {
        let (module, _) = crate::tests::baseline();
        let mut f = module.functions[0].clone();
        f.parameters = vec![LocalId(0)];
        f.locals[0].ty = Type::Address {
            mutable: true,
            inner: Box::new(inner),
        };
        f.storage_life = Some(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            classes: vec![],
            bindings: vec![StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(0),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(0),
            }],
        });
        (module, f)
    }
    fn accepts(module: &TypedModule, f: &TypedFunction) -> bool {
        check(module, f, &mut Budget::new(&Limits::default())).unwrap()
    }
    #[test]
    fn complete_zero_task_keeps_actual_outer_head_contract() {
        let inner = Type::Task {
            result: Box::new(Type::Tuple(vec![
                Type::Managed(Box::new(Type::I32)),
                Type::I32,
            ])),
            retained_life: shape(),
        };
        let (module, f) = fixture(inner);
        assert!(accepts(&module, &f));
        assert_eq!(f.storage_life.as_ref().unwrap().bindings.len(), 1);
        let mut unknown = f.clone();
        unknown.storage_life = None;
        assert!(!accepts(&module, &unknown));
    }
    #[test]
    fn task_empty_ports_cannot_hide_phantom_or_borrowed_result_domain() {
        let (module, mut f) = fixture(Type::Task {
            result: Box::new(Type::I32),
            retained_life: shape(),
        });
        let Type::Address { inner, .. } = &mut f.locals[0].ty else {
            unreachable!()
        };
        let Type::Task { retained_life, .. } = inner.as_mut() else {
            unreachable!()
        };
        retained_life.binder_count = 1;
        assert!(!accepts(&module, &f));
        let (module, f) = fixture(Type::Task {
            result: Box::new(Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }),
            retained_life: shape(),
        });
        assert!(!accepts(&module, &f));
    }
    #[test]
    fn all_retained_shape_fields_participate_in_zero_proof() {
        let (module, mut f) = fixture(Type::Task {
            result: Box::new(Type::I32),
            retained_life: shape(),
        });
        let Type::Address { inner, .. } = &mut f.locals[0].ty else {
            unreachable!()
        };
        let Type::Task { retained_life, .. } = inner.as_mut() else {
            unreachable!()
        };
        retained_life.retained.push(zil_ir::StorageRetainedBinding {
            path: vec![],
            class: 0,
            binder: 0,
        });
        assert!(!accepts(&module, &f));
    }
    #[test]
    fn head_rows_are_complete_canonical_and_never_result_relations() {
        let (module, f) = fixture(Type::I32);
        assert!(accepts(&module, &f));
        for attack in 0..6 {
            let mut wrong = f.clone();
            let c = wrong.storage_life.as_mut().unwrap();
            match attack {
                0 => c.bindings[0].position.base = StorageBase::Result,
                1 => c.bindings[0]
                    .position
                    .path
                    .push(ContractProjection::Place(PlaceProjection::Field(0))),
                2 => c.bindings[0].binder = StorageBinder::Invocation(1),
                3 => c.invocation_binder_count = 2,
                4 => c.bindings.push(c.bindings[0].clone()),
                _ => c.fixed_binder_count = 1,
            }
            assert!(!accepts(&module, &wrong), "attack {attack}");
        }
    }
    #[test]
    fn owning_and_opaque_wrappers_do_not_erase_nested_borrowed_data() {
        let borrowed = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        for inner in [
            Type::Managed(Box::new(borrowed.clone())),
            Type::Tuple(vec![borrowed.clone()]),
            Type::Opaque {
                identity: zil_base::NodeId(0),
                hidden: Box::new(borrowed),
            },
        ] {
            let (module, f) = fixture(inner);
            assert!(!accepts(&module, &f));
        }
    }
    #[test]
    fn missing_closure_domain_is_not_known_zero() {
        let (module, f) = fixture(Type::Closure(zil_ir::ClosureId(0)));
        assert!(!accepts(&module, &f));
    }
    #[test]
    fn retained_body_keeps_rich_ledger_despite_ordinary_public_head() {
        let (module, mut f) = fixture(Type::I32);
        assert!(accepts(&module, &f));
        f.values.push(Type::Task {
            result: Box::new(Type::I32),
            retained_life: zil_ir::RetainedLifeShape {
                binder_count: 1,
                ..shape()
            },
        });
        assert!(!body(&module, &f, &mut Budget::new(&Limits::default())).unwrap());
        assert!(requires_stored(&module, &f, &mut Budget::new(&Limits::default())).unwrap());
    }
}

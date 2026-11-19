//! Compare immutable physical types within an established bounds proof.
//! Every visited pair follows the same constructor and ordered member. A
//! positive comparison transfers only structural equality of the full type.
use super::*;

/// Each metadata row contains a variable-length path and fixed-width fields.
fn same_rows<T: PartialEq>(
    left: &[T],
    right: &[T],
    path_len: impl Fn(&T) -> usize,
    b: &mut Budget,
) -> Check<bool> {
    if left.len() != right.len() {
        return Ok(false);
    }
    b.spend(left.len())?;
    for (left, right) in left.iter().zip(right) {
        b.spend(path_len(left))?;
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn compare(left: &Type, right: &Type, b: &mut Budget) -> Check<bool> {
    fn same_name(left: &str, right: &str, b: &mut Budget) -> Check<bool> {
        if left.len() != right.len() {
            return Ok(false);
        }
        b.spend(left.len())?;
        Ok(left == right)
    }

    let mut next = Some((left, right));
    let mut pending = Vec::new();
    while let Some((left, right)) = next.take().or_else(|| pending.pop()) {
        b.spend(1)?;
        if std::ptr::eq(left, right) {
            continue;
        }
        if std::mem::discriminant(left) != std::mem::discriminant(right) {
            return Ok(false);
        }
        // Match the left constructor exhaustively. The discriminant check
        // above also keeps the grouped right-hand patterns in the same variant.
        match left {
            Type::Address { mutable, inner } => {
                let Type::Address {
                    mutable: other_mode,
                    inner: other_inner,
                } = right
                else {
                    return Ok(false);
                };
                if mutable != other_mode {
                    return Ok(false);
                }
                next = Some((inner.as_ref(), other_inner.as_ref()));
            }
            Type::Managed(inner)
            | Type::Desc(inner)
            | Type::OwnedStore { element: inner }
            | Type::Slice { element: inner }
            | Type::Mutex { payload: inner }
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner } => {
                let (Type::Managed(other)
                | Type::Desc(other)
                | Type::OwnedStore { element: other }
                | Type::Slice { element: other }
                | Type::Mutex { payload: other }
                | Type::Registration { environment: other }
                | Type::AsyncRegistration { environment: other }) = right
                else {
                    return Ok(false);
                };
                next = Some((inner.as_ref(), other.as_ref()));
            }
            Type::BorrowedStorage { storage, life } => {
                let Type::BorrowedStorage {
                    storage: other_storage,
                    life: other_life,
                } = right
                else {
                    return Ok(false);
                };
                if life.binder_count != other_life.binder_count
                    || !same_rows(&life.slots, &other_life.slots, |row| row.path.len(), b)?
                    || !same_rows(
                        &life.retained,
                        &other_life.retained,
                        |row| row.path.len(),
                        b,
                    )?
                    || !same_rows(
                        &life.nominal_arguments,
                        &other_life.nominal_arguments,
                        |row| row.path.len(),
                        b,
                    )?
                {
                    return Ok(false);
                }
                next = Some((storage.as_ref(), other_storage.as_ref()));
            }
            Type::Task {
                result: inner,
                retained_life: life,
            }
            | Type::MutexGuard {
                payload: inner,
                retained_life: life,
            } => {
                let (Type::Task {
                    result: other_inner,
                    retained_life: other_life,
                }
                | Type::MutexGuard {
                    payload: other_inner,
                    retained_life: other_life,
                }) = right
                else {
                    return Ok(false);
                };
                if life.binder_count != other_life.binder_count
                    || life.ports.len() != other_life.ports.len()
                    || !same_rows(
                        &life.retained,
                        &other_life.retained,
                        |row| row.path.len(),
                        b,
                    )?
                    || !same_rows(
                        &life.nominal_arguments,
                        &other_life.nominal_arguments,
                        |row| row.path.len(),
                        b,
                    )?
                {
                    return Ok(false);
                }
                b.spend(life.ports.len().saturating_mul(2))?;
                for (left, right) in life.ports.iter().zip(&other_life.ports) {
                    if left.binder != right.binder || left.mode != right.mode {
                        return Ok(false);
                    }
                    pending.push((&left.leaf, &right.leaf));
                }
                next = Some((inner.as_ref(), other_inner.as_ref()));
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                let (Type::Record {
                    name: other_name,
                    fields: other_fields,
                }
                | Type::Package {
                    name: other_name,
                    fields: other_fields,
                }) = right
                else {
                    return Ok(false);
                };
                if fields.len() != other_fields.len() || !same_name(name, other_name, b)? {
                    return Ok(false);
                }
                b.spend(fields.len().saturating_mul(2))?;
                for ((name, field), (other, actual)) in fields.iter().zip(other_fields) {
                    if !same_name(name, other, b)? {
                        return Ok(false);
                    }
                    pending.push((field, actual));
                }
            }
            Type::Enum { name, variants } => {
                let Type::Enum {
                    name: other_name,
                    variants: other_variants,
                } = right
                else {
                    return Ok(false);
                };
                if variants.len() != other_variants.len() || !same_name(name, other_name, b)? {
                    return Ok(false);
                }
                b.spend(variants.len())?;
                for ((name, fields), (other, actual)) in variants.iter().zip(other_variants) {
                    if fields.len() != actual.len() || !same_name(name, other, b)? {
                        return Ok(false);
                    }
                    b.spend(fields.len().saturating_mul(2))?;
                    pending.extend(fields.iter().zip(actual));
                }
            }
            Type::Array { element, length } => {
                let Type::Array {
                    element: other_element,
                    length: other_length,
                } = right
                else {
                    return Ok(false);
                };
                if length != other_length {
                    return Ok(false);
                }
                next = Some((element.as_ref(), other_element.as_ref()));
            }
            Type::Tuple(fields) => {
                let Type::Tuple(other_fields) = right else {
                    return Ok(false);
                };
                if fields.len() != other_fields.len() {
                    return Ok(false);
                }
                b.spend(fields.len().saturating_mul(2))?;
                pending.extend(fields.iter().zip(other_fields));
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                let (Type::FunctionPointer {
                    parameters: other_parameters,
                    result: other_result,
                }
                | Type::ForeignCallback {
                    parameters: other_parameters,
                    result: other_result,
                }) = right
                else {
                    return Ok(false);
                };
                if parameters.len() != other_parameters.len() {
                    return Ok(false);
                }
                b.spend(parameters.len().saturating_mul(2))?;
                next = Some((result.as_ref(), other_result.as_ref()));
                pending.extend(parameters.iter().zip(other_parameters));
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                let Type::BorrowedFunctionPointer {
                    parameters: other_parameters,
                    result: other_result,
                    life: other_life,
                } = right
                else {
                    return Ok(false);
                };
                if parameters.len() != other_parameters.len()
                    || life.fixed_binder_count != other_life.fixed_binder_count
                    || life.invocation_binder_count != other_life.invocation_binder_count
                    || !same_rows(
                        &life.bindings,
                        &other_life.bindings,
                        |row| row.position.path.len(),
                        b,
                    )?
                    || !same_rows(
                        &life.static_bindings,
                        &other_life.static_bindings,
                        |row| row.path.len(),
                        b,
                    )?
                    || !same_rows(
                        &life.classes,
                        &other_life.classes,
                        |row| row.position.path.len(),
                        b,
                    )?
                {
                    return Ok(false);
                }
                b.spend(parameters.len().saturating_mul(2))?;
                next = Some((result.as_ref(), other_result.as_ref()));
                pending.extend(parameters.iter().zip(other_parameters));
            }
            Type::ErrorUnion { error, result } => {
                let Type::ErrorUnion {
                    error: other_error,
                    result: other_result,
                } = right
                else {
                    return Ok(false);
                };
                b.spend(1)?;
                next = Some((error.as_ref(), other_error.as_ref()));
                pending.push((result.as_ref(), other_result.as_ref()));
            }
            Type::Opaque { identity, hidden } => {
                let Type::Opaque {
                    identity: other_identity,
                    hidden: other_hidden,
                } = right
                else {
                    return Ok(false);
                };
                if identity != other_identity {
                    return Ok(false);
                }
                next = Some((hidden.as_ref(), other_hidden.as_ref()));
            }
            Type::Named(name) | Type::Unsupported(name) => {
                let (Type::Named(other) | Type::Unsupported(other)) = right else {
                    return Ok(false);
                };
                if !same_name(name, other, b)? {
                    return Ok(false);
                }
            }
            Type::SyncCell(kind) => {
                if !matches!(right, Type::SyncCell(other) if kind == other) {
                    return Ok(false);
                }
            }
            Type::Closure(id) => {
                if !matches!(right, Type::Closure(other) if id == other) {
                    return Ok(false);
                }
            }
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                if !matches!(right, Type::Dyn { interface: other } | Type::OwnedDyn { interface: other } if interface == other)
                {
                    return Ok(false);
                }
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::ByteBuffer
            | Type::ForeignHandle
            | Type::TaskI32
            | Type::CWideString => {}
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_nominals_and_referents_match_derived_equality() {
        fn package(name: &str, fields: &[(&str, Type)]) -> Type {
            Type::Package {
                name: name.to_owned(),
                fields: fields
                    .iter()
                    .map(|(n, t)| ((*n).to_owned(), t.clone()))
                    .collect(),
            }
        }

        let closure = Type::Closure(ClosureId(1));
        let fields = [("left", closure.clone()), ("right", Type::I32)];
        let mut types = vec![
            package("A", &fields),
            package("B", &fields),
            package("A", &[fields[1].clone(), fields[0].clone()]),
            package(
                "A",
                &[("left", Type::Closure(ClosureId(2))), fields[1].clone()],
            ),
        ];
        for inner in types.clone() {
            for mutable in [false, true] {
                let reference = Type::Address {
                    mutable,
                    inner: Box::new(inner.clone()),
                };
                types.push(Type::Tuple(vec![reference.clone(), Type::I32]));
                types.push(Type::Enum {
                    name: "Result".into(),
                    variants: vec![
                        ("ok".into(), vec![reference]),
                        ("err".into(), vec![Type::U8]),
                    ],
                });
            }
        }
        let independent = types.clone();
        for left in &types {
            for right in &independent {
                let mut budget = Budget::new(&Limits::default());
                assert_eq!(compare(left, right, &mut budget).unwrap(), left == right);
            }
        }
        let limits = Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        };
        let error = compare(&types[0], &independent[0], &mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    }

    #[test]
    fn transport_pays_for_mutex_payload_and_guard_port_in_both_profiles() {
        fn resource_types(name: &str) -> [Type; 2] {
            let payload = Type::Record {
                name: name.into(),
                fields: vec![("value".into(), Type::I32)],
            };
            let mutex = Type::Mutex {
                payload: Box::new(payload.clone()),
            };
            let guard = Type::MutexGuard {
                payload: Box::new(payload),
                retained_life: RetainedLifeShape {
                    binder_count: 1,
                    ports: vec![RetainedBorrowPort {
                        leaf: mutex.clone(),
                        mode: StoredBorrowMode::SharedAddress,
                        binder: 0,
                    }],
                    retained: Vec::new(),
                    nominal_arguments: Vec::new(),
                },
            };
            [Type::Managed(Box::new(mutex)), guard]
        }

        let prefix = "Payload";
        let suffix = "Wide".repeat(16);
        let short = resource_types(prefix);
        let long = resource_types(&format!("{prefix}{suffix}"));
        for pod_stores in [false, true] {
            for (index, (short, long)) in short.iter().zip(&long).enumerate() {
                let limits = Limits::default();
                let cost = |ty: &Type| {
                    let mut budget = Budget::new(&limits);
                    budget.pod_stores = pod_stores;
                    budget.stored_borrows = !pod_stores;
                    assert!(transport_equal(ty, &ty.clone(), true, &mut budget).unwrap());
                    limits.max_analysis_steps - budget.remaining
                };
                let short_cost = cost(short);
                let long_cost = cost(long);
                assert!(long_cost - short_cost >= suffix.len() * (index + 1));

                let mut budget = Budget::new(&limits);
                budget.pod_stores = pod_stores;
                budget.stored_borrows = !pod_stores;
                assert!(!transport_equal(short, long, true, &mut budget).unwrap());

                let mut budget = Budget::new(&Limits {
                    max_analysis_steps: long_cost - 1,
                    ..limits
                });
                budget.pod_stores = pod_stores;
                budget.stored_borrows = !pod_stores;
                let err = transport_equal(long, &long.clone(), true, &mut budget).unwrap_err();
                assert_eq!(err.category, zil_base::ResultCategory::ResourceLimit);
            }
        }
    }

    #[test]
    fn task_port_types_and_access_modes_are_part_of_physical_equality() {
        let task = |leaf, mode| Type::Task {
            result: Box::new(Type::I32),
            retained_life: RetainedLifeShape {
                binder_count: 1,
                ports: vec![RetainedBorrowPort {
                    leaf,
                    mode,
                    binder: 0,
                }],
                retained: Vec::new(),
                nominal_arguments: Vec::new(),
            },
        };
        let i32_desc = Type::Desc(Box::new(Type::Record {
            name: "Counter".into(),
            fields: vec![("value".into(), Type::I32)],
        }));
        let i64_desc = Type::Desc(Box::new(Type::Record {
            name: "WideCounter".into(),
            fields: vec![("value".into(), Type::I64)],
        }));
        let types = [
            task(i32_desc.clone(), StoredBorrowMode::ReadOnlyDescription),
            task(i32_desc, StoredBorrowMode::WritableDescription),
            task(i64_desc, StoredBorrowMode::ReadOnlyDescription),
        ];
        let independent = types.clone();
        let limits = Limits::default();
        for left in &types {
            tasks::require_closed_context(left, None).unwrap();
            for right in &independent {
                assert_eq!(
                    compare(left, right, &mut Budget::new(&limits)).unwrap(),
                    left == right
                );
            }
        }
    }
}

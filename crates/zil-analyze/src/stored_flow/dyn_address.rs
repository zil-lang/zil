//! Opaque owner dependencies retain actual pointee targets and parent loans.
//! These permissions are independent of the owning pair's own address/debt.
use super::*;
use zil_ir::ContractProjection;

pub(super) fn work(value: &Value, b: &mut Budget) -> CompileResult<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    fee(b, value.dyn_ports.len())?;
    for (path, permission) in &value.dyn_ports {
        fee(b, path.len().saturating_add(permission.loans.len()))?;
        target_work(&permission.targets, b)?;
    }
    Ok(())
}
pub(super) fn merge(to: &mut Value, from: &Value, b: &mut Budget) -> CompileResult<bool> {
    if !b.dyn_contexts {
        return Ok(false);
    }
    let mut changed = false;
    for (path, permission) in &from.dyn_ports {
        ordered(b, to.dyn_ports.len(), path.len())?;
        fee(b, path.len())?;
        // An explicit empty row proves an unused upper. Missing is unknown.
        if !to.dyn_ports.contains_key(path) {
            changed = true;
        }
        changed |= union(to.dyn_ports.entry(path.clone()).or_default(), permission, b)?;
    }
    Ok(changed)
}
fn key(path: &[PlaceProjection], port: u32) -> Vec<ContractProjection> {
    path.iter()
        .copied()
        .map(ContractProjection::Place)
        .chain([ContractProjection::RetainedPort(port)])
        .collect()
}
/// Substitute only an explicitly known dyn row. Missing is not empty, and
/// borrowing the owner retains its actual head/address ancestors as well.
pub(super) fn select(
    value: &Value,
    carrier: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> CompileResult<Permission> {
    fee(b, carrier.len().saturating_add(1))?;
    let mut key = carrier.to_vec();
    key.push(ContractProjection::RetainedPort(port));
    ordered(b, value.dyn_ports.len(), key.len())?;
    let permission = value
        .dyn_ports
        .get(&key)
        .ok_or_else(|| failure("dyn source has no complete actual opaque-port row"))?;
    let mut out = Permission::default();
    union(&mut out, permission, b)?;
    fee(b, value.pointer.loans.len())?;
    out.loans.extend(&value.pointer.loans);
    Ok(out)
}
fn directory<'a>(
    module: &'a TypedModule,
    ty: &Type,
    path: &mut Path,
    rows: &mut Vec<(Path, &'a DynInterface)>,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    if path.len() > b.partial_depth {
        return Err(failure("dyn dependency directory exceeds type depth"));
    }
    match ty {
        Type::OwnedDyn { interface } | Type::Dyn { interface } => {
            fee(b, module.dyn_interfaces.len())?;
            let item = module
                .dyn_interfaces
                .iter()
                .find(|item| item.id == *interface)
                .ok_or_else(|| failure("dyn dependency has no actual interface"))?;
            if item.context.is_some() {
                fee(b, path.len().saturating_add(1))?;
                rows.push((path.clone(), item));
            }
        }
        Type::Address { inner, .. } => directory(module, inner, path, rows, b)?,
        Type::ErrorUnion { error, result } => {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                path.push(PlaceProjection::ErrorArm(arm));
                directory(module, child, path, rows, b)?;
                path.pop();
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fee(b, fields.len())?;
            for (index, (_, child)) in fields.iter().enumerate() {
                path.push(PlaceProjection::Field(index as u32));
                directory(module, child, path, rows, b)?;
                path.pop();
            }
        }
        Type::Tuple(fields) => {
            fee(b, fields.len())?;
            for (index, child) in fields.iter().enumerate() {
                path.push(PlaceProjection::Field(index as u32));
                directory(module, child, path, rows, b)?;
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            fee(b, variants.len())?;
            for (variant, (_, fields)) in variants.iter().enumerate() {
                fee(b, fields.len())?;
                for (index, child) in fields.iter().enumerate() {
                    path.push(PlaceProjection::Variant(VariantProjection {
                        variant: variant as u32,
                        index: index as u32,
                    }));
                    directory(module, child, path, rows, b)?;
                    path.pop();
                }
            }
        }
        // Array/Store/managed/capture transport needs its actual storage/window
        // consumer. These constructors cannot be treated as empty Contexts.
        Type::Array { element, .. }
        | Type::Managed(element)
        | Type::OwnedStore { element }
        | Type::Desc(element)
        | Type::BorrowedStorage {
            storage: element, ..
        } => {
            let mut nested = Vec::new();
            directory(module, element, path, &mut nested, b)?;
            if !nested.is_empty() {
                return Err(failure(
                    "nested dyn Context requires complete storage/window transport",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn capture_directories(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    fee(b, module.closures.len())?;
    for definition in &module.closures {
        fee(b, definition.captures.len())?;
        for capture in &definition.captures {
            let mut rows = Vec::new();
            directory(module, capture, &mut vec![], &mut rows, b)?;
            if !rows.is_empty() {
                return Err(failure(
                    "dyn Context capture requires its complete retained environment class and source transport",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn seed(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    value: &mut Value,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    let mut rows = Vec::new();
    directory(module, ty, &mut vec![], &mut rows, b)?;
    for (path, item) in rows {
        fee(b, item.retained_life.ports.len())?;
        for (port, upper) in item.retained_life.ports.iter().enumerate() {
            fee(b, path.len().saturating_mul(7).saturating_add(4))?;
            let target = Target {
                base,
                stored: path.clone(),
                retained_port: Some(PortOrigin::Dyn {
                    carrier: path
                        .iter()
                        .copied()
                        .map(ContractProjection::Place)
                        .collect(),
                    port: port as u32,
                }),
                place: vec![],
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            let mutable = upper.mode == StoredBorrowMode::ExclusiveAddress;
            loans.push(Loan {
                targets: Targets::from([target.clone()]),
                parents: Loans::new(),
                mutable,
            });
            b.stored_exclusive_loans |= mutable;
            value.dyn_ports.insert(
                key(&path, port as u32),
                Permission {
                    targets: Targets::from([target]),
                    loans: Loans::from([id]),
                },
            );
        }
    }
    Ok(())
}

fn selected(
    value: &Value,
    position: &StoragePosition,
    b: &mut Budget,
) -> CompileResult<Option<Permission>> {
    fee(b, position.path.len().saturating_add(1))?;
    if position.base != StorageBase::Result {
        return Err(failure(
            "dyn provider source is not rooted in its actual Self",
        ));
    }
    if matches!(
        position.path.last(),
        Some(ContractProjection::RetainedPort(_))
    ) {
        ordered(b, value.dyn_ports.len(), position.path.len())?;
        return value
            .dyn_ports
            .get(&position.path)
            .map(|p| copy_permission(p, b))
            .transpose();
    }
    let mut path = Vec::new();
    for step in &position.path {
        let ContractProjection::Place(step) = step else {
            return Err(failure(
                "dyn provider source needs its actual capture/retained consumer",
            ));
        };
        path.push(*step);
    }
    ordered(b, value.slots.len(), path.len())?;
    value
        .slots
        .get(&path)
        .map(|p| copy_permission(p, b))
        .transpose()
}
fn copy_with_prefix(
    out: &mut Value,
    from: &Value,
    prefix: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<()> {
    work(from, b)?;
    for (path, permission) in &from.dyn_ports {
        fee(b, prefix.len().saturating_add(path.len()))?;
        let mut key = prefix.to_vec();
        key.extend_from_slice(path);
        ordered(b, out.dyn_ports.len(), key.len())?;
        union(out.dyn_ports.entry(key).or_default(), permission, b)?;
    }
    Ok(())
}
fn projection(
    out: &mut Value,
    from: &Value,
    prefix: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<()> {
    work(from, b)?;
    for (path, permission) in &from.dyn_ports {
        fee(b, prefix.len())?;
        if path.starts_with(prefix) {
            fee(b, path.len() - prefix.len())?;
            let key = path[prefix.len()..].to_vec();
            ordered(b, out.dyn_ports.len(), key.len())?;
            union(out.dyn_ports.entry(key).or_default(), permission, b)?;
        }
    }
    Ok(())
}
pub(super) fn admitted_value(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.dyn_contexts {
        return Ok(false);
    }
    let id = match ty {
        Type::OwnedDyn { interface } => Some(*interface),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::OwnedDyn { interface } | Type::Dyn { interface } => Some(*interface),
            _ => None,
        },
        Type::ErrorUnion { error, result } if **error == crate::dyn_dispatch::create_error() => {
            match result.as_ref() {
                Type::OwnedDyn { interface } => Some(*interface),
                _ => None,
            }
        }
        _ => None,
    };
    let Some(id) = id else {
        return Ok(false);
    };
    fee(b, module.dyn_interfaces.len())?;
    Ok(module.dyn_interfaces.iter().any(|i| i.id == id))
}
pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.dyn_contexts {
        return Ok(false);
    }
    if operation(&i.kind) {
        return Ok(true);
    }
    let source = match &i.kind {
        InstructionKind::PlaceTagLocal { local, path } if path.is_empty() => {
            Some(&f.locals[local.0 as usize].ty)
        }
        InstructionKind::TakeVariantField { local, path }
            if matches!(path.as_slice(), [PlaceProjection::ErrorArm(_)]) =>
        {
            Some(&f.locals[local.0 as usize].ty)
        }
        InstructionKind::DropErrorUnion { local } => Some(&f.locals[local.0 as usize].ty),
        InstructionKind::ErrorTag { value }
        | InstructionKind::ReadErrorUnion { value, .. }
        | InstructionKind::TakeErrorUnion { value, .. } => Some(value_type(f, *value)?),
        InstructionKind::MakeErrorUnion { .. } => Some(value_type(
            f,
            i.result
                .ok_or_else(|| failure("dyn union constructor has no result"))?,
        )?),
        _ => None,
    };
    let Some(ty) = source else {
        return Ok(false);
    };
    let Type::ErrorUnion { error, result } = ty else {
        return Ok(false);
    };
    if !admitted_value(module, ty, b)? {
        return Ok(false);
    }
    let expected = match &i.kind {
        InstructionKind::PlaceTagLocal { .. } => Some(Type::I32),
        InstructionKind::ErrorTag { .. } => Some(Type::Bool),
        InstructionKind::TakeVariantField { path, .. } => Some(
            if matches!(path.as_slice(), [PlaceProjection::ErrorArm(true)]) {
                error.as_ref().clone()
            } else {
                result.as_ref().clone()
            },
        ),
        InstructionKind::ReadErrorUnion { is_error, .. }
        | InstructionKind::TakeErrorUnion { is_error, .. } => Some(if *is_error {
            error.as_ref().clone()
        } else {
            result.as_ref().clone()
        }),
        InstructionKind::MakeErrorUnion { value, is_error } => {
            if value_type(f, *value)?
                != if *is_error {
                    error.as_ref()
                } else {
                    result.as_ref()
                }
            {
                return Err(failure("dyn union constructor changes actual arm type"));
            }
            Some(ty.clone())
        }
        InstructionKind::DropErrorUnion { .. } => None,
        _ => unreachable!(),
    };
    if i.result.and_then(|v| f.values.get(v.0 as usize)) != expected.as_ref() {
        return Err(failure(
            "dyn union instruction changes complete selected type",
        ));
    }
    Ok(true)
}
pub(super) fn operation(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::MakeDynOwned { .. }
            | InstructionKind::MakeDynBorrow { .. }
            | InstructionKind::BorrowDynOwned { .. }
            | InstructionKind::CallDynSlot { .. }
            | InstructionKind::DropDynOwned { .. }
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    if !instruction(module, f, i, b)? {
        return Ok(None);
    }
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("dyn actual value is absent"))
    };
    let mut out = Value::default();
    match &i.kind {
        InstructionKind::MakeDynOwned {
            interface,
            vtable,
            source,
            ..
        }
        | InstructionKind::MakeDynBorrow {
            interface,
            vtable,
            source,
            ..
        } => {
            fee(
                b,
                module
                    .dyn_vtables
                    .len()
                    .saturating_add(module.dyn_interfaces.len()),
            )?;
            let table = module
                .dyn_vtables
                .iter()
                .find(|table| table.id == *vtable && table.interface == *interface)
                .ok_or_else(|| failure("dyn constructor has no exact selected provider table"))?;
            let item = module
                .dyn_interfaces
                .iter()
                .find(|item| item.id == *interface)
                .ok_or_else(|| failure("dyn constructor interface is absent"))?;
            let actual = get(*source)?;
            let owned = matches!(i.kind, InstructionKind::MakeDynOwned { .. });
            if !owned {
                union(&mut out.pointer, &actual.pointer, b)?;
            }
            fee(
                b,
                table
                    .ports
                    .len()
                    .saturating_add(item.retained_life.ports.len()),
            )?;
            if table.ports.len() != item.retained_life.ports.len() {
                return Err(failure("dyn constructor omits explicit retained rows"));
            }
            let mut complete = true;
            for (port, row) in table.ports.iter().enumerate() {
                if row.port as usize != port {
                    return Err(failure(
                        "dyn constructor port rows are not complete and canonical",
                    ));
                }
                let mut permission = Permission::default();
                for source in &row.sources {
                    if let Some(actual) = selected(actual, source, b)? {
                        complete &= !actual.targets.is_empty();
                        union(&mut permission, &actual, b)?;
                    } else {
                        complete = false;
                    }
                }
                let prefix = if owned {
                    vec![PlaceProjection::ErrorArm(false)]
                } else {
                    vec![]
                };
                fee(b, prefix.len().saturating_add(1))?;
                out.dyn_ports.insert(key(&prefix, row.port), permission);
            }
            out.creation_complete = Some(complete);
        }
        InstructionKind::BorrowDynOwned { local, .. } => {
            ordered(b, state.locals.len(), 0)?;
            let actual = state
                .locals
                .get(local)
                .ok_or_else(|| failure("dyn owner local is unavailable"))?;
            if matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) {
                // A borrowed pair keeps the actual owner position and its
                // incoming loan ancestry. Ports never replace this head.
                union(&mut out.pointer, &actual.pointer, b)?;
            } else {
                fee(b, 1)?;
                out.pointer.targets.insert(Target {
                    base: HoldRoot::Local(*local),
                    retained_port: None,
                    stored: vec![],
                    place: vec![],
                    heap_parents: Vec::new(),
                    heap_element: None,
                });
            }
            merge(&mut out, actual, b)?;
            out.creation_complete = actual.creation_complete;
        }
        InstructionKind::CallDynSlot {
            callee,
            interface,
            slot,
            ..
        } => {
            fee(b, module.dyn_interfaces.len())?;
            let selected = module
                .dyn_interfaces
                .iter()
                .find(|item| item.id == *interface)
                .and_then(|item| item.slots.get(*slot as usize))
                .ok_or_else(|| failure("dyn slot selection is absent"))?;
            if let Some(DynResultSource::Borrow { sources, .. }) = &selected.contract.result {
                let actual = get(*callee)?;
                let mut complete = true;
                for atom in sources {
                    let source = crate::exact_storage_source(atom)?;
                    fee(b, source.path.len().saturating_add(1))?;
                    if source.base != StorageBase::Parameter(0) {
                        return Err(failure("dyn result source is outside its actual receiver"));
                    }
                    if source.path.is_empty() {
                        union(&mut out.pointer, &actual.pointer, b)?;
                    } else {
                        ordered(b, actual.dyn_ports.len(), source.path.len())?;
                        if let Some(port) = actual.dyn_ports.get(&source.path) {
                            union(&mut out.pointer, port, b)?;
                        } else {
                            complete = false;
                        }
                    }
                }
                out.creation_complete = Some(complete);
            }
        }
        InstructionKind::TakeVariantField { local, path } => {
            let prefix = path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            fee(b, path.len())?;
            ordered(b, state.locals.len(), 0)?;
            let actual = state
                .locals
                .get_mut(local)
                .ok_or_else(|| failure("dyn union local is not initialized"))?;
            projection(&mut out, actual, &prefix, b)?;
            work(actual, b)?;
            actual
                .dyn_ports
                .retain(|path, _| !path.starts_with(&prefix));
        }
        InstructionKind::ReadErrorUnion { value, is_error }
        | InstructionKind::TakeErrorUnion { value, is_error } => projection(
            &mut out,
            get(*value)?,
            &[ContractProjection::Place(PlaceProjection::ErrorArm(
                *is_error,
            ))],
            b,
        )?,
        InstructionKind::MakeErrorUnion { value, is_error } => copy_with_prefix(
            &mut out,
            get(*value)?,
            &[ContractProjection::Place(PlaceProjection::ErrorArm(
                *is_error,
            ))],
            b,
        )?,
        InstructionKind::PlaceTagLocal { .. } | InstructionKind::ErrorTag { .. } => {}
        InstructionKind::DropErrorUnion { local } | InstructionKind::DropDynOwned { local } => {
            ordered(b, state.locals.len(), 0)?;
            state.locals.remove(local);
        }
        _ => unreachable!(),
    }
    Ok(Some(out))
}
/// Enrich ordinary moves/addresses with the same actual port facts. This is
/// additive: it never replaces a manager address, head loan or physical slot.
pub(super) fn propagate(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    out: &mut Value,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("dyn transport input is absent"))
    };
    match &i.kind {
        InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
            ordered(b, state.locals.len(), 0)?;
            if let Some(actual) = state.locals.get(local) {
                merge(out, actual, b)?;
            }
        }
        InstructionKind::ReborrowAddress { address, .. } => {
            merge(out, get(*address)?, b)?;
        }
        InstructionKind::AddressField { address, index } => projection(
            out,
            get(*address)?,
            &[ContractProjection::Place(PlaceProjection::Field(*index))],
            b,
        )?,
        InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakePackage { fields }
        | InstructionKind::MakeOwnedTuple { fields } => {
            for (index, value) in fields.iter().enumerate() {
                copy_with_prefix(
                    out,
                    get(*value)?,
                    &[ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    ))],
                    b,
                )?;
            }
        }
        InstructionKind::TakePackageField { local, index } => {
            ordered(b, state.locals.len(), 0)?;
            if let Some(actual) = state.locals.get_mut(local) {
                let prefix = [ContractProjection::Place(PlaceProjection::Field(*index))];
                projection(out, actual, &prefix, b)?;
                work(actual, b)?;
                actual
                    .dyn_ports
                    .retain(|path, _| !path.starts_with(&prefix));
            }
        }
        InstructionKind::DropPackageField { local, index } => {
            ordered(b, state.locals.len(), 0)?;
            if let Some(actual) = state.locals.get_mut(local) {
                let prefix = ContractProjection::Place(PlaceProjection::Field(*index));
                work(actual, b)?;
                actual
                    .dyn_ports
                    .retain(|path, _| path.first() != Some(&prefix));
            }
        }
        InstructionKind::MakeClosure { captures, .. } => {
            for capture in captures {
                let mut directory_rows = Vec::new();
                directory(
                    module,
                    value_type(f, *capture)?,
                    &mut vec![],
                    &mut directory_rows,
                    b,
                )?;
                if !directory_rows.is_empty() || !get(*capture)?.dyn_ports.is_empty() {
                    return Err(failure(
                        "captured dyn Context needs complete environment source transport",
                    ));
                }
            }
        }
        // Every other operation leaves the new namespace alone. Its ordinary
        // consumer still validates actual types and any owner responsibility.
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn substitution_separates_same_numbered_task_and_dyn_ports_and_keeps_head_parents() {
        let mut b = Budget::new(&Limits::default());
        b.dyn_contexts = true;
        b.task_borrowed_results = true;
        let target = |base, origin| Target {
            base,
            retained_port: origin,
            stored: vec![],
            place: vec![],
            heap_parents: Vec::new(),
            heap_element: None,
        };
        let mut value = Value::default();
        value.pointer.loans.insert(77);
        value.task_ports.insert(
            key(&[], 0),
            Permission {
                targets: Targets::from([target(HoldRoot::Local(LocalId(1)), None)]),
                loans: Loans::from([11]),
            },
        );
        value.dyn_ports.insert(
            key(&[], 0),
            Permission {
                targets: Targets::from([target(HoldRoot::Local(LocalId(2)), None)]),
                loans: Loans::from([22]),
            },
        );
        let capture_key = vec![
            ContractProjection::Capture(2),
            ContractProjection::RetainedPort(0),
        ];
        value.dyn_ports.insert(
            capture_key,
            Permission {
                targets: Targets::from([target(HoldRoot::Local(LocalId(3)), None)]),
                loans: Loans::from([33]),
            },
        );
        let values = vec![value];
        let arguments = vec![ValueId(0)];
        let state = State::default();
        let dyn_source = target(
            HoldRoot::Parameter(0),
            Some(PortOrigin::Dyn {
                carrier: vec![],
                port: 0,
            }),
        );
        let task_source = target(HoldRoot::Parameter(0), Some(PortOrigin::Task(0)));
        let dyn_actual = substitute(&dyn_source, &arguments, &values, &state, &mut b).unwrap();
        let task_actual = substitute(&task_source, &arguments, &values, &state, &mut b).unwrap();
        assert!(
            dyn_actual
                .targets
                .contains(&target(HoldRoot::Local(LocalId(2)), None))
        );
        assert!(
            !dyn_actual
                .targets
                .contains(&target(HoldRoot::Local(LocalId(1)), None))
        );
        assert_eq!(dyn_actual.loans, Loans::from([22, 77]));
        assert_ne!(dyn_actual.targets, task_actual.targets);
        let nested = target(
            HoldRoot::Parameter(0),
            Some(PortOrigin::Dyn {
                carrier: vec![ContractProjection::Capture(2)],
                port: 0,
            }),
        );
        let nested_actual = substitute(&nested, &arguments, &values, &state, &mut b).unwrap();
        assert!(
            nested_actual
                .targets
                .contains(&target(HoldRoot::Local(LocalId(3)), None))
        );
        assert_eq!(nested_actual.loans, Loans::from([33, 77]));

        let missing = target(
            HoldRoot::Parameter(0),
            Some(PortOrigin::Dyn {
                carrier: vec![],
                port: 1,
            }),
        );
        assert!(substitute(&missing, &arguments, &values, &state, &mut b).is_err());
        let mut explicit_empty = values[0].clone();
        explicit_empty
            .dyn_ports
            .insert(key(&[], 1), Permission::default());
        let known = substitute(&missing, &arguments, &[explicit_empty], &state, &mut b).unwrap();
        assert!(known.targets.is_empty());
        assert_eq!(known.loans, Loans::from([77]));
    }
}

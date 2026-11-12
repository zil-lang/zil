//! Physical identity only. Constructor/projection CFG transfer separately binds
//! every actual permission, parent loan and phase; these rows create no target.
use super::*;
fn reason(t: &Type) -> bool {
    matches!(t,Type::Enum{name,variants} if name=="@std.task:v1:TaskCreateError" && matches!(variants.as_slice(),[(allocation,fields)] if allocation=="Allocation" && fields.is_empty()))
}
pub(super) fn declaration(d: &LifeNominalDeclaration) -> bool {
    let kind = match d.id {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD => LifeNominalKind::Record,
        zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE => LifeNominalKind::Package,
        _ => return false,
    };
    d.kind == kind
        && d.parameters == [LifeParameterKind::Type, LifeParameterKind::Type]
        && d.identity.package == "@zil.builtin"
        && d.identity.module_path == ["task"]
        && d.identity.declaration_path
            == [
                if kind == LifeNominalKind::Record {
                    "record"
                } else {
                    "package"
                },
                "TaskCreateFailure",
            ]
        && d.identity.generation_path.is_empty()
}
pub(super) fn shape(t: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Some((storage, life)) = carrier(t) else {
        return Ok(false);
    };
    let (inputs, outer) = match storage {
        Type::Record { name, fields } | Type::Package { name, fields }
            if name.starts_with("@std.task:v1:TaskCreateFailure:") =>
        {
            fee(b, name.len().saturating_add(fields.len()))?;
            let [(r, ty), (i, Type::Tuple(inputs))] = fields.as_slice() else {
                return Err(failure("failure carrier has no ordered input Tuple"));
            };
            if r != "reason" || i != "inputs" || !reason(ty) {
                return Err(failure("failure carrier reason/input selectors differ"));
            }
            b.type_work(&fields[1].1, true, "failure complete input identity")?;
            if *name
                != format!(
                    "@std.task:v1:TaskCreateFailure:{}",
                    fields[1].1.fingerprint()
                )
            {
                return Err(failure("failure complete input identity differs"));
            }
            if inputs.iter().any(|t| {
                !t.is_copy_value()
                    && !matches!(t, Type::Address { .. })
                    && !matches!(t, Type::Managed(payload) if payload.is_copy_value())
            }) {
                return Err(failure(
                    "mixed owning failure view requires its complete per-leaf responsibility consumer",
                ));
            }
            let owner = inputs.iter().any(|t| matches!(t, Type::Managed(_)));
            if matches!(storage, Type::Package { .. }) != owner {
                return Err(failure(
                    "failure carrier shell must match its actual input responsibilities",
                ));
            }
            (inputs, true)
        }
        Type::Tuple(inputs) => (inputs, false),
        _ => return Ok(false),
    };
    if inputs.iter().any(|t| {
        !t.is_copy_value()
            && !matches!(t, Type::Address { .. })
            && !matches!(t, Type::Managed(payload) if payload.is_copy_value())
    }) {
        return Err(failure(
            "mixed owning input view requires its complete per-leaf responsibility consumer",
        ));
    }
    fee(
        b,
        inputs
            .len()
            .saturating_add(life.slots.len())
            .saturating_add(life.retained.len())
            .saturating_add(life.nominal_arguments.len()),
    )?;
    if life.slots.is_empty() || !life.nominal_arguments.is_empty() {
        return Err(failure("failure/view class domain lacks its actual slots"));
    }
    let mut slot = 0;
    let mut classes = BTreeSet::new();
    let mut expected = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        fee(b, 1)?;
        if let Type::Address { mutable, inner } = input {
            if !inner.is_address_pointee() {
                return Err(failure("failure view address pointee is invalid"));
            }
            let row = life
                .slots
                .get(slot)
                .ok_or_else(|| failure("failure input slot omitted"))?;
            let path = if outer {
                vec![
                    PlaceProjection::Field(1),
                    PlaceProjection::Field(index as u32),
                ]
            } else {
                vec![PlaceProjection::Field(index as u32)]
            };
            let mode = if *mutable {
                StoredBorrowMode::ExclusiveAddress
            } else {
                StoredBorrowMode::SharedAddress
            };
            fee(
                b,
                path.len()
                    .saturating_add(row.path.len())
                    .saturating_add(classes.len()),
            )?;
            if row.path != path || row.mode != mode || row.binder >= life.binder_count {
                return Err(failure("failure input slot path/mode/class differs"));
            }
            if !classes.contains(&row.binder) && row.binder as usize != classes.len() {
                return Err(failure(
                    "failure view classes are not dense by actual first occurrence",
                ));
            }
            classes.insert(row.binder);
            if outer {
                expected.push(zil_ir::StorageRetainedBinding {
                    path: vec![
                        zil_ir::LifeTypeProjection::TypeArgument(0),
                        zil_ir::LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),
                    ],
                    class: 0,
                    binder: row.binder,
                });
            }
            slot += 1;
        }
    }
    if slot != life.slots.len() || classes.len() != life.binder_count as usize {
        return Err(failure(
            "failure/view has an extra slot or ungrounded class",
        ));
    }
    if outer {
        for class in 0..life.binder_count {
            fee(b, 1)?;
            expected.push(zil_ir::StorageRetainedBinding {
                path: vec![zil_ir::LifeTypeProjection::TypeArgument(1)],
                class,
                binder: class,
            });
        }
    }
    if life.retained != expected {
        return Err(failure("failure/view retained actual class map differs"));
    }
    Ok(true)
}
pub(super) fn instance(i: &LifeNominalInstance, b: &mut Budget) -> CompileResult<()> {
    if !shape(&i.ty, b)? {
        return Err(failure(
            "failure nominal instance lacks its complete physical carrier",
        ));
    }
    let Type::BorrowedStorage { storage, life } = &i.ty else {
        unreachable!()
    };
    let (Type::Record { fields, .. } | Type::Package { fields, .. }) = storage.as_ref() else {
        return Err(failure(
            "anonymous view must not invent a nominal declaration",
        ));
    };
    let actual_declaration = if matches!(storage.as_ref(), Type::Package { .. }) {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
    } else {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
    };
    if i.declaration != actual_declaration {
        return Err(failure(
            "failure nominal declaration kind differs from its actual shell",
        ));
    }
    let [
        LifeNominalActual::Type(inputs),
        LifeNominalActual::Type(Type::Task {
            result,
            retained_life: context,
        }),
    ] = i.arguments.as_slice()
    else {
        return Err(failure(
            "failure nominal instance omitted ordered Inputs/Context actuals",
        ));
    };
    b.type_work(inputs, true, "failure nominal input equality")?;
    if inputs != &fields[1].1
        || result.as_ref() != &Type::Unit
        || context.binder_count != life.binder_count
        || !context.retained.is_empty()
        || !context.nominal_arguments.is_empty()
    {
        return Err(failure("failure nominal Inputs/Context differs"));
    }
    let Type::Tuple(inputs) = inputs else {
        unreachable!()
    };
    let mut index = 0;
    for input in inputs {
        if let Type::Address { .. } = input {
            fee(b, 1)?;
            let p = context
                .ports
                .get(index)
                .ok_or_else(|| failure("failure context omitted an input port"))?;
            b.type_work(&p.leaf, true, "failure actual context leaf equality")?;
            let slot = &life.slots[index];
            if p.leaf != *input || p.mode != slot.mode || p.binder != slot.binder {
                return Err(failure("failure actual context port differs"));
            }
            index += 1;
        }
    }
    if index != context.ports.len() {
        return Err(failure("failure context has an extra retained port"));
    }
    Ok(())
}

pub(super) fn projection(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<Option<Type>> {
    if !b.task_failure_carriers {
        return Ok(None);
    }
    let selected = [PlaceProjection::Field(1)];
    let (inner, path): (&Type, &[PlaceProjection]) = match &i.kind {
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::Take,
        } if path.as_slice() == selected => {
            let Type::Address {
                mutable: true,
                inner,
            } = value_type(f, *storage)?
            else {
                return Ok(None);
            };
            (inner, path)
        }
        InstructionKind::TakePackageField { local, index: 1 } => {
            let Some(parent) = f.locals.get(local.0 as usize).map(|l| &l.ty) else {
                return Ok(None);
            };
            if !matches!(parent, Type::BorrowedStorage {storage,..} if matches!(storage.as_ref(),Type::Package{..}))
            {
                return Ok(None);
            }
            (parent, &selected)
        }
        _ => return Ok(None),
    };
    let Type::BorrowedStorage { storage: raw, life } = inner else {
        return Ok(None);
    };
    let (Type::Record { name, fields } | Type::Package { name, fields }) = raw.as_ref() else {
        return Ok(None);
    };
    if !name.starts_with("@std.task:v1:TaskCreateFailure:") {
        return Ok(None);
    }
    if !shape(inner, b)? {
        return Err(failure("failure input projection parent is not complete"));
    }
    b.type_work(inner, true, "failure selected child clone and class map")?;
    let mut classes = BTreeMap::new();
    let mut selected = Vec::new();
    for row in &life.slots {
        fee(
            b,
            row.path
                .len()
                .saturating_add(classes.len())
                .saturating_add(2),
        )?;
        let tail = row
            .path
            .strip_prefix(path)
            .ok_or_else(|| failure("failure input projection omits a parent slot"))?;
        let next = classes.len() as u32;
        let binder = *classes.entry(row.binder).or_insert(next);
        selected.push(StorageLifeSlot {
            path: tail.to_vec(),
            binder,
            mode: row.mode,
        });
    }
    Ok(Some(Type::BorrowedStorage {
        storage: Box::new(fields[1].1.clone()),
        life: StorageLifeShape {
            binder_count: classes.len() as u32,
            slots: selected,
            retained: vec![],
            nominal_arguments: vec![],
        },
    }))
}

pub(super) fn create(
    f: &TypedFunction,
    i: &Instruction,
    arguments: &[ValueId],
    values: &[Value],
    out: &mut Value,
    b: &mut Budget,
) -> CompileResult<()> {
    let Some(Type::ErrorUnion { error, .. }) = i.result.and_then(|v| f.values.get(v.0 as usize))
    else {
        return Ok(());
    };
    if !matches!(error.as_ref(),Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{name,..}|Type::Package{name,..}if name.starts_with("@std.task:v1:TaskCreateFailure:")))
    {
        return Ok(());
    }
    if !shape(error, b)? {
        return Err(failure(
            "owned failure constructor has no complete input carrier",
        ));
    }
    for (index, input) in arguments.iter().enumerate() {
        fee(b, 1)?;
        if matches!(value_type(f, *input)?, Type::Address { .. }) {
            let path = vec![
                PlaceProjection::ErrorArm(true),
                PlaceProjection::Field(1),
                PlaceProjection::Field(index as u32),
            ];
            let original = values
                .get(input.0 as usize)
                .ok_or_else(|| failure("owned failure original input is absent"))?;
            ordered(b, out.slots.len(), path.len())?;
            union(out.slots.entry(path).or_default(), &original.pointer, b)?;
        }
    }
    Ok(())
}

pub(super) fn take_view(
    state: &mut State,
    address: &Permission,
    parent: &StorageLifeShape,
    out: &mut Value,
    b: &mut Budget,
) -> CompileResult<()> {
    for row in &parent.slots {
        fee(b, row.path.len().saturating_add(2))?;
        let Some(child) = row.path.strip_prefix(&[PlaceProjection::Field(1)]) else {
            return Err(failure("owned failure view omitted an original input slot"));
        };
        let permission = select(state, address, &row.path, b)?;
        ordered(b, out.slots.len(), child.len())?;
        union(out.slots.entry(child.to_vec()).or_default(), &permission, b)?;
        if !address.targets.is_empty() {
            let position = slot_position(address, &row.path, b)?;
            match position.base {
                HoldRoot::Local(local) => {
                    if let Some(holder) = state.locals.get_mut(&local) {
                        holder.slots.remove(&position.place);
                    }
                }
                HoldRoot::Parameter(_) | HoldRoot::Capture(_) => {
                    if let Some(holder) = state.external.get_mut(&position.base) {
                        holder.remove(&position.place);
                    }
                }
                HoldRoot::Static => {
                    return Err(failure("static storage cannot own a failure input Tuple"));
                }
            }
            ordered(b, state.vacant.len(), target_width(&position))?;
            state.vacant.insert(position);
        }
    }
    Ok(())
}

fn error_carrier(t: &Type) -> bool {
    matches!(t,Type::ErrorUnion{error,..}if matches!(error.as_ref(),Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{name,..}|Type::Package{name,..}if name.starts_with("@std.task:v1:TaskCreateFailure:"))))
}
pub(super) fn wrapper(
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    if !b.task_failure_carriers {
        return Ok(None);
    }
    let mut out = Value::default();
    let prefix = [PlaceProjection::ErrorArm(true)];
    match &i.kind {
        InstructionKind::TakeErrorUnion {
            value,
            is_error: true,
        } if error_carrier(value_type(f, *value)?) => {
            for (path, permission) in &values[value.0 as usize].slots {
                fee(b, path.len().saturating_add(1))?;
                if let Some(child) = path.strip_prefix(&prefix) {
                    union(out.slots.entry(child.to_vec()).or_default(), permission, b)?;
                }
            }
        }
        InstructionKind::TakeVariantField { local, path }
            if path.as_slice() == prefix && error_carrier(&f.locals[local.0 as usize].ty) =>
        {
            if let Some(parent) = state.locals.get_mut(local) {
                fee(b, parent.slots.len())?;
                for (path, p) in &parent.slots {
                    fee(b, path.len().saturating_add(1))?;
                    if let Some(child) = path.strip_prefix(&prefix) {
                        union(out.slots.entry(child.to_vec()).or_default(), p, b)?;
                    }
                }
                parent.slots.retain(|path, _| !path.starts_with(&prefix));
            }
        }
        InstructionKind::MakeErrorUnion {
            value,
            is_error: true,
        } if i
            .result
            .is_some_and(|r| error_carrier(&f.values[r.0 as usize])) =>
        {
            for (path, p) in &values[value.0 as usize].slots {
                fee(b, path.len().saturating_add(2))?;
                let mut parent = prefix.to_vec();
                parent.extend(path);
                union(out.slots.entry(parent).or_default(), p, b)?;
            }
        }
        InstructionKind::PlaceTagLocal { local, path }
            if path.is_empty() && error_carrier(&f.locals[local.0 as usize].ty) => {}
        InstructionKind::ErrorTag { value } if error_carrier(value_type(f, *value)?) => {}
        _ => return Ok(None),
    }
    Ok(Some(out))
}

pub(super) fn unpack(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> CompileResult<bool> {
    if !b.task_failure_carriers {
        return Ok(false);
    }
    let InstructionKind::UnpackTuple { local, fields } = &i.kind else {
        return Ok(false);
    };
    let Some(ty) = f.locals.get(local.0 as usize).map(|l| &l.ty) else {
        return Err(failure("view unpack source is absent"));
    };
    let Type::BorrowedStorage { storage, .. } = ty else {
        return Ok(false);
    };
    let Type::Tuple(types) = storage.as_ref() else {
        return Ok(false);
    };
    if !shape(ty, b)? {
        return Err(failure("view unpack has no complete physical tuple"));
    }
    fee(
        b,
        fields.len().saturating_mul(fields.len().saturating_add(2)),
    )?;
    if fields.len() != types.len() || i.result.is_some() {
        return Err(failure("view unpack arity/result differs"));
    }
    let mut seen = BTreeSet::new();
    for (target, t) in fields.iter().zip(types) {
        if *target == *local
            || !seen.insert(*target)
            || f.locals.get(target.0 as usize).is_none_or(|l| &l.ty != t)
        {
            return Err(failure(
                "view unpack must move exact distinct input positions",
            ));
        }
    }
    Ok(true)
}
pub(super) fn unpack_value(
    f: &TypedFunction,
    local: LocalId,
    fields: &[LocalId],
    state: &mut State,
    b: &mut Budget,
) -> CompileResult<()> {
    let Type::BorrowedStorage { storage, .. } = &f.locals[local.0 as usize].ty else {
        unreachable!()
    };
    let Type::Tuple(types) = storage.as_ref() else {
        unreachable!()
    };
    let mut targets = Vec::new();
    for (index, _) in types.iter().enumerate() {
        fee(b, 1)?;
        let mut value = Value::default();
        if let Some(parent) = state.locals.get(&local)
            && let Some(permission) = parent
                .slots
                .get(&vec![PlaceProjection::Field(index as u32)])
        {
            union(&mut value.pointer, permission, b)?;
        }
        targets.push(value);
    }
    state.locals.remove(&local);
    for (target, value) in fields.iter().zip(targets) {
        ordered(b, state.locals.len(), 0)?;
        state.locals.insert(*target, value);
    }
    Ok(())
}

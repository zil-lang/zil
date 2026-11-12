//! Verifier-local physical and nominal failure validation. No producer class,
//! marker or table authorizes a target; executable transfers supply those.
use super::*;
pub(super) fn declaration(d: &LifeNominalDeclaration) -> bool {
    let (id, kind, word) = match d.kind {
        LifeNominalKind::Record => (
            zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD,
            LifeNominalKind::Record,
            "record",
        ),
        LifeNominalKind::Package => (
            zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE,
            LifeNominalKind::Package,
            "package",
        ),
        _ => return false,
    };
    d.id == id
        && d.kind == kind
        && d.parameters.as_slice() == [LifeParameterKind::Type, LifeParameterKind::Type]
        && d.identity.package == "@zil.builtin"
        && d.identity.module_path.as_slice() == ["task"]
        && d.identity.declaration_path.as_slice() == [word, "TaskCreateFailure"]
        && d.identity.generation_path.is_empty()
}
fn complete_inputs(inputs: &[Type], b: &mut Budget) -> Check<bool> {
    for t in inputs {
        if t.is_copy_value()
            || matches!(t, Type::Address { .. })
            || matches!(t,Type::Managed(payload) if payload.is_copy_value())
        {
            continue;
        }
        if !matches!(t, Type::Managed(_))
            || crate::tasks::input_scope::domain(t, b)?
                != crate::tasks::input_scope::Domain::Known(0)
        {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn shape(t: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    let Some((raw, classes)) = qualified(t) else {
        return Ok(false);
    };
    let (elements, whole) = match raw {
        Type::Tuple(elements) => (elements, false),
        Type::Record { name, fields } | Type::Package { name, fields }
            if name.starts_with("@std.task:v1:TaskCreateFailure:") =>
        {
            b.spend(name.len().saturating_add(fields.len()))?;
            let [
                (
                    reason,
                    Type::Enum {
                        name: error_name,
                        variants,
                    },
                ),
                (inputs, Type::Tuple(elements)),
            ] = fields.as_slice()
            else {
                return Err(err("Task failure physical fields differ"));
            };
            if reason != "reason"
                || inputs != "inputs"
                || error_name != "@std.task:v1:TaskCreateError"
                || !matches!(variants.as_slice(),[(name,fields)]if name=="Allocation"&&fields.is_empty())
            {
                return Err(err("Task failure physical reason or input labels differ"));
            }
            type_work(&fields[1].1, limits, b)?;
            if name
                != &format!(
                    "@std.task:v1:TaskCreateFailure:{}",
                    fields[1].1.fingerprint()
                )
            {
                return Err(err("Task failure input identity differs"));
            }
            if !complete_inputs(elements, b)? {
                return Err(err(
                    "mixed owning failure view lacks its complete responsibility consumer",
                ));
            }
            let needs_owned_shell = elements.iter().any(|element| {
                !element.is_copy_value() && !matches!(element, Type::Address { .. })
            });
            if matches!(raw, Type::Record { .. }) == needs_owned_shell {
                return Err(err(
                    "failure carrier shell must match its actual input responsibilities",
                ));
            }
            (elements, true)
        }
        _ => return Ok(false),
    };
    if !complete_inputs(elements, b)? {
        return Err(err(
            "mixed owning input view lacks its complete responsibility consumer",
        ));
    }
    b.spend(
        elements
            .len()
            .saturating_add(classes.slots.len())
            .saturating_add(classes.retained.len())
            .saturating_add(classes.nominal_arguments.len()),
    )?;
    if classes.slots.is_empty() || !classes.nominal_arguments.is_empty() {
        return Err(err(
            "Task failure/view has no physical slots or forged nominal classes",
        ));
    }
    let mut cursor = 0usize;
    let mut ordering = Vec::new();
    let mut retained = 0usize;
    for (position, element) in elements.iter().enumerate() {
        b.spend(1)?;
        if let Type::Address { mutable, inner } = element {
            if !inner.is_address_pointee() {
                return Err(err("Task failure input has invalid pointee"));
            }
            let slot = classes
                .slots
                .get(cursor)
                .ok_or_else(|| err("Task failure input slot is missing"))?;
            let exact = if whole {
                slot.path.as_slice()
                    == [
                        PlaceProjection::Field(1),
                        PlaceProjection::Field(position as u32),
                    ]
            } else {
                slot.path.as_slice() == [PlaceProjection::Field(position as u32)]
            };
            b.spend(
                slot.path
                    .len()
                    .saturating_add(ordering.len())
                    .saturating_add(1),
            )?;
            let mode = if *mutable {
                StoredBorrowMode::ExclusiveAddress
            } else {
                StoredBorrowMode::SharedAddress
            };
            if !exact || slot.mode != mode || slot.binder >= classes.binder_count {
                return Err(err("Task failure slot has a different path, mode or class"));
            }
            if !ordering.contains(&slot.binder) {
                if slot.binder as usize != ordering.len() {
                    return Err(err(
                        "Task failure view did not alpha-normalize its class order",
                    ));
                }
                ordering.push(slot.binder);
            }
            if whole {
                let row = classes
                    .retained
                    .get(retained)
                    .ok_or_else(|| err("Task failure input class binding is missing"))?;
                b.spend(row.path.len().saturating_add(1))?;
                if row.path.as_slice()
                    != [
                        zil_ir::LifeTypeProjection::TypeArgument(0),
                        zil_ir::LifeTypeProjection::Place(PlaceProjection::Field(position as u32)),
                    ]
                    || row.class != 0
                    || row.binder != slot.binder
                {
                    return Err(err("Task failure input class binding differs"));
                }
                retained += 1;
            }
            cursor += 1;
        }
    }
    if cursor != classes.slots.len() || ordering.len() != classes.binder_count as usize {
        return Err(err("Task failure/view has excess slots or classes"));
    }
    if whole {
        for class in 0..classes.binder_count {
            let row = classes
                .retained
                .get(retained)
                .ok_or_else(|| err("Task failure context class binding is missing"))?;
            b.spend(row.path.len().saturating_add(1))?;
            if row.path.as_slice() != [zil_ir::LifeTypeProjection::TypeArgument(1)]
                || row.class != class
                || row.binder != class
            {
                return Err(err("Task failure actual context class binding differs"));
            }
            retained += 1;
        }
    }
    if retained != classes.retained.len() {
        return Err(err("Task failure/view retained rows are not exact"));
    }
    Ok(true)
}
pub(super) fn instance(i: &LifeNominalInstance, limits: &Limits, b: &mut Budget) -> Check<()> {
    if !shape(&i.ty, limits, b)? {
        return Err(err("Task failure nominal has no complete physical carrier"));
    }
    let Type::BorrowedStorage { storage, life } = &i.ty else {
        unreachable!()
    };
    let (Type::Record { fields, .. } | Type::Package { fields, .. }) = storage.as_ref() else {
        return Err(err(
            "anonymous tuple view may not invent a nominal declaration",
        ));
    };
    let actual_declaration = if matches!(storage.as_ref(), Type::Package { .. }) {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
    } else {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
    };
    if i.declaration != actual_declaration {
        return Err(err(
            "failure nominal declaration kind differs from its actual shell",
        ));
    }
    let [
        LifeNominalActual::Type(inputs),
        LifeNominalActual::Type(context),
    ] = i.arguments.as_slice()
    else {
        return Err(err(
            "Task failure nominal has no exact Inputs and Context actuals",
        ));
    };
    type_work(inputs, limits, b)?;
    type_work(context, limits, b)?;
    let Type::Task {
        result,
        retained_life,
    } = context
    else {
        return Err(err("Task failure context actual differs"));
    };
    if inputs != &fields[1].1
        || result.as_ref() != &Type::Unit
        || retained_life.binder_count != life.binder_count
        || !retained_life.retained.is_empty()
        || !retained_life.nominal_arguments.is_empty()
    {
        return Err(err("Task failure ordered input/context identity differs"));
    }
    let Type::Tuple(inputs) = inputs else {
        unreachable!()
    };
    let leaves = inputs.iter().filter(|t| matches!(t, Type::Address { .. }));
    b.spend(inputs.len().saturating_add(retained_life.ports.len()))?;
    if leaves.clone().count() != retained_life.ports.len() {
        return Err(err("Task failure actual context port count differs"));
    }
    for ((input, port), slot) in leaves.zip(&retained_life.ports).zip(&life.slots) {
        type_work(&port.leaf, limits, b)?;
        if input != &port.leaf || port.mode != slot.mode || port.binder != slot.binder {
            return Err(err("Task failure actual context port leaf/class differs"));
        }
    }
    Ok(())
}

pub(super) fn projection(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Option<Type>> {
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
            } = vt(f, *storage)?
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
    let Some((physical, parent)) = qualified(inner) else {
        return Ok(None);
    };
    let (Type::Record { name, fields } | Type::Package { name, fields }) = physical else {
        return Ok(None);
    };
    if !name.starts_with("@std.task:v1:TaskCreateFailure:") {
        return Ok(None);
    }
    let limits = b.limits.clone();
    if !shape(inner, &limits, b)? {
        return Err(err("failure input view has no complete parent"));
    }
    type_work(inner, &limits, b)?;
    let mut originals = Vec::new();
    let mut rows = Vec::new();
    for slot in &parent.slots {
        b.spend(
            slot.path
                .len()
                .saturating_add(originals.len())
                .saturating_add(2),
        )?;
        let Some(rest) = slot.path.strip_prefix(path) else {
            return Err(err("failure input view dropped a parent physical slot"));
        };
        let class = if let Some(index) = originals.iter().position(|old| old == &slot.binder) {
            index
        } else {
            let index = originals.len();
            originals.push(slot.binder);
            index
        };
        rows.push(StorageLifeSlot {
            path: rest.to_vec(),
            mode: slot.mode,
            binder: class as u32,
        });
    }
    Ok(Some(Type::BorrowedStorage {
        storage: Box::new(fields[1].1.clone()),
        life: StorageLifeShape {
            binder_count: originals.len() as u32,
            slots: rows,
            retained: vec![],
            nominal_arguments: vec![],
        },
    }))
}

pub(super) fn create(
    f: &TypedFunction,
    i: &Instruction,
    arguments: &[ValueId],
    values: &[Payload],
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    let Some(Type::ErrorUnion { error, .. }) = i.result.and_then(|v| f.values.get(v.0 as usize))
    else {
        return Ok(());
    };
    let Some((Type::Record { name, .. } | Type::Package { name, .. }, _)) = qualified(error) else {
        return Ok(());
    };
    if !name.starts_with("@std.task:v1:TaskCreateFailure:") {
        return Ok(());
    }
    let limits = b.limits.clone();
    if !shape(error, &limits, b)? {
        return Err(err("owned failure constructor input carrier differs"));
    }
    for (ordinal, argument) in arguments.iter().enumerate() {
        b.spend(1)?;
        if matches!(vt(f, *argument)?, Type::Address { .. }) {
            let original = values
                .get(argument.0 as usize)
                .ok_or_else(|| err("owned failure original input is absent"))?;
            add_leaf(
                out,
                &[
                    PlaceProjection::ErrorArm(true),
                    PlaceProjection::Field(1),
                    PlaceProjection::Field(ordinal as u32),
                ],
                &original.address,
                b,
            )?;
        }
    }
    Ok(())
}
pub(super) fn take_view(
    state: &mut Frame,
    address: &Permission,
    shape: &StorageLifeShape,
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    for slot in &shape.slots {
        b.spend(slot.path.len().saturating_add(1))?;
        let Some(child) = slot.path.strip_prefix(&[PlaceProjection::Field(1)]) else {
            return Err(err("failure child view omitted a physical parent input"));
        };
        let selected = select(state, address, &slot.path, b)?;
        add_leaf(out, child, &selected, b)?;
        if address.sources.is_empty() {
            continue;
        }
        let position = slot_position(address, &slot.path, b)?;
        let holder = match position.base {
            HoldRoot::Local(l) => state.locals.get_mut(l.0 as usize),
            HoldRoot::Parameter(p) => state.external.get_mut(p as usize),
            HoldRoot::Capture(c) => state.captures.get_mut(c as usize),
            HoldRoot::Static => None,
        };
        if let Some(holder) = holder {
            b.spend(holder.leaves.len())?;
            holder
                .leaves
                .retain(|(path, _)| path != &position.inline, b)?;
        }
        add_sources(&mut state.vacant, &[position], b)?;
    }
    Ok(())
}

fn error_carrier(t: &Type) -> bool {
    matches!(t,Type::ErrorUnion{error,..}if matches!(error.as_ref(),Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{name,..}|Type::Package{name,..}if name.starts_with("@std.task:v1:TaskCreateFailure:"))))
}
pub(super) fn wrapper(
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Option<Payload>> {
    if !b.task_failure_carriers {
        return Ok(None);
    }
    let mut out = Payload::default();
    let selected = [PlaceProjection::ErrorArm(true)];
    match &i.kind {
        InstructionKind::TakeErrorUnion {
            value,
            is_error: true,
        } if error_carrier(vt(f, *value)?) => {
            for (path, p) in &values[value.0 as usize].leaves {
                b.spend(path.len().saturating_add(1))?;
                if let Some(child) = path.strip_prefix(&selected) {
                    add_leaf(&mut out, child, p, b)?;
                }
            }
        }
        InstructionKind::TakeVariantField { local, path }
            if path.as_slice() == selected && error_carrier(&f.locals[local.0 as usize].ty) =>
        {
            let parent = state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| err("failure carrier local is absent"))?;
            b.spend(parent.leaves.len())?;
            for (path, p) in &parent.leaves {
                b.spend(path.len().saturating_add(1))?;
                if let Some(child) = path.strip_prefix(&selected) {
                    add_leaf(&mut out, child, p, b)?;
                }
            }
            parent
                .leaves
                .retain(|(path, _)| !path.starts_with(&selected), b)?;
        }
        InstructionKind::MakeErrorUnion {
            value,
            is_error: true,
        } if i
            .result
            .is_some_and(|r| error_carrier(&f.values[r.0 as usize])) =>
        {
            for (path, p) in &values[value.0 as usize].leaves {
                b.spend(path.len().saturating_add(2))?;
                let mut combined = selected.to_vec();
                combined.extend(path);
                add_leaf(&mut out, &combined, p, b)?;
            }
        }
        InstructionKind::PlaceTagLocal { local, path }
            if path.is_empty() && error_carrier(&f.locals[local.0 as usize].ty) => {}
        InstructionKind::ErrorTag { value } if error_carrier(vt(f, *value)?) => {}
        _ => return Ok(None),
    }
    Ok(Some(out))
}

pub(super) fn unpack(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<bool> {
    if !b.task_failure_carriers {
        return Ok(false);
    }
    let InstructionKind::UnpackTuple { local, fields } = &i.kind else {
        return Ok(false);
    };
    let Some(Type::BorrowedStorage { storage, .. }) = f.locals.get(local.0 as usize).map(|l| &l.ty)
    else {
        return Ok(false);
    };
    let Type::Tuple(types) = storage.as_ref() else {
        return Ok(false);
    };
    let limits = b.limits.clone();
    if !shape(&f.locals[local.0 as usize].ty, &limits, b)? {
        return Err(err("tuple view unpack physical shape differs"));
    }
    b.spend(fields.len().saturating_mul(fields.len().saturating_add(2)))?;
    if i.result.is_some() || types.len() != fields.len() {
        return Err(err("tuple view unpack must select all input fields"));
    }
    for (index, target) in fields.iter().enumerate() {
        if target == local
            || fields[..index].contains(target)
            || f.locals
                .get(target.0 as usize)
                .is_none_or(|l| l.ty != types[index])
        {
            return Err(err(
                "tuple view unpack target identity or exact field type differs",
            ));
        }
    }
    Ok(true)
}
pub(super) fn unpack_value(
    f: &TypedFunction,
    local: LocalId,
    fields: &[LocalId],
    state: &mut Frame,
    b: &mut Budget,
) -> Check<()> {
    let Type::BorrowedStorage { storage, .. } = &f.locals[local.0 as usize].ty else {
        unreachable!()
    };
    let Type::Tuple(types) = storage.as_ref() else {
        unreachable!()
    };
    let mut incoming = Vec::new();
    for (index, _) in types.iter().enumerate() {
        b.spend(1)?;
        let mut value = Payload::default();
        if let Some(permission) = leaf(
            &state.locals[local.0 as usize],
            &[PlaceProjection::Field(index as u32)],
            b,
        )? {
            join_permission(&mut value.address, permission, b)?;
        }
        incoming.push(value);
    }
    state.locals[local.0 as usize] = Payload::default();
    for (&target, value) in fields.iter().zip(incoming) {
        state.locals[target.0 as usize] = value;
    }
    Ok(())
}

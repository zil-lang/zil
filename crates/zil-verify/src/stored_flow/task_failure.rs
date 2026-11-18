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
    let (elements, whole, input_life) = match raw {
        Type::Tuple(elements) => (elements, false, None),
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
                (inputs, field),
            ] = fields.as_slice()
            else {
                return Err(err("Task failure physical fields differ"));
            };
            let (tuple, input_life) = qualified(field)
                .map_or((field, None), |(tuple, life)| (tuple, Some(life)));
            let Type::Tuple(elements) = tuple else {
                return Err(err("Task failure has no ordered input Tuple"));
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
            (elements, true, input_life)
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
    if let Some(child) = input_life {
        b.spend(child.retained.len().saturating_add(child.nominal_arguments.len()))?;
        if child.binder_count != classes.binder_count || child.slots.len() != classes.slots.len()
            || !child.retained.is_empty() || !child.nominal_arguments.is_empty()
        {
            return Err(err("Task failure input field changes its complete storage domain"));
        }
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
            if let Some(child) = input_life {
                let field = &child.slots[cursor];
                b.spend(field.path.len().saturating_add(1))?;
                if field.path.as_slice() != [PlaceProjection::Field(position as u32)]
                    || field.mode != slot.mode || field.binder != slot.binder
                {
                    return Err(err("Task failure input field changes its exact physical slot"));
                }
            }
            if !ordering.contains(&slot.binder) {
                if slot.binder as usize != ordering.len() {
                    return Err(err(
                        "Task failure view did not alpha-normalize its class order",
                    ));
                }
                ordering.push(slot.binder);
            }
            if whole && input_life.is_none() {
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
    if input_life.is_some() {
        use zil_ir::LifeTypeProjection as L;
        for projection in [L::Place(PlaceProjection::Field(1)), L::TypeArgument(0), L::TypeArgument(1)] {
            for class in 0..classes.binder_count {
                let row = classes.retained.get(retained)
                    .ok_or_else(|| err("Task failure omits an input/context class occurrence"))?;
                b.spend(row.path.len().saturating_add(1))?;
                if row.path.as_slice() != [projection] || row.class != class || row.binder != class {
                    return Err(err("Task failure changes an input/context class occurrence"));
                }
                retained += 1;
            }
        }
    } else if whole {
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
fn plain_actual(argument: &LifeNominalActual) -> Option<&Type> {
    match argument {
        LifeNominalActual::Type(ty) => Some(ty),
        LifeNominalActual::QualifiedType { ty, life }
            if life.binder_count == 0
                && life.slots.is_empty()
                && life.retained.is_empty()
                && life.nominal_arguments.is_empty() =>
        {
            Some(ty)
        }
        _ => None,
    }
}

/// Plain failure metadata has no persistent borrowing domain. The caller
/// independently proves every complete Type actual and physical child Known0;
/// this check seals their ordered nominal/field identity, not an executable
/// Task phase, input source or cleanup certificate.
pub(super) fn plain_instance(
    i: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
) -> Check<()> {
    let (name, fields, record) = match &i.ty {
        Type::Record { name, fields } => (name, fields, true),
        Type::Package { name, fields } => (name, fields, false),
        _ => return Err(err("plain Task failure has another actual carrier kind")),
    };
    let expected = if record {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
    } else {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
    };
    if i.declaration != expected {
        return Err(err(
            "plain Task failure declaration differs from its actual shell",
        ));
    }
    let [inputs, context] = i.arguments.as_slice() else {
        return Err(err(
            "plain Task failure omits its exact Inputs and Context Type actuals",
        ));
    };
    let inputs = plain_actual(inputs)
        .ok_or_else(|| err("plain Task failure changes its complete Inputs Type actual"))?;
    let context = plain_actual(context)
        .ok_or_else(|| err("plain Task failure changes its complete Context Type actual"))?;
    type_work(inputs, limits, b)?;
    type_work(context, limits, b)?;
    b.spend(name.len().saturating_add(fields.len()).saturating_add(96))?;
    let Type::Tuple(elements) = inputs else {
        return Err(err(
            "plain Task failure Inputs is not its complete ordered tuple",
        ));
    };
    let Type::Task {
        result,
        retained_life,
    } = context
    else {
        return Err(err(
            "plain Task failure Context is not its actual Task domain",
        ));
    };
    if result.as_ref() != &Type::Unit
        || retained_life.binder_count != 0
        || !retained_life.ports.is_empty()
        || !retained_life.retained.is_empty()
        || !retained_life.nominal_arguments.is_empty()
    {
        return Err(err(
            "plain Task failure cannot erase a nonneutral complete Context",
        ));
    }
    if name != &format!("@std.task:v1:TaskCreateFailure:{}", inputs.fingerprint()) {
        return Err(err("plain Task failure input nominal identity differs"));
    }
    let [
        (
            reason,
            Type::Enum {
                name: error,
                variants,
            },
        ),
        (input_name, physical_inputs),
    ] = fields.as_slice()
    else {
        return Err(err("plain Task failure complete physical fields differ"));
    };
    if reason != "reason"
        || input_name != "inputs"
        || physical_inputs != inputs
        || error != "@std.task:v1:TaskCreateError"
        || !matches!(variants.as_slice(),[(name,fields)]if name=="Allocation"&&fields.is_empty())
    {
        return Err(err(
            "plain Task failure changes its sealed reason or ordered input fields",
        ));
    }
    b.spend(elements.len())?;
    let needs_owned_shell = elements
        .iter()
        .any(|element| !element.is_copy_value() && !matches!(element, Type::Address { .. }));
    if record == needs_owned_shell {
        return Err(err(
            "plain Task failure shell changes its actual input responsibilities",
        ));
    }
    Ok(())
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
    let [LifeNominalActual::Type(inputs), actual_context] = i.arguments.as_slice()
    else {
        return Err(err(
            "Task failure nominal has no exact Inputs and Context actuals",
        ));
    };
    let full_inputs = qualified(inputs).is_some();
    let context = match actual_context {
        LifeNominalActual::Type(context) if !full_inputs => context,
        LifeNominalActual::QualifiedType { ty, life: context }
            if full_inputs && context.binder_count == life.binder_count
                && context.slots.is_empty() && context.retained.is_empty()
                && context.nominal_arguments.is_empty() => ty,
        _ => return Err(err("Task failure Context changes its complete class domain")),
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
        || !retained_life.nominal_arguments.is_empty()
    {
        return Err(err("Task failure ordered input/context identity differs"));
    }
    let Type::Tuple(inputs) = qualified(inputs).map_or(inputs, |(tuple, _)| tuple) else {
        unreachable!()
    };
    let leaves = inputs
        .iter()
        .enumerate()
        .filter(|(_, ty)| matches!(ty, Type::Address { .. }));
    b.spend(inputs.len().saturating_add(retained_life.ports.len()))?;
    if leaves.clone().count() != retained_life.ports.len() {
        return Err(err("Task failure actual context port count differs"));
    }
    let explicit = full_inputs || !retained_life.retained.is_empty();
    if explicit && retained_life.retained.len() != retained_life.ports.len() {
        return Err(err(
            "Task failure context omits or adds an original input occurrence",
        ));
    }
    for (ordinal, (((parameter, input), port), slot)) in leaves
        .zip(&retained_life.ports)
        .zip(&life.slots)
        .enumerate()
    {
        type_work(&port.leaf, limits, b)?;
        if input != &port.leaf || port.mode != slot.mode || port.binder != slot.binder {
            return Err(err("Task failure actual context port leaf/class differs"));
        }
        if explicit {
            let row = &retained_life.retained[ordinal];
            b.spend(row.path.len().saturating_add(1))?;
            if row.path.as_slice() != [LifeTypeProjection::Parameter(parameter as u32)]
                || row.class != 0
                || row.binder != slot.binder
            {
                return Err(err(
                    "Task failure context changes its exact original retained input occurrence",
                ));
            }
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
    if qualified(&fields[1].1).is_some() {
        return Ok(Some(fields[1].1.clone()));
    }
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
    if let Type::Package { name, fields } | Type::Record { name, fields } = error.as_ref() {
        if !name.starts_with("@std.task:v1:TaskCreateFailure:") {
            return Ok(());
        }
        let [
            (
                reason,
                Type::Enum {
                    name: error_name,
                    variants,
                },
            ),
            (input_name, Type::Tuple(inputs)),
        ] = fields.as_slice()
        else {
            return Err(err("plain Task failure constructor fields differ"));
        };
        b.spend(
            name.len()
                .saturating_add(inputs.len())
                .saturating_add(variants.len()),
        )?;
        if reason != "reason"
            || input_name != "inputs"
            || error_name != "@std.task:v1:TaskCreateError"
            || !matches!(variants.as_slice(), [(name, fields)] if name == "Allocation" && fields.is_empty())
            || inputs.len() != arguments.len()
        {
            return Err(err(
                "plain Task failure constructor loses its exact inputs or reason",
            ));
        }
        for (ordinal, (argument, input)) in arguments.iter().zip(inputs).enumerate() {
            type_work(input, &b.limits.clone(), b)?;
            type_work(vt(f, *argument)?, &b.limits.clone(), b)?;
            if vt(f, *argument)? != input {
                return Err(err("plain Task failure input type differs"));
            }
            let actual = values
                .get(argument.0 as usize)
                .ok_or_else(|| err("plain Task failure input is absent"))?;
            let prefix = [
                PlaceProjection::ErrorArm(true),
                PlaceProjection::Field(1),
                PlaceProjection::Field(ordinal as u32),
            ];
            value_transport::attach(out, &prefix, input, actual, b)?;
        }
        return Ok(());
    }
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
        let actual = values
            .get(argument.0 as usize)
            .ok_or_else(|| err("failure auxiliary input is absent"))?;
        value_transport::attach_aux(
            out,
            &[
                PlaceProjection::ErrorArm(true),
                PlaceProjection::Field(1),
                PlaceProjection::Field(ordinal as u32),
            ],
            vt(f, *argument)?,
            actual,
            b,
        )?;
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
    ty: &Type,
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    let whole = if address.sources.is_empty() {
        None
    } else {
        let position = slot_position(address, &[PlaceProjection::Field(1)], b)?;
        let holder = memory_holder(state, position.base, b)?
            .ok_or_else(|| err("failure Inputs target memory is absent"))?;
        let actual = value_transport::project(holder, &position.inline, ty, b)?;
        join_payload(out, &actual, b)?;
        Some(position)
    };
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
        let holder = memory_holder_mut(state, position.base, b)?;
        if let Some(holder) = holder {
            b.spend(holder.leaves.len())?;
            holder
                .leaves
                .retain(|(path, _)| path != &position.inline, b)?;
        }
        add_sources(&mut state.vacant, &[position], b)?;
    }
    if let Some(position) = whole {
        let holder = memory_holder_mut(state, position.base, b)?
            .ok_or_else(|| err("failure Inputs target memory disappeared"))?;
        value_transport::remove(holder, &position.inline, b)?;
    }
    Ok(())
}

fn failure_name(t: &Type) -> bool {
    matches!(t, Type::Record { name, .. } | Type::Package { name, .. }
        if name.starts_with("@std.task:v1:TaskCreateFailure:"))
}
fn error_carrier(t: &Type) -> bool {
    let Type::ErrorUnion { error, .. } = t else {
        return false;
    };
    // Plain and qualified shells share the sealed original Inputs product.
    // Full declaration/actuals/type qualification remains independently checked.
    failure_name(error)
        || matches!(error.as_ref(), Type::BorrowedStorage { storage, .. } if failure_name(storage))
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
            out = value_transport::project(
                &values[value.0 as usize],
                &selected,
                vt(
                    f,
                    i.result
                        .ok_or_else(|| err("failure extraction result is absent"))?,
                )?,
                b,
            )?;
        }
        InstructionKind::TakeVariantField { local, path }
            if path.as_slice() == selected && error_carrier(&f.locals[local.0 as usize].ty) =>
        {
            let parent = state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| err("failure carrier local is absent"))?;
            out = value_transport::take(
                parent,
                &selected,
                vt(
                    f,
                    i.result
                        .ok_or_else(|| err("failure take result is absent"))?,
                )?,
                b,
            )?;
        }
        InstructionKind::MakeErrorUnion {
            value,
            is_error: true,
        } if i
            .result
            .is_some_and(|r| error_carrier(&f.values[r.0 as usize])) =>
        {
            value_transport::attach(
                &mut out,
                &selected,
                vt(f, *value)?,
                &values[value.0 as usize],
                b,
            )?;
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
        let value = value_transport::project(
            &state.locals[local.0 as usize],
            &[PlaceProjection::Field(index as u32)],
            &types[index],
            b,
        )?;
        incoming.push(value);
    }
    state.locals[local.0 as usize] = Payload::default();
    for (&target, value) in fields.iter().zip(incoming) {
        state.locals[target.0 as usize] = value;
    }
    Ok(())
}

//! The owning failure arm returns every input unchanged. Its full type actuals
//! are checked independently; a scalar physical tuple is not an empty domain.
use super::*;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-TASK", None, message)
}

pub(super) fn check(
    module: &TypedModule,
    proposed: &Type,
    worker: &TypedFunction,
    frame: &crate::semantic::task_domains::FrameMetadata,
    b: &mut Budget,
) -> Check<Type> {
    dynamic_metadata::ty(proposed, b)?;
    let Type::ErrorUnion { error, result } = proposed else {
        return Err(bad(
            "owning Task constructor has another complete outcome kind",
        ));
    };
    let expected = Type::Managed(Box::new(Type::Task {
        result: Box::new(worker.return_type.clone()),
        retained_life: frame.shape.clone(),
    }));
    if **result != expected {
        return Err(bad(
            "owning Task success changes its complete declared worker frame",
        ));
    }
    let (storage, error_life) = qualified(error);
    let (name, fields, record) = match storage {
        Type::Record { name, fields } => (name, fields, true),
        Type::Package { name, fields } => (name, fields, false),
        _ => {
            return Err(bad(
                "owning Task failure does not retain its actual input product",
            ));
        }
    };
    if fields.len() != 2
        || fields[0].0 != "reason"
        || fields[1].0 != "inputs"
        || fields[0].1 != create_error()
    {
        return Err(bad(
            "owning Task failure changes its canonical reason/input fields",
        ));
    }
    let inputs = &fields[1].1;
    let (tuple, tuple_life) = qualified(inputs);
    let Type::Tuple(parameters) = tuple else {
        return Err(bad(
            "owning Task failure inputs are not the full original ordered tuple",
        ));
    };
    if parameters.len() != worker.parameters.len() {
        return Err(bad("owning Task failure omits original worker inputs"));
    }
    for (input, parameter) in parameters.iter().zip(&worker.parameters) {
        if input != local_type(worker, *parameter, Some(worker.span))? {
            return Err(bad("owning Task failure substitutes a worker input type"));
        }
    }
    if record && !copy_inputs(module, inputs, b, 0)? {
        return Err(bad(
            "owning Task failure cannot grant Copy to an affine input",
        ));
    }
    if name != &format!("@std.task:v1:TaskCreateFailure:{}", inputs.fingerprint()) {
        return Err(bad(
            "owning Task failure has another canonical nominal identity",
        ));
    }
    let declaration = if record {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
    } else {
        zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
    };
    b.spend(module.life_instances.len())?;
    let mut found = false;
    for instance in &module.life_instances {
        if instance.declaration != declaration || instance.ty != **error {
            continue;
        }
        for actual in &instance.arguments {
            match actual {
                LifeNominalActual::Type(ty) => dynamic_metadata::ty(ty, b)?,
                LifeNominalActual::QualifiedType { ty, life } => {
                    dynamic_metadata::ty(ty, b)?;
                    crate::semantic::task_domains::charge_life(life, b)?;
                }
                _ => b.spend(1)?,
            }
        }
        if instance.arguments.len() != 2 {
            return Err(bad(
                "owning Task failure omits its full ordered Type actuals",
            ));
        }
        let Some((actual, life)) = type_actual(&instance.arguments[0]) else {
            return Err(bad("owning Task failure inputs actual has another kind"));
        };
        if actual != inputs || life != tuple_life {
            return Err(bad(
                "owning Task failure erases its complete input Type actual",
            ));
        }
        let Some((task, task_life)) = type_actual(&instance.arguments[1]) else {
            return Err(bad("owning Task failure Task actual has another kind"));
        };
        let mut input_context = frame.shape.clone();
        input_context
            .retained
            .retain(|row| row.path != [LifeTypeProjection::Result]);
        let context = Type::Task {
            result: Box::new(Type::Unit),
            retained_life: input_context,
        };
        let full_context = StorageLifeShape {
            binder_count: frame.shape.binder_count,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        };
        b.spend(frame.shape.binder_count as usize)?;
        if task != &context || task_life != full_context {
            return Err(bad(
                "owning Task failure loses the complete phantom input-context actual",
            ));
        }
        found = true;
    }
    if !found {
        return Err(bad(
            "owning Task failure has no exact complete builtin nominal instance",
        ));
    }
    full_type(module, inputs, &tuple_life, b)?;
    full_type(module, error, &error_life, b)?;
    // Input SSA values, actual error/success ownership, and every failure
    // cleanup edge are checked by the independent ordinary flow passes.
    Ok(proposed.clone())
}

pub(super) fn full_type(
    module: &TypedModule,
    ty: &Type,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> Check<()> {
    dynamic_metadata::ty(ty, b)?;
    if life.binder_count as usize > b.limits.max_nodes {
        return Err(limit("Task complete nominal domain exceeded"));
    }
    crate::semantic::task_domains::charge_life(life, b)?;
    b.spend(life.binder_count as usize)?;
    let value = DynTypeValue {
        ty: ty.clone(),
        life: life.clone(),
        classes: (0..life.binder_count).map(StorageBinder::Fixed).collect(),
    };
    dynamic_witness::check_type_value(
        module,
        &value,
        DynInstantiationScope {
            fixed_binder_count: life.binder_count,
            invocation_binder_count: 0,
        },
        b,
    )
    .map(|_| ())
}
fn qualified(ty: &Type) -> (&Type, StorageLifeShape) {
    match ty {
        Type::BorrowedStorage { storage, life } => (storage, life.clone()),
        _ => (
            ty,
            StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        ),
    }
}
fn type_actual(actual: &LifeNominalActual) -> Option<(&Type, StorageLifeShape)> {
    match actual {
        LifeNominalActual::Type(ty) => Some((ty, qualified(ty).1)),
        LifeNominalActual::QualifiedType { ty, life } => Some((ty, life.clone())),
        _ => None,
    }
}
fn create_error() -> Type {
    Type::Enum {
        name: "@std.task:v1:TaskCreateError".into(),
        variants: vec![("Allocation".into(), vec![])],
    }
}
fn copy_inputs(module: &TypedModule, ty: &Type, b: &mut Budget, depth: usize) -> Check<bool> {
    dynamic_metadata::ty(ty, b)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("Task Copy input depth exceeded"));
    }
    Ok(match ty {
        Type::BorrowedStorage { storage, .. } => copy_inputs(module, storage, b, depth + 1)?,
        Type::Address { mutable: false, .. } | Type::Desc(_) => true,
        Type::Tuple(fields) => {
            for field in fields {
                if !copy_inputs(module, field, b, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Record { fields, .. } => {
            b.spend(module.resources.len())?;
            if module
                .resources
                .iter()
                .any(|row| row.payload == *ty && row.teardown.is_some())
            {
                return Ok(false);
            }
            for (_, field) in fields {
                if !copy_inputs(module, field, b, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    if !copy_inputs(module, field, b, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            true
        }
        Type::ErrorUnion { error, result } => {
            copy_inputs(module, error, b, depth + 1)? && copy_inputs(module, result, b, depth + 1)?
        }
        Type::Array { element, .. } => copy_inputs(module, element, b, depth + 1)?,
        _ => ty.is_copy_value(),
    })
}

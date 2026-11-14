//! Complete zero persistent domains for plain nominal inventory entries.
//! This proof creates no Copy, ownership, address, loan or access authority.
use super::*;

pub(super) fn validate(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    crate::dyn_metadata::ty(&instance.ty, b)?;
    instance_zero(module, instance, limits, b, &mut Vec::new(), 0)
}

pub(super) fn validate_type(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    type_zero(module, ty, limits, b, &mut Vec::new(), 0)
}

fn exact<'a>(
    module: &'a TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<&'a LifeNominalInstance> {
    fee(b, module.life_instances.len())?;
    let mut selected = None;
    for instance in &module.life_instances {
        shape_work(ty, limits, b)?;
        shape_work(&instance.ty, limits, b)?;
        crate::dyn_metadata::ty(ty, b)?;
        crate::dyn_metadata::ty(&instance.ty, b)?;
        if instance.ty == *ty && selected.replace(instance).is_some() {
            return Err(failure(
                "zero nominal type has ambiguous complete instances",
            ));
        }
    }
    selected.ok_or_else(|| failure("zero nominal type lacks its complete instance"))
}

fn empty(life: &StorageLifeShape) -> bool {
    life.binder_count == 0
        && life.slots.is_empty()
        && life.retained.is_empty()
        && life.nominal_arguments.is_empty()
}

fn instance_zero(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
    active: &mut Vec<usize>,
    depth: usize,
) -> CompileResult<()> {
    fee(b, active.len().saturating_add(1))?;
    if depth > limits.max_call_depth {
        return Err(failure("zero nominal domain exceeds the type depth limit"));
    }
    let key = instance as *const LifeNominalInstance as usize;
    if active.contains(&key) {
        return Err(failure("cyclic complete zero nominal domain"));
    }
    fee(b, 1)?;
    active.push(key);
    let storage = match &instance.ty {
        Type::BorrowedStorage { storage, life } if empty(life) => storage.as_ref(),
        Type::BorrowedStorage { .. } => {
            return Err(failure(
                "plain nominal depends on a nonzero qualified domain",
            ));
        }
        ty => ty,
    };
    fee(b, module.life_declarations.len())?;
    let declaration = module
        .life_declarations
        .iter()
        .find(|declaration| declaration.id == instance.declaration)
        .ok_or_else(|| failure("zero nominal declaration is absent"))?;
    identity_work(&declaration.identity, b)?;
    if !matches!(
        (declaration.kind, storage),
        (LifeNominalKind::Record, Type::Record { .. })
            | (LifeNominalKind::Package, Type::Package { .. })
            | (LifeNominalKind::Enum, Type::Enum { .. })
    ) {
        return Err(failure(
            "zero nominal declaration kind differs from its actual type",
        ));
    }
    if declaration.id.compiler {
        fee(b, 96)?;
        if matches!(instance.ty, Type::BorrowedStorage { .. }) {
            if !option_address::declaration(declaration)
                && !enum_address::poll_declaration(declaration)
                && !task_failure::declaration(declaration)
            {
                return Err(failure(
                    "zero nominal compiler declaration has no authentic complete role",
                ));
            }
            nominal::rows(module, instance, limits, b)?;
        } else {
            compiler_zero(declaration, instance, storage, b)?;
        }
    }
    fee(
        b,
        declaration
            .parameters
            .len()
            .saturating_add(instance.arguments.len()),
    )?;
    if declaration.parameters.len() != instance.arguments.len() {
        return Err(failure("zero nominal omits original mixed actual ordinals"));
    }
    // Phantom actuals participate even when no physical field mentions them.
    for (formal, actual) in declaration.parameters.iter().zip(&instance.arguments) {
        match (formal, actual) {
            (LifeParameterKind::Type, LifeNominalActual::Type(ty)) => {
                type_zero(module, ty, limits, b, active, depth + 1)?;
            }
            (LifeParameterKind::Type, LifeNominalActual::QualifiedType { ty, life }) => {
                fee(
                    b,
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len())
                        .saturating_add(1),
                )?;
                if !empty(life) {
                    return Err(failure("plain nominal has a nonzero qualified Type actual"));
                }
                type_zero(module, ty, limits, b, active, depth + 1)?;
            }
            (LifeParameterKind::Const(carrier), LifeNominalActual::Const(value)) => {
                type_zero(module, carrier, limits, b, active, depth + 1)?;
                b.type_work(carrier, false, "zero nominal constant carrier")?;
                if value.ty() != *carrier
                    || !matches!(
                        value,
                        Constant::Bool(_)
                            | Constant::I8(_)
                            | Constant::I16(_)
                            | Constant::I32(_)
                            | Constant::I64(_)
                            | Constant::Isize(_)
                            | Constant::U8(_)
                            | Constant::U16(_)
                            | Constant::U32(_)
                            | Constant::U64(_)
                            | Constant::Usize(_)
                    )
                {
                    return Err(failure("zero nominal const actual differs from its formal"));
                }
            }
            (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(_)) => {
                return Err(failure("plain nominal omits a persistent lifetime actual"));
            }
            _ => {
                return Err(failure(
                    "zero nominal actual kind differs from its full formal",
                ));
            }
        }
    }
    physical_zero(module, storage, limits, b, active, depth + 1)?;
    active.pop();
    Ok(())
}

fn compiler_zero(
    declaration: &LifeNominalDeclaration,
    instance: &LifeNominalInstance,
    storage: &Type,
    b: &mut Budget,
) -> CompileResult<()> {
    let Type::Enum { name, variants } = storage else {
        return Err(failure(
            "plain compiler nominal has no supported complete enum body",
        ));
    };
    fee(b, name.len().saturating_add(variants.len() + 1))?;
    if option_address::declaration(declaration) || enum_address::poll_declaration(declaration) {
        let child = match instance.arguments.as_slice() {
            [LifeNominalActual::Type(ty)] => ty,
            [LifeNominalActual::QualifiedType { ty, life }] if empty(life) => ty,
            _ => {
                return Err(failure(
                    "plain compiler enum loses its complete original Type actual",
                ));
            }
        };
        crate::dyn_metadata::ty(child, b)?;
        let (empty_name, value_name) = if declaration.id == zil_ir::builtin_life::OPTION {
            ("None", "Some")
        } else {
            ("Pending", "Ready")
        };
        if name.is_empty()
            || !matches!(variants.as_slice(),[(none,empty),(some,fields)] if none==empty_name && empty.is_empty() && some==value_name && matches!(fields.as_slice(),[payload] if payload==child))
        {
            return Err(failure(
                "plain compiler enum changes its canonical original variants or complete payload",
            ));
        }
        return Ok(());
    }
    let (source, physical, expected): (&str, &str, &[&str]) = match declaration.id {
        zil_ir::builtin_life::TASK_CREATE_ERROR => (
            "TaskCreateError",
            "@std.task:v1:TaskCreateError",
            &["Allocation"],
        ),
        zil_ir::builtin_life::TASK_POLL_STATE => (
            "TaskPollState",
            "@std.task:v1:TaskPollState",
            &["Complete", "Cancelled", "Invalid"],
        ),
        _ => {
            return Err(failure(
                "plain compiler nominal has no authentic sealed role",
            ));
        }
    };
    if declaration.kind != LifeNominalKind::Enum
        || !declaration.parameters.is_empty()
        || !instance.arguments.is_empty()
        || declaration.identity.package != "@zil.builtin"
        || declaration.identity.module_path != ["task"]
        || declaration.identity.declaration_path != ["enum", source]
        || !declaration.identity.generation_path.is_empty()
        || name != physical
        || variants.len() != expected.len()
    {
        return Err(failure(
            "plain Task enum changes its original role identity, formals or physical declaration",
        ));
    }
    for ((actual, fields), expected) in variants.iter().zip(expected) {
        fee(b, actual.len().saturating_add(fields.len() + 1))?;
        if actual.as_str() != *expected || !fields.is_empty() {
            return Err(failure(
                "plain Task enum changes its exact tag order or nullary payload",
            ));
        }
    }
    Ok(())
}

fn physical_zero(
    module: &TypedModule,
    storage: &Type,
    limits: &Limits,
    b: &mut Budget,
    active: &mut Vec<usize>,
    depth: usize,
) -> CompileResult<()> {
    fee(b, 1)?;
    match storage {
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fee(b, fields.len())?;
            for (_, ty) in fields {
                type_zero(module, ty, limits, b, active, depth)?;
            }
        }
        Type::Enum { variants, .. } => {
            fee(b, variants.len())?;
            for (_, fields) in variants {
                fee(b, fields.len())?;
                for ty in fields {
                    type_zero(module, ty, limits, b, active, depth)?;
                }
            }
        }
        _ => return Err(failure("zero nominal has no complete physical body")),
    }
    Ok(())
}

fn type_zero(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
    active: &mut Vec<usize>,
    depth: usize,
) -> CompileResult<()> {
    fee(b, 1)?;
    if depth > limits.max_call_depth {
        return Err(failure(
            "zero nominal dependency exceeds the type depth limit",
        ));
    }
    // Prepay complete recursive identity work, including opaque/retained data,
    // before any structural comparison or directory lookup.
    crate::dyn_metadata::ty(ty, b)?;
    b.type_work(ty, false, "complete zero nominal dependency")?;
    match ty {
        Type::Unit
        | Type::Bool
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::Isize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64
        | Type::ForeignHandle
        | Type::CWideString
        | Type::ByteBuffer
        | Type::SyncCell(_) => Ok(()),
        Type::Record { .. } | Type::Package { .. } | Type::Enum { .. } => instance_zero(
            module,
            exact(module, ty, limits, b)?,
            limits,
            b,
            active,
            depth,
        ),
        Type::BorrowedStorage { storage, life } => {
            if !empty(life) {
                return Err(failure("plain nominal contains a nonzero qualified domain"));
            }
            if matches!(
                storage.as_ref(),
                Type::Record { .. } | Type::Package { .. } | Type::Enum { .. }
            ) {
                instance_zero(
                    module,
                    exact(module, ty, limits, b)?,
                    limits,
                    b,
                    active,
                    depth,
                )
            } else {
                physical(ty, b)?;
                type_zero(module, storage, limits, b, active, depth + 1)
            }
        }
        Type::Tuple(fields) => {
            fee(b, fields.len())?;
            for ty in fields {
                type_zero(module, ty, limits, b, active, depth + 1)?;
            }
            Ok(())
        }
        Type::Array { element, .. }
        | Type::Slice { element }
        | Type::OwnedStore { element }
        | Type::Managed(element)
        | Type::Mutex { payload: element } => {
            type_zero(module, element, limits, b, active, depth + 1)
        }
        Type::ErrorUnion { error, result } => {
            type_zero(module, error, limits, b, active, depth + 1)?;
            type_zero(module, result, limits, b, active, depth + 1)
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            fee(b, parameters.len())?;
            for ty in parameters {
                type_zero(module, ty, limits, b, active, depth + 1)?;
            }
            type_zero(module, result, limits, b, active, depth + 1)
        }
        Type::Address { .. } | Type::Desc(_) => Err(failure(
            "plain nominal contains an outer persistent borrow occurrence",
        )),
        Type::Closure(id) => {
            b.lookup(
                b.semantic_zero_closures.len(),
                "completed zero closure domain",
            )?;
            if b.semantic_zero_closures.contains(id) {
                Ok(())
            } else {
                Err(failure(
                    "plain nominal closure dependency lacks its completed zero-domain proof",
                ))
            }
        }
        // Their hidden class domains need the completed original/opaque
        // consumer. Neither an empty port list nor Some(0) proves this case.
        Type::Opaque { .. }
        | Type::Dyn { .. }
        | Type::OwnedDyn { .. }
        | Type::TaskI32
        | Type::Task { .. }
        | Type::MutexGuard { .. }
        | Type::Registration { .. }
        | Type::AsyncRegistration { .. }
        | Type::BorrowedFunctionPointer { .. }
        | Type::Named(_)
        | Type::Unsupported(_) => Err(failure(
            "plain nominal dependency has no complete zero-domain proof",
        )),
    }
}

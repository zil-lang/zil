//! Private nominal metadata proof. No physical target or permission is produced.
use super::*;

pub(super) fn rows(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    walk(module, instance, limits, b, &mut Vec::new()).map(|_| ())
}

fn walk(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
    active: &mut Vec<usize>,
) -> CompileResult<Vec<NominalLifeArgument>> {
    if !matches!(instance.ty, Type::BorrowedStorage { .. }) {
        nominal_zero::validate(module, instance, limits, b)?;
        return Ok(Vec::new());
    }
    fee(b, active.len().saturating_add(1))?;
    if active.len() > limits.max_call_depth {
        return Err(failure("nominal Type argument recursion depth exceeded"));
    }
    let identity = instance as *const LifeNominalInstance as usize;
    if active.contains(&identity) {
        return Err(failure("cyclic nominal Type argument identity"));
    }
    fee(b, 1)?;
    active.push(identity);
    physical(&instance.ty, b)?;
    let Type::BorrowedStorage { storage, life } = &instance.ty else {
        return Err(failure("nominal metadata has no qualified type"));
    };
    fee(b, module.life_declarations.len())?;
    let decl = module
        .life_declarations
        .iter()
        .find(|d| d.id == instance.declaration)
        .ok_or_else(|| failure("nominal metadata declaration is absent"))?;
    if decl.id.compiler {
        fee(b, 96)?;
    }
    if (decl.id.compiler
        && !option_address::declaration(decl)
        && !enum_address::poll_declaration(decl)
        && !task_failure::declaration(decl))
        || !matches!(
            (decl.kind, storage.as_ref()),
            (
                LifeNominalKind::Record | LifeNominalKind::Resource,
                Type::Record { .. }
            ) | (LifeNominalKind::Package, Type::Package { .. })
                | (LifeNominalKind::Enum, Type::Enum { .. })
        )
        || decl.parameters.len() != instance.arguments.len()
    {
        return Err(failure(
            "nominal metadata kind or full actual arity differs",
        ));
    }
    fee(
        b,
        instance
            .arguments
            .len()
            .saturating_add(life.nominal_arguments.len()),
    )?;
    if task_failure::declaration(decl) {
        task_failure::instance(instance, b)?;
        active.pop();
        return Ok(Vec::new());
    }
    if decl.id.compiler && enum_address::poll_declaration(decl) {
        enum_address::poll_instance(instance, b)?;
        active.pop();
        return Ok(Vec::new());
    }
    if decl.id.compiler {
        for actual in &instance.arguments {
            if let LifeNominalActual::Type(ty) = actual {
                shape_work(ty, limits, b)?;
                b.type_work(ty, true, "builtin Option actual equality")?;
            }
        }
        shape_work(storage, limits, b)?;
        b.type_work(storage, true, "builtin Option physical equality")?;
        if !option_address::shape(&instance.ty, b)?
            || !matches!((instance.arguments.as_slice(), storage.as_ref()),
                ([LifeNominalActual::Type(actual)], Type::Enum { variants, .. })
                if actual == &variants[1].1[0])
        {
            return Err(failure(
                "builtin Option actual, source-class map or physical payload differs",
            ));
        }
        active.pop();
        return Ok(Vec::new());
    }
    physical_instances(module, storage, limits, b)?;
    enum_address::arguments(instance, b)?;
    let mut expected = Vec::new();
    // All direct lifetimes precede recursive Type actuals, regardless of their ordinal.
    for (ordinal, (formal, actual)) in decl.parameters.iter().zip(&instance.arguments).enumerate() {
        match (formal, actual) {
            (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(class))
                if *class < life.binder_count =>
            {
                fee(b, 1)?;
                expected.push(NominalLifeArgument {
                    path: Vec::new(),
                    declaration: decl.id,
                    argument: ordinal as u32,
                    binder: *class,
                });
            }
            (LifeParameterKind::Type, LifeNominalActual::Type(_)) => {}
            (LifeParameterKind::Const(ty), LifeNominalActual::Const(c))
                if c.ty() == *ty
                    && matches!(
                        c,
                        Constant::Bool(_)
                            | Constant::I32(_)
                            | Constant::I8(_)
                            | Constant::I16(_)
                            | Constant::Isize(_)
                            | Constant::I64(_)
                            | Constant::U64(_)
                            | Constant::Usize(_)
                            | Constant::U8(_)
                            | Constant::U16(_)
                            | Constant::U32(_)
                    ) => {}
            _ => {
                return Err(failure(
                    "nominal metadata actual kind or class differs from full formal",
                ));
            }
        }
    }
    for (ordinal, actual) in instance.arguments.iter().enumerate() {
        let LifeNominalActual::Type(ty) = actual else {
            continue;
        };
        shape_work(ty, limits, b)?;
        b.type_work(ty, true, "nominal metadata identity")?;
        if enum_address::candidate(&instance.ty)
            && matches!(ty, Type::Address { mutable: false, .. })
        {
            continue;
        }
        if !matches!(ty, Type::BorrowedStorage { .. }) {
            nominal_zero::validate_type(module, ty, limits, b)?;
            continue;
        }
        // Full equality resolves actual semantic identity; names and row claims never select a child.
        let mut selected = None;
        for child in &module.life_instances {
            shape_work(&child.ty, limits, b)?;
            b.type_work(&child.ty, true, "nominal metadata identity")?;
            shape_work(ty, limits, b)?;
            b.type_work(ty, true, "nominal metadata identity")?;
            if child.ty == *ty && selected.replace(child).is_some() {
                return Err(failure(
                    "Type actual has ambiguous complete nominal instances",
                ));
            }
        }
        let child =
            selected.ok_or_else(|| failure("Type actual has no complete nominal instance"))?;
        let child_rows = walk(module, child, limits, b, active)?;
        let Type::BorrowedStorage {
            life: child_life, ..
        } = &child.ty
        else {
            unreachable!()
        };
        let mut mapping = Vec::<(u32, u32)>::new();
        for child_row in child_rows {
            let row = life
                .nominal_arguments
                .get(expected.len())
                .ok_or_else(|| failure("nested nominal lifetime row is absent"))?;
            fee(
                b,
                row.path
                    .len()
                    .saturating_add(child_row.path.len())
                    .saturating_add(mapping.len())
                    .saturating_add(2),
            )?;
            let mut path = Vec::with_capacity(child_row.path.len().saturating_add(1));
            path.push(LifeTypeProjection::TypeArgument(ordinal as u32));
            path.extend_from_slice(&child_row.path);
            if row.path != path
                || row.declaration != child_row.declaration
                || row.argument != child_row.argument
                || row.binder >= life.binder_count
            {
                return Err(failure(
                    "nested nominal row differs from actual Type argument graph",
                ));
            }
            if let Some((_, target)) = mapping.iter().find(|(class, _)| *class == child_row.binder)
            {
                if *target != row.binder {
                    return Err(failure("one Type argument local class maps inconsistently"));
                }
            } else {
                fee(b, 1)?;
                mapping.push((child_row.binder, row.binder));
            }
            fee(b, 1)?;
            expected.push(NominalLifeArgument {
                path,
                declaration: child_row.declaration,
                argument: child_row.argument,
                binder: row.binder,
            });
        }
        fee(b, mapping.len().saturating_add(1))?;
        if mapping.len() != child_life.binder_count as usize {
            return Err(failure(
                "Type argument has a fixed class without a complete nominal metadata occurrence",
            ));
        }
    }
    if expected.len() != life.nominal_arguments.len() {
        return Err(failure(
            "nominal lifetime metadata coverage differs from complete actual graph",
        ));
    }
    for (actual, row) in expected.iter().zip(&life.nominal_arguments) {
        fee(
            b,
            actual
                .path
                .len()
                .saturating_add(row.path.len())
                .saturating_add(1),
        )?;
        if actual != row {
            return Err(failure(
                "nominal metadata order, declaration, ordinal or class differs",
            ));
        }
    }
    active.pop();
    Ok(expected)
}

fn physical_instances(
    module: &TypedModule,
    storage: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    let mut todo = vec![(storage, true)];
    while let Some((ty, root)) = todo.pop() {
        fee(b, 1)?;
        if !root
            && matches!(
                ty,
                Type::Record { .. } | Type::Package { .. } | Type::Enum { .. }
            )
        {
            nominal_zero::validate_type(module, ty, limits, b)?;
            continue;
        }
        match ty {
            Type::BorrowedStorage { .. } => {
                let mut count = 0usize;
                for child in &module.life_instances {
                    shape_work(ty, limits, b)?;
                    b.type_work(ty, true, "nominal metadata identity")?;
                    shape_work(&child.ty, limits, b)?;
                    b.type_work(&child.ty, true, "nominal metadata identity")?;
                    if child.ty == *ty {
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(failure(
                        "physical retained nominal child lacks a unique complete instance",
                    ));
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fee(b, fields.len())?;
                todo.extend(fields.iter().map(|(_, t)| (t, false)));
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                todo.extend(fields.iter().map(|t| (t, false)));
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    fee(b, fields.len().saturating_add(1))?;
                    todo.extend(fields.iter().map(|t| (t, false)));
                }
            }
            Type::Array { element, .. } => {
                fee(b, 1)?;
                todo.push((element, false));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn same_key(
    a: &LifeNominalInstance,
    c: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<bool> {
    fee(
        b,
        a.arguments
            .len()
            .saturating_add(c.arguments.len())
            .saturating_add(1),
    )?;
    for actual in a.arguments.iter().chain(&c.arguments) {
        match actual {
            LifeNominalActual::Type(ty) => {
                shape_work(ty, limits, b)?;
                b.type_work(ty, true, "nominal actual vector identity")?;
            }
            LifeNominalActual::Const(Constant::WideString(data)) => {
                fee(b, data.len())?;
            }
            _ => {}
        }
    }
    let (Type::BorrowedStorage { life: al, .. }, Type::BorrowedStorage { life: cl, .. }) =
        (&a.ty, &c.ty)
    else {
        return Ok(!matches!(a.ty, Type::BorrowedStorage { .. })
            && !matches!(c.ty, Type::BorrowedStorage { .. })
            && a.declaration == c.declaration
            && a.arguments == c.arguments);
    };
    fee(
        b,
        al.nominal_arguments
            .len()
            .saturating_add(cl.nominal_arguments.len()),
    )?;
    for row in al.nominal_arguments.iter().chain(&cl.nominal_arguments) {
        fee(b, row.path.len().saturating_add(1))?;
    }
    // The Type actual's local scope alone omits the enclosing occurrence's
    // class relation. Include the independently checked nominal relation.
    Ok(a.declaration == c.declaration
        && a.arguments == c.arguments
        && al.nominal_arguments == cl.nominal_arguments)
}

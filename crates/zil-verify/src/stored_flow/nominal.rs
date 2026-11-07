//! Private nominal metadata proof. No physical target or permission is produced.
use super::*;

pub(super) fn rows(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
) -> Check<()> {
    walk(module, instance, limits, b, &mut Vec::new()).map(|_| ())
}

fn walk(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
    active: &mut Vec<usize>,
) -> Check<Vec<NominalLifeArgument>> {
    b.spend(active.len().saturating_add(1))?;
    if active.len() > limits.max_call_depth {
        return Err(err("nominal Type argument recursion depth exceeded"));
    }
    let identity = instance as *const LifeNominalInstance as usize;
    if active.contains(&identity) {
        return Err(err("cyclic nominal Type argument identity"));
    }
    b.spend(1)?;
    active.push(identity);
    physical(&instance.ty, limits, b)?;
    let Type::BorrowedStorage { storage, life } = &instance.ty else {
        return Err(err("nominal metadata has no qualified type"));
    };
    b.spend(module.life_declarations.len())?;
    let decl = module
        .life_declarations
        .iter()
        .find(|d| d.id == instance.declaration)
        .ok_or_else(|| err("nominal metadata declaration is absent"))?;
    if decl.id.compiler {
        b.spend(96)?;
    }
    if (decl.id.compiler
        && !option_address::declaration(decl)
        && !enum_address::poll_declaration(decl))
        || !matches!(
            (decl.kind, storage.as_ref()),
            (LifeNominalKind::Record, Type::Record { .. })
                | (LifeNominalKind::Package, Type::Package { .. })
                | (LifeNominalKind::Enum, Type::Enum { .. })
        )
        || decl.parameters.len() != instance.arguments.len()
    {
        return Err(err("nominal metadata kind or full actual arity differs"));
    }
    b.spend(
        instance
            .arguments
            .len()
            .saturating_add(life.nominal_arguments.len()),
    )?;
    if decl.id.compiler && enum_address::poll_declaration(decl) {
        enum_address::poll_instance(instance, b)?;
        active.pop();
        return Ok(Vec::new());
    }
    if decl.id.compiler {
        for actual in &instance.arguments {
            if let LifeNominalActual::Type(ty) = actual {
                type_work(ty, limits, b)?;
            }
        }
        type_work(storage, limits, b)?;
        if !option_address::shape(&instance.ty, limits, b)?
            || !matches!((instance.arguments.as_slice(), storage.as_ref()),
                ([LifeNominalActual::Type(actual)], Type::Enum { variants, .. })
                if actual == &variants[1].1[0])
        {
            return Err(err(
                "builtin Option actual, source-class map or physical payload differs",
            ));
        }
        active.pop();
        return Ok(Vec::new());
    }
    physical_instances(module, storage, limits, b)?;
    enum_address::arguments(instance, limits, b)?;
    let mut expected = Vec::new();
    // All direct lifetimes precede recursive Type actuals, regardless of their ordinal.
    for (ordinal, (formal, actual)) in decl.parameters.iter().zip(&instance.arguments).enumerate() {
        match (formal, actual) {
            (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(class))
                if *class < life.binder_count =>
            {
                b.spend(1)?;
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
                            | Constant::I64(_)
                            | Constant::U64(_)
                            | Constant::Usize(_)
                            | Constant::U8(_)
                            | Constant::U16(_)
                    ) => {}
            _ => {
                return Err(err(
                    "nominal metadata actual kind or class differs from full formal",
                ));
            }
        }
    }
    for (ordinal, actual) in instance.arguments.iter().enumerate() {
        let LifeNominalActual::Type(ty) = actual else {
            continue;
        };
        type_work(ty, limits, b)?;
        if enum_address::candidate(&instance.ty)
            && matches!(ty, Type::Address { mutable: false, .. })
        {
            continue;
        }
        if !matches!(ty, Type::BorrowedStorage { .. }) {
            if !context_free_copy(ty, b)? {
                return Err(err(
                    "non-nominal borrowed Type actual requires its semantic metadata consumer",
                ));
            }
            continue;
        }
        // Full equality resolves actual semantic identity; names and row claims never select a child.
        let mut selected = None;
        for child in &module.life_instances {
            type_work(&child.ty, limits, b)?;
            type_work(ty, limits, b)?;
            if child.ty == *ty && selected.replace(child).is_some() {
                return Err(err("Type actual has ambiguous complete nominal instances"));
            }
        }
        let child = selected.ok_or_else(|| err("Type actual has no complete nominal instance"))?;
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
                .ok_or_else(|| err("nested nominal lifetime row is absent"))?;
            b.spend(
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
                return Err(err(
                    "nested nominal row differs from actual Type argument graph",
                ));
            }
            if let Some((_, target)) = mapping.iter().find(|(class, _)| *class == child_row.binder)
            {
                if *target != row.binder {
                    return Err(err("one Type argument local class maps inconsistently"));
                }
            } else {
                b.spend(1)?;
                mapping.push((child_row.binder, row.binder));
            }
            b.spend(1)?;
            expected.push(NominalLifeArgument {
                path,
                declaration: child_row.declaration,
                argument: child_row.argument,
                binder: row.binder,
            });
        }
        b.spend(mapping.len().saturating_add(1))?;
        if mapping.len() != child_life.binder_count as usize {
            return Err(err(
                "Type argument has a fixed class without a complete nominal metadata occurrence",
            ));
        }
    }
    if expected.len() != life.nominal_arguments.len() {
        return Err(err(
            "nominal lifetime metadata coverage differs from complete actual graph",
        ));
    }
    for (actual, row) in expected.iter().zip(&life.nominal_arguments) {
        b.spend(
            actual
                .path
                .len()
                .saturating_add(row.path.len())
                .saturating_add(1),
        )?;
        if actual != row {
            return Err(err(
                "nominal metadata order, declaration, ordinal or class differs",
            ));
        }
    }
    active.pop();
    Ok(expected)
}

fn context_free_copy(ty: &Type, b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            Type::BorrowedFunctionPointer { .. } | Type::BorrowedStorage { .. } => {
                return Ok(false);
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    b.spend(fields.len().saturating_add(1))?;
                    pending.extend(fields);
                }
            }
            Type::Array { element, .. } => {
                b.spend(1)?;
                pending.push(element);
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.push(error);
                pending.push(result);
            }
            Type::FunctionPointer { parameters, result } => {
                b.spend(parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            other if other.is_copy_value() => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn physical_instances(
    module: &TypedModule,
    storage: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?;
    let mut todo = vec![storage];
    while let Some(ty) = todo.pop() {
        b.spend(1)?;
        match ty {
            Type::BorrowedStorage { .. } => {
                let mut count = 0usize;
                for child in &module.life_instances {
                    type_work(ty, limits, b)?;
                    type_work(&child.ty, limits, b)?;
                    if child.ty == *ty {
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(err(
                        "physical retained nominal child lacks a unique complete instance",
                    ));
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                todo.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                todo.extend(fields);
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    b.spend(fields.len().saturating_add(1))?;
                    todo.extend(fields);
                }
            }
            Type::Array { element, .. } => {
                b.spend(1)?;
                todo.push(element);
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
) -> Check<bool> {
    b.spend(
        a.arguments
            .len()
            .saturating_add(c.arguments.len())
            .saturating_add(1),
    )?;
    for actual in a.arguments.iter().chain(&c.arguments) {
        match actual {
            LifeNominalActual::Type(ty) => {
                type_work(ty, limits, b)?;
            }
            LifeNominalActual::Const(Constant::WideString(data)) => {
                b.spend(data.len())?;
            }
            _ => {}
        }
    }
    let (Type::BorrowedStorage { life: al, .. }, Type::BorrowedStorage { life: cl, .. }) =
        (&a.ty, &c.ty)
    else {
        return Ok(false);
    };
    b.spend(
        al.nominal_arguments
            .len()
            .saturating_add(cl.nominal_arguments.len()),
    )?;
    for row in al.nominal_arguments.iter().chain(&cl.nominal_arguments) {
        b.spend(row.path.len().saturating_add(1))?;
    }
    // The Type actual's local scope alone omits the enclosing occurrence's
    // class relation. Include the independently checked nominal relation.
    Ok(a.declaration == c.declaration
        && a.arguments == c.arguments
        && al.nominal_arguments == cl.nominal_arguments)
}

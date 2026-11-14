//! Private nominal metadata proof. No physical target or permission is produced.
use super::*;

pub(super) fn rows(
    module: &CoreModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    left: &mut usize,
    phase: InventoryPhase,
) -> R<()> {
    if !matches!(instance.ty, Type::BorrowedStorage { .. }) {
        return super::ordinary_nominal::rows(module, instance, limits, left, phase);
    }
    walk(module, instance, limits, left, &mut Vec::new(), phase).map(|_| ())
}

fn walk(
    module: &CoreModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    left: &mut usize,
    active: &mut Vec<usize>,
    phase: InventoryPhase,
) -> R<Vec<NominalLifeArgument>> {
    let span = Span::default();
    fee(left, active.len().saturating_add(1), span)?;
    if active.len() > limits.max_call_depth {
        return Err(error(
            "E-STORED-IDENTITY",
            span,
            "nominal Type argument recursion depth exceeded",
        ));
    }
    let identity = instance as *const LifeNominalInstance as usize;
    if active.contains(&identity) {
        return Err(error(
            "E-STORED-IDENTITY",
            span,
            "cyclic nominal Type argument identity",
        ));
    }
    fee(left, 1, span)?;
    active.push(identity);
    if phase == InventoryPhase::Resolved {
        shape(&instance.ty, left, limits.max_call_depth, span)?;
    }
    let Type::BorrowedStorage { storage, life } = &instance.ty else {
        return Err(error(
            "E-STORED-IDENTITY",
            span,
            "nominal metadata has no qualified type",
        ));
    };
    fee(left, module.life_declarations.len(), span)?;
    let decl = module
        .life_declarations
        .iter()
        .find(|d| d.id == instance.declaration)
        .ok_or_else(|| {
            error(
                "E-STORED-IDENTITY",
                span,
                "nominal metadata declaration is absent",
            )
        })?;
    if decl.id.compiler {
        fee(left, 96, span)?;
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
        return Err(error(
            "E-STORED-IDENTITY",
            span,
            "nominal metadata kind or full actual arity differs",
        ));
    }
    fee(
        left,
        instance
            .arguments
            .len()
            .saturating_add(life.nominal_arguments.len()),
        span,
    )?;
    if decl.id.compiler && enum_address::poll_declaration(decl) {
        enum_address::poll_instance(instance, left, limits, phase)?;
        active.pop();
        return Ok(Vec::new());
    }
    if decl.id.compiler {
        for actual in &instance.arguments {
            if let LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. } =
                actual
            {
                let work = inference::type_snapshot_work(
                    ty,
                    span,
                    &Limits {
                        max_analysis_steps: *left,
                        ..limits.clone()
                    },
                )?;
                fee(left, work, span)?;
                if let LifeNominalActual::QualifiedType {
                    ty: ty @ Type::Address { .. },
                    life,
                } = actual
                {
                    qualified_address_shape(ty, life, left, span)?;
                }
            }
        }
        let work = inference::type_snapshot_work(
            storage,
            span,
            &Limits {
                max_analysis_steps: *left,
                ..limits.clone()
            },
        )?;
        fee(left, work, span)?;
        if !option_address::shape(
            &instance.ty,
            left,
            limits.max_call_depth,
            span,
            phase == InventoryPhase::Core,
        )? || !matches!((instance.arguments.as_slice(), storage.as_ref()),
                ([LifeNominalActual::Type(actual) | LifeNominalActual::QualifiedType { ty: actual, .. }], Type::Enum { variants, .. })
                if actual == &variants[1].1[0])
        {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "builtin Option actual, source-class map or physical payload differs",
            ));
        }
        active.pop();
        return Ok(Vec::new());
    }
    physical_instances(module, storage, limits, left)?;
    enum_address::arguments(instance, left, span)?;
    let mut expected = Vec::new();
    // All direct lifetimes precede recursive Type actuals, regardless of their ordinal.
    for (ordinal, (formal, actual)) in decl.parameters.iter().zip(&instance.arguments).enumerate() {
        match (formal, actual) {
            (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(class))
                if *class < life.binder_count =>
            {
                fee(left, 1, span)?;
                expected.push(NominalLifeArgument {
                    path: Vec::new(),
                    declaration: decl.id,
                    argument: ordinal as u32,
                    binder: *class,
                });
            }
            (
                LifeParameterKind::Type,
                LifeNominalActual::Type(_) | LifeNominalActual::QualifiedType { .. },
            ) => {}
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
                return Err(error(
                    "E-STORED-IDENTITY",
                    span,
                    "nominal metadata actual kind or class differs from full formal",
                ));
            }
        }
    }
    for (ordinal, actual) in instance.arguments.iter().enumerate() {
        let (LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. }) = actual
        else {
            continue;
        };
        let work = inference::type_snapshot_work(
            ty,
            span,
            &Limits {
                max_analysis_steps: *left,
                ..limits.clone()
            },
        )?;
        fee(left, work, span)?;
        if let LifeNominalActual::QualifiedType {
            ty: ty @ Type::Address { .. },
            life,
        } = actual
        {
            qualified_address_shape(ty, life, left, span)?;
        }
        if enum_address::candidate(&instance.ty)
            && matches!(ty, Type::Address { mutable: false, .. })
        {
            continue;
        }
        if !matches!(ty, Type::BorrowedStorage { .. }) {
            if !context_free_copy(ty, left, phase)? {
                return Err(error(
                    "E-STORED-IDENTITY",
                    span,
                    "non-nominal borrowed Type actual requires its semantic metadata consumer",
                ));
            }
            continue;
        }
        // Full equality resolves actual semantic identity; names and row claims never select a child.
        let mut selected = None;
        for child in &module.life_instances {
            let work = inference::type_snapshot_work(
                &child.ty,
                span,
                &Limits {
                    max_analysis_steps: *left,
                    ..limits.clone()
                },
            )?;
            fee(left, work, span)?;
            let work = inference::type_snapshot_work(
                ty,
                span,
                &Limits {
                    max_analysis_steps: *left,
                    ..limits.clone()
                },
            )?;
            fee(left, work, span)?;
            if child.ty == *ty && selected.replace(child).is_some() {
                return Err(error(
                    "E-STORED-IDENTITY",
                    span,
                    "Type actual has ambiguous complete nominal instances",
                ));
            }
        }
        let child = selected.ok_or_else(|| {
            error(
                "E-STORED-IDENTITY",
                span,
                "Type actual has no complete nominal instance",
            )
        })?;
        let child_rows = walk(module, child, limits, left, active, phase)?;
        let Type::BorrowedStorage {
            life: child_life, ..
        } = &child.ty
        else {
            unreachable!()
        };
        let mut mapping = Vec::<(u32, u32)>::new();
        for child_row in child_rows {
            let row = life.nominal_arguments.get(expected.len()).ok_or_else(|| {
                error(
                    "E-STORED-IDENTITY",
                    span,
                    "nested nominal lifetime row is absent",
                )
            })?;
            fee(
                left,
                row.path
                    .len()
                    .saturating_add(child_row.path.len())
                    .saturating_add(mapping.len())
                    .saturating_add(2),
                span,
            )?;
            let mut path = Vec::with_capacity(child_row.path.len().saturating_add(1));
            path.push(LifeTypeProjection::TypeArgument(ordinal as u32));
            path.extend_from_slice(&child_row.path);
            if row.path != path
                || row.declaration != child_row.declaration
                || row.argument != child_row.argument
                || row.binder >= life.binder_count
            {
                return Err(error(
                    "E-STORED-IDENTITY",
                    span,
                    "nested nominal row differs from actual Type argument graph",
                ));
            }
            if let Some((_, target)) = mapping.iter().find(|(class, _)| *class == child_row.binder)
            {
                if *target != row.binder {
                    return Err(error(
                        "E-STORED-IDENTITY",
                        span,
                        "one Type argument local class maps inconsistently",
                    ));
                }
            } else {
                fee(left, 1, span)?;
                mapping.push((child_row.binder, row.binder));
            }
            fee(left, 1, span)?;
            expected.push(NominalLifeArgument {
                path,
                declaration: child_row.declaration,
                argument: child_row.argument,
                binder: row.binder,
            });
        }
        fee(left, mapping.len().saturating_add(1), span)?;
        if mapping.len() != child_life.binder_count as usize {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "Type argument has a fixed class without a complete nominal metadata occurrence",
            ));
        }
    }
    if expected.len() != life.nominal_arguments.len() {
        return Err(error(
            "E-STORED-IDENTITY",
            span,
            "nominal lifetime metadata coverage differs from complete actual graph",
        ));
    }
    for (actual, row) in expected.iter().zip(&life.nominal_arguments) {
        fee(
            left,
            actual
                .path
                .len()
                .saturating_add(row.path.len())
                .saturating_add(1),
            span,
        )?;
        if actual != row {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "nominal metadata order, declaration, ordinal or class differs",
            ));
        }
    }
    active.pop();
    Ok(expected)
}

fn context_free_copy(ty: &Type, left: &mut usize, phase: InventoryPhase) -> R<bool> {
    let span = Span::default();
    fee(left, 1, span)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        fee(left, 1, span)?;
        match ty {
            Type::Named(name) if phase == InventoryPhase::Core => {
                // This is pending name resolution, never Copy authority.
                // The mandatory Resolved pass admits no Named actual.
                fee(left, name.len(), span)?;
            }
            Type::BorrowedFunctionPointer { .. } | Type::BorrowedStorage { .. } => {
                return Ok(false);
            }
            Type::Tuple(fields) => {
                fee(left, fields.len(), span)?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } => {
                fee(left, fields.len(), span)?;
                pending.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    fee(left, fields.len().saturating_add(1), span)?;
                    pending.extend(fields);
                }
            }
            Type::Array { element, .. } => {
                fee(left, 1, span)?;
                pending.push(element);
            }
            Type::ErrorUnion { error, result } => {
                fee(left, 2, span)?;
                pending.push(error);
                pending.push(result);
            }
            Type::FunctionPointer { parameters, result } => {
                fee(left, parameters.len().saturating_add(1), span)?;
                pending.extend(parameters);
                pending.push(result);
            }
            other if other.is_copy_value() => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub(super) fn same_key(
    a: &LifeNominalInstance,
    c: &LifeNominalInstance,
    limits: &Limits,
    left: &mut usize,
) -> R<bool> {
    let span = Span::default();
    fee(
        left,
        a.arguments
            .len()
            .saturating_add(c.arguments.len())
            .saturating_add(1),
        span,
    )?;
    for actual in a.arguments.iter().chain(&c.arguments) {
        match actual {
            LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. } => {
                let cost = inference::type_snapshot_work(
                    ty,
                    span,
                    &Limits {
                        max_analysis_steps: *left,
                        ..limits.clone()
                    },
                )?;
                fee(left, cost, span)?;
                if let LifeNominalActual::QualifiedType { life, .. } = actual {
                    fee(
                        left,
                        life.slots
                            .len()
                            .saturating_add(life.retained.len())
                            .saturating_add(life.nominal_arguments.len()),
                        span,
                    )?;
                    for n in life
                        .slots
                        .iter()
                        .map(|r| r.path.len())
                        .chain(life.retained.iter().map(|r| r.path.len()))
                        .chain(life.nominal_arguments.iter().map(|r| r.path.len()))
                    {
                        fee(left, n, span)?;
                    }
                }
            }
            LifeNominalActual::Const(Constant::WideString(data)) => {
                fee(left, data.len(), span)?;
            }
            _ => {}
        }
    }
    let (Type::BorrowedStorage { life: al, .. }, Type::BorrowedStorage { life: cl, .. }) =
        (&a.ty, &c.ty)
    else {
        // Plain N0 has no additional class-relation rows. A qualified N0 and
        // plain proposal for the same full actual key are still conflicting
        // semantic types, rather than two independent instantiations.
        return Ok(a.declaration == c.declaration && a.arguments == c.arguments);
    };
    fee(
        left,
        al.nominal_arguments
            .len()
            .saturating_add(cl.nominal_arguments.len()),
        span,
    )?;
    for row in al.nominal_arguments.iter().chain(&cl.nominal_arguments) {
        fee(left, row.path.len().saturating_add(1), span)?;
    }
    // The Type actual's local scope alone omits the enclosing occurrence's
    // class relation. Include the independently checked nominal relation.
    Ok(a.declaration == c.declaration
        && a.arguments == c.arguments
        && al.nominal_arguments == cl.nominal_arguments)
}

fn physical_instances(
    module: &CoreModule,
    storage: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let span = Span::default();
    fee(left, 1, span)?;
    let mut todo = vec![storage];

    while let Some(ty) = todo.pop() {
        fee(left, 1, span)?;
        match ty {
            Type::BorrowedStorage { .. } => {
                let mut count = 0usize;
                for child in &module.life_instances {
                    let work = inference::type_snapshot_work(
                        ty,
                        span,
                        &Limits {
                            max_analysis_steps: *left,
                            ..limits.clone()
                        },
                    )?;
                    fee(left, work, span)?;
                    let work = inference::type_snapshot_work(
                        &child.ty,
                        span,
                        &Limits {
                            max_analysis_steps: *left,
                            ..limits.clone()
                        },
                    )?;
                    fee(left, work, span)?;
                    if child.ty == *ty {
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(error(
                        "E-STORED-IDENTITY",
                        span,
                        "physical retained nominal child lacks a unique complete instance",
                    ));
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fee(left, fields.len(), span)?;
                todo.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Tuple(fields) => {
                fee(left, fields.len(), span)?;
                todo.extend(fields);
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    fee(left, fields.len().saturating_add(1), span)?;
                    todo.extend(fields);
                }
            }
            Type::Array { element, .. } => {
                fee(left, 1, span)?;
                todo.push(element);
            }
            _ => {}
        }
    }
    Ok(())
}

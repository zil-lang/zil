//! Type qualification for inline stored borrowing. Runtime target provenance is
//! reconstructed independently by Analyze and Verify, never by these checks.
use super::*;

mod nominal;
mod retained;

fn fee(left: &mut usize, amount: usize, span: Span) -> R<()> {
    *left = left.checked_sub(amount).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "stored type qualification budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

pub(super) fn physical(ty: &Type) -> &Type {
    match ty {
        Type::BorrowedStorage { storage, .. } => storage,
        _ => ty,
    }
}

// Match the actual declaration and its canonical resolved spelling. Never
// recover declaration authority by parsing a generated nominal name.
pub(super) fn aggregate<'a>(
    aggregates: &'a [CoreAggregate],
    module: zil_base::ModuleId,
    name: &str,
    left: &mut usize,
) -> R<&'a CoreAggregate> {
    for aggregate in aggregates {
        fee(
            left,
            aggregate
                .name
                .len()
                .saturating_add(name.len())
                .saturating_add(40),
            aggregate.span,
        )?;
        if name == aggregate.name || name == format!("@module{}:{}", module.0, aggregate.name) {
            return Ok(aggregate);
        }
    }
    Err(error(
        "E-STORED-IDENTITY",
        Span::default(),
        "stored instance has no actual aggregate declaration",
    ))
}

pub(super) fn inventory(core: &CoreModule, left: &mut usize, limits: &Limits) -> R<()> {
    if core.life_instances.is_empty() && core.life_declarations.is_empty() {
        return Ok(());
    }
    let span = Span::default();
    let cost = inference::life_inventory_snapshot_work(
        core,
        &Limits {
            max_analysis_steps: *left,
            ..limits.clone()
        },
    )?;
    fee(left, cost, span)?;
    let mut declarations = BTreeMap::new();
    for (index, declaration) in core.life_declarations.iter().enumerate() {
        fee(left, declarations.len().saturating_add(1), span)?;
        if declaration.id.compiler {
            return Err(unsupported(
                span,
                "stored compiler nominal requires its authentic builtin declaration",
            ));
        }
        if declaration.identity.package.is_empty()
            || declaration.identity.declaration_path.is_empty()
            || declaration
                .identity
                .module_path
                .iter()
                .chain(&declaration.identity.declaration_path)
                .any(String::is_empty)
        {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "stored nominal declaration identity is incomplete",
            ));
        }
        for earlier in &core.life_declarations[..index] {
            for identity in [&earlier.identity, &declaration.identity] {
                fee(
                    left,
                    identity
                        .package
                        .len()
                        .saturating_add(identity.generation_path.len())
                        .saturating_add(1),
                    span,
                )?;
                for part in identity
                    .module_path
                    .iter()
                    .chain(&identity.declaration_path)
                {
                    fee(left, part.len().saturating_add(1), span)?;
                }
            }
            if declaration.identity == earlier.identity {
                return Err(error(
                    "E-STORED-IDENTITY",
                    span,
                    "duplicate actual stored nominal declaration identity",
                ));
            }
        }
        if declarations.insert(declaration.id, declaration).is_some() {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "duplicate stored nominal declaration",
            ));
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for instance in &core.life_instances {
        fee(left, declarations.len().saturating_add(1), span)?;
        let declaration = declarations.get(&instance.declaration).ok_or_else(|| {
            error(
                "E-STORED-IDENTITY",
                span,
                "stored instance declaration is absent",
            )
        })?;
        let Type::BorrowedStorage { storage, .. } = &instance.ty else {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "stored instance is unqualified",
            ));
        };
        let Type::Record { name, fields } = storage.as_ref() else {
            return Err(unsupported(
                span,
                "stored nominal kind requires its consumer",
            ));
        };
        if declaration.kind != LifeNominalKind::Record
            || declaration.parameters.len() != instance.arguments.len()
        {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "stored instance kind or full argument arity differs",
            ));
        }
        fee(
            left,
            name.len()
                .saturating_add(1)
                .saturating_mul(names.len().saturating_add(1)),
            span,
        )?;
        if !names.insert(name) {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "multiple stored instances claim the same actual declaration layout",
            ));
        }
        let actual = aggregate(&core.aggregates, core.module_id, name, left)?;
        if actual.kind != AggregateKind::Struct || actual.fields.len() != fields.len() {
            return Err(error(
                "E-STORED-TYPE",
                actual.span,
                "stored instance differs from actual aggregate kind or layout",
            ));
        }
        for ((name, ty), field) in fields.iter().zip(&actual.fields) {
            fee(
                left,
                name.len().saturating_add(field.name.len()),
                field.span,
            )?;
            for checked in [ty, &field.ty] {
                let cost = inference::type_snapshot_work(
                    checked,
                    field.span,
                    &Limits {
                        max_analysis_steps: *left,
                        ..limits.clone()
                    },
                )?;
                fee(left, cost, field.span)?;
            }
            if name != &field.name || ty != &field.ty {
                return Err(error(
                    "E-STORED-TYPE",
                    field.span,
                    "stored instance physical fields differ from actual declaration",
                ));
            }
        }
        nominal::rows(core, instance, limits, left)?;
    }
    Ok(())
}

pub(super) fn copy_inventory(
    core: &CoreModule,
    left: &mut usize,
    limits: &Limits,
) -> R<(Vec<LifeNominalDeclaration>, Vec<LifeNominalInstance>)> {
    let work = inference::life_inventory_snapshot_work(
        core,
        &Limits {
            max_analysis_steps: *left,
            ..limits.clone()
        },
    )?;
    fee(left, work, Span::default())?;
    Ok((core.life_declarations.clone(), core.life_instances.clone()))
}

fn closed_copy(ty: &Type, left: &mut usize, depth: usize, limit: usize, span: Span) -> R<bool> {
    fee(left, 1, span)?;
    if depth > limit {
        return Err(Diagnostic::limit(
            Phase::Types,
            "stored pointee depth exceeded",
            Some(span),
        ));
    }
    Ok(match ty {
        Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::U8
        | Type::U16
        | Type::F32
        | Type::F64 => true,
        Type::Tuple(fields) => {
            let mut accepted = true;
            for field in fields {
                accepted &= closed_copy(field, left, depth + 1, limit, span)?;
            }
            accepted
        }
        Type::Record { name, fields } => {
            fee(left, name.len(), span)?;
            let mut accepted = true;
            for (name, field) in fields {
                fee(left, name.len(), span)?;
                accepted &= closed_copy(field, left, depth + 1, limit, span)?;
            }
            accepted
        }
        Type::Array { element, .. } => closed_copy(element, left, depth + 1, limit, span)?,
        Type::Enum { name, variants } => {
            fee(left, name.len(), span)?;
            let mut accepted = true;
            for (name, fields) in variants {
                fee(left, name.len(), span)?;
                for field in fields {
                    accepted &= closed_copy(field, left, depth + 1, limit, span)?;
                }
            }
            accepted
        }
        Type::ErrorUnion { error, result } => {
            closed_copy(error, left, depth + 1, limit, span)?
                & closed_copy(result, left, depth + 1, limit, span)?
        }
        _ => false,
    })
}

pub(super) fn shared_value(ty: &Type) -> bool {
    // Builder only receives Resolver output. Its paid shape check admits this
    // shared finite profile and binds it to the actual nominal inventory.
    // Do not recursively repeat that qualification without debiting work at
    // every expression lookup. This private dispatch tag grants no raw Copy
    // capability; both kernels reconstruct the actual targets independently.
    matches!(ty, Type::BorrowedStorage { .. })
}

pub(super) fn shared_address(ty: &Type) -> bool {
    matches!(ty, Type::Address { inner, .. } if shared_value(inner))
}

/// Exactly cover the actual shared Address leaves. Physical selectors cannot
/// traverse a pointee or a nested independently qualified carrier.
pub(super) fn shape(ty: &Type, left: &mut usize, depth_limit: usize, span: Span) -> R<()> {
    let Type::BorrowedStorage { storage, life } = ty else {
        return Err(error(
            "E-STORED-TYPE",
            span,
            "stored instance lacks its qualified type",
        ));
    };
    if !matches!(storage.as_ref(), Type::Record { .. })
        || (!life.retained.is_empty() && !life.slots.is_empty())
    {
        return Err(unsupported(
            span,
            "stored nested/opaque carrier qualification requires its consumer",
        ));
    }
    struct Walk<'a> {
        left: &'a mut usize,
        limit: usize,
        span: Span,
        slots: Vec<Vec<PlaceProjection>>,
    }
    impl Walk<'_> {
        fn ty(&mut self, ty: &Type, path: &mut Vec<PlaceProjection>, depth: usize) -> R<()> {
            fee(self.left, 1, self.span)?;
            if depth > self.limit {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "stored layout depth exceeded",
                    Some(self.span),
                ));
            }
            match ty {
                Type::Address {
                    mutable: false,
                    inner,
                } => {
                    let pointee = match inner.as_ref() {
                        Type::Slice { element } => element.as_ref(),
                        other => other,
                    };
                    if !closed_copy(pointee, self.left, depth + 1, self.limit, self.span)? {
                        return Err(unsupported(
                            self.span,
                            "stored pointee needs its full borrowing/ownership consumer",
                        ));
                    }
                    fee(self.left, path.len(), self.span)?;
                    self.slots.push(path.clone());
                }
                Type::Record { name, fields } => {
                    fee(self.left, name.len(), self.span)?;
                    for (index, (name, field)) in fields.iter().enumerate() {
                        fee(self.left, name.len().saturating_add(1), self.span)?;
                        let index = u32::try_from(index).map_err(|_| {
                            error("E-STORED-TYPE", self.span, "stored field ordinal overflow")
                        })?;
                        path.push(PlaceProjection::Field(index));
                        self.ty(field, path, depth + 1)?;
                        path.pop();
                    }
                }
                Type::Tuple(fields) => {
                    for (index, field) in fields.iter().enumerate() {
                        let index = u32::try_from(index).map_err(|_| {
                            error("E-STORED-TYPE", self.span, "stored tuple ordinal overflow")
                        })?;
                        path.push(PlaceProjection::Field(index));
                        self.ty(field, path, depth + 1)?;
                        path.pop();
                    }
                }
                Type::BorrowedStorage { .. } => {
                    if !retained::pure(ty, self.span, self.left)? {
                        return Err(unsupported(
                            self.span,
                            "retained physical leaf needs a real borrowing consumer",
                        ));
                    }
                }
                other => {
                    if !closed_copy(other, self.left, depth + 1, self.limit, self.span)? {
                        return Err(unsupported(
                            self.span,
                            "stored field ownership or reference mode requires its consumer",
                        ));
                    }
                }
            }
            Ok(())
        }
    }
    let mut walk = Walk {
        left,
        limit: depth_limit,
        span,
        slots: Vec::new(),
    };
    walk.ty(storage, &mut Vec::new(), 0)?;
    fee(
        walk.left,
        life.slots
            .len()
            .saturating_add(life.nominal_arguments.len()),
        span,
    )?;
    if walk.slots.len() != life.slots.len() {
        return Err(error(
            "E-STORED-TYPE",
            span,
            "stored shape must cover every physical address exactly once",
        ));
    }
    let mut classes = std::collections::BTreeSet::new();
    for row in &life.nominal_arguments {
        fee(
            walk.left,
            row.path
                .len()
                .saturating_add(classes.len())
                .saturating_add(1),
            span,
        )?;
        if !classes.contains(&row.binder) && row.binder as usize != classes.len() {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "nominal lifetime classes are not canonical",
            ));
        }
        classes.insert(row.binder);
    }
    for (path, slot) in walk.slots.iter().zip(&life.slots) {
        fee(
            walk.left,
            path.len()
                .saturating_add(slot.path.len())
                .saturating_add(classes.len())
                .saturating_add(1),
            span,
        )?;
        if path != &slot.path || slot.mode != StoredBorrowMode::SharedAddress {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "stored slot differs from actual physical leaf",
            ));
        }
        if !classes.contains(&slot.binder) && slot.binder as usize != classes.len() {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "stored lifetime classes are not canonical",
            ));
        }
        classes.insert(slot.binder);
    }
    let actual_retained = retained::expected(storage, depth_limit, span, walk.left)?;
    fee(walk.left, life.retained.len(), span)?;
    if actual_retained.len() != life.retained.len() {
        return Err(error(
            "E-STORED-TYPE",
            span,
            "physical retained class coverage differs",
        ));
    }
    for ((path, class), row) in actual_retained.iter().zip(&life.retained) {
        fee(
            walk.left,
            path.len()
                .saturating_add(row.path.len())
                .saturating_add(classes.len())
                .saturating_add(1),
            span,
        )?;
        if path != &row.path || *class != row.class || row.binder >= life.binder_count {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "retained row differs from actual physical boundary/class",
            ));
        }
        if !classes.contains(&row.binder) && row.binder as usize != classes.len() {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "retained classes are not canonical",
            ));
        }
        classes.insert(row.binder);
    }
    if classes.is_empty() || classes.len() != life.binder_count as usize {
        return Err(error(
            "E-STORED-TYPE",
            span,
            "stored lifetime domain is empty, sparse or has unused classes",
        ));
    }
    Ok(())
}

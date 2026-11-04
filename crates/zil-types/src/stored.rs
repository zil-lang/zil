//! Type qualification for inline stored borrowing. Runtime target provenance is
//! reconstructed independently by Analyze and Verify, never by these checks.
use super::*;

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
    for declaration in &core.life_declarations {
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
        let Type::BorrowedStorage { storage, life } = &instance.ty else {
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
        for aggregate in &core.aggregates {
            fee(
                left,
                aggregate
                    .name
                    .len()
                    .saturating_add(name.len())
                    .saturating_add(1),
                aggregate.span,
            )?;
        }
        let actual = core
            .aggregates
            .iter()
            .find(|aggregate| aggregate.name == *name)
            .ok_or_else(|| {
                error(
                    "E-STORED-IDENTITY",
                    span,
                    "stored instance has no actual aggregate declaration",
                )
            })?;
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
        let mut rows = life.nominal_arguments.iter();
        for (ordinal, (parameter, argument)) in declaration
            .parameters
            .iter()
            .zip(&instance.arguments)
            .enumerate()
        {
            fee(left, 1, span)?;
            match (parameter, argument) {
                (LifeParameterKind::Type, LifeNominalActual::Type(_)) => {}
                (LifeParameterKind::Const(ty), LifeNominalActual::Const(value))
                    if *ty == value.ty() => {}
                (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(class)) => {
                    let row = rows.next().ok_or_else(|| {
                        error(
                            "E-STORED-IDENTITY",
                            span,
                            "nominal lifetime actual occurrence is absent",
                        )
                    })?;
                    if !row.path.is_empty()
                        || row.declaration != instance.declaration
                        || row.argument as usize != ordinal
                        || row.binder != *class
                        || *class >= life.binder_count
                    {
                        return Err(error(
                            "E-STORED-IDENTITY",
                            span,
                            "nominal lifetime occurrence differs from full actual position",
                        ));
                    }
                }
                _ => {
                    return Err(error(
                        "E-STORED-IDENTITY",
                        span,
                        "stored nominal full argument kind differs",
                    ));
                }
            }
        }
        if rows.next().is_some() {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "extra nominal lifetime actual occurrence",
            ));
        }
    }
    Ok(())
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
    let Type::BorrowedStorage { storage, life } = ty else {
        return false;
    };
    fn copy_layout(ty: &Type) -> bool {
        match ty {
            Type::Address {
                mutable: false,
                inner,
            } => {
                inner.is_copy_value()
                    || matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value())
            }
            Type::Record { fields, .. } => fields.iter().all(|(_, ty)| copy_layout(ty)),
            Type::Tuple(fields) => fields.iter().all(copy_layout),
            _ => ty.is_copy_value(),
        }
    }
    life.retained.is_empty()
        && life
            .slots
            .iter()
            .all(|slot| slot.mode == StoredBorrowMode::SharedAddress)
        && copy_layout(storage)
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
    if !matches!(storage.as_ref(), Type::Record { .. }) || !life.retained.is_empty() {
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
        if !row.path.is_empty() {
            return Err(unsupported(
                span,
                "nested nominal life actual needs its complete type context",
            ));
        }
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
    if classes.is_empty() || classes.len() != life.binder_count as usize {
        return Err(error(
            "E-STORED-TYPE",
            span,
            "stored lifetime domain is empty, sparse or has unused classes",
        ));
    }
    Ok(())
}

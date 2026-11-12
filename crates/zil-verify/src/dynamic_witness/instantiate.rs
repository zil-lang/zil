//! Fresh construction uses intrinsic terms. Existing Self/whole Type actuals
//! retain their complete value; neither path re-partitions by lexical images.
use super::*;

fn empty(ty: &Type, b: &mut Budget) -> Check<terms::Value> {
    dynamic_metadata::ty(ty, b)?;
    Ok(terms::Value {
        raw: DynTypeValue {
            ty: ty.clone(),
            life: StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes: vec![],
        },
        terms: vec![],
    })
}
fn actual_type(
    module: &TypedModule,
    method: bool,
    index: u32,
    args: &mut matching::Arguments<'_>,
    b: &mut Budget,
) -> Check<terms::Value> {
    if let Some(value) = args.terms.type_actual(method, index, b)? {
        dynamic_metadata::recipe(&value.raw, b)?;
        b.spend(value.terms.len())?;
        return Ok(value.clone());
    }
    let raw = match args.formal(index, method)? {
        DynGenericActual::QualifiedType(value) => {
            dynamic_metadata::recipe(value, b)?;
            value.clone()
        }
        DynGenericActual::Type(ty) => {
            values::closed(module, ty, b)?;
            empty(ty, b)?.raw
        }
        _ => return Err(bad("original Type formal has no complete actual type")),
    };
    let root = if method {
        terms::Root::MethodType(index)
    } else {
        terms::Root::DeclarationType(index)
    };
    let value = terms::Value::anchored(&raw, args.owner, root, b)?;
    args.terms.bind_type(method, index, &value, b)?;
    Ok(value)
}
fn bind(
    classes: &mut [Option<terms::LifeTerm>],
    intrinsic: u32,
    term: terms::LifeTerm,
    raw: &DynTypeValue,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(1)?;
    if raw.classes.get(intrinsic as usize) != Some(&term.image) {
        return Ok(false);
    }
    let slot = classes
        .get_mut(intrinsic as usize)
        .ok_or_else(|| bad("fresh nominal occurrence exceeds actual intrinsic domain"))?;
    match slot {
        Some(previous) if previous != &term => Ok(false),
        Some(_) => Ok(true),
        None => {
            *slot = Some(term);
            Ok(true)
        }
    }
}
fn child_raw(
    parent: &DynTypeValue,
    ty: &Type,
    life: &StorageLifeShape,
    ordinal: u32,
    b: &mut Budget,
) -> Check<DynTypeValue> {
    if life.binder_count as usize > b.limits.max_nodes {
        return Err(limit("fresh nominal child scope exceeds selected bound"));
    }
    dynamic_metadata::ty(ty, b)?;
    b.spend((life.binder_count as usize).saturating_add(parent.life.retained.len()))?;
    let mut classes = Vec::with_capacity(life.binder_count as usize);
    for class in 0..life.binder_count {
        let mut rows =
            parent.life.retained.iter().filter(|r| {
                r.path == [LifeTypeProjection::TypeArgument(ordinal)] && r.class == class
            });
        let row = rows
            .next()
            .ok_or_else(|| bad("fresh nominal child omits its actual TypeArgument occurrence"))?;
        if rows.next().is_some() {
            return Err(bad(
                "fresh nominal child repeats an actual TypeArgument occurrence",
            ));
        }
        classes.push(
            *parent
                .classes
                .get(row.binder as usize)
                .ok_or_else(|| bad("fresh nominal child image exceeds parent domain"))?,
        );
    }
    Ok(DynTypeValue {
        ty: ty.clone(),
        life: life.clone(),
        classes,
    })
}
fn nominal(
    module: &TypedModule,
    pattern: &DynSelfPattern,
    actual: &DynTypeValue,
    receiver: &terms::Value,
    args: &mut matching::Arguments<'_>,
    b: &mut Budget,
    depth: usize,
) -> Check<Option<terms::Value>> {
    dynamic_metadata::recipe(actual, b)?;
    b.spend(module.life_instances.len())?;
    let mut selected = None;
    for instance in &module.life_instances {
        dynamic_metadata::ty(&instance.ty, b)?;
        if instance.ty == actual.ty && selected.replace(instance).is_some() {
            return Err(bad("fresh nominal exact full Type key is ambiguous"));
        }
    }
    let instance =
        selected.ok_or_else(|| bad("fresh nominal has no exact full Type catalogue key"))?;
    if instance.declaration != pattern.declaration
        || instance.arguments.len() != pattern.arguments.len()
    {
        return Ok(None);
    }
    if actual.life.binder_count as usize > b.limits.max_nodes {
        return Err(limit(
            "fresh nominal intrinsic domain exceeds selected bound",
        ));
    }
    b.spend(actual.life.binder_count as usize)?;
    let mut assigned = vec![None; actual.life.binder_count as usize];
    for (ordinal, (pattern, argument)) in pattern
        .arguments
        .iter()
        .zip(&instance.arguments)
        .enumerate()
    {
        b.spend(1)?;
        match (pattern, argument) {
            (DynPatternActual::Life(pattern), LifeNominalActual::Lifetime(class)) => {
                let term = args.terms.life(*pattern, b)?;
                b.spend(actual.life.nominal_arguments.len())?;
                let mut rows = actual.life.nominal_arguments.iter().filter(|r| {
                    r.path.is_empty()
                        && r.declaration == instance.declaration
                        && r.argument as usize == ordinal
                });
                let row = rows.next().ok_or_else(|| {
                    bad("fresh nominal lacks an original full lifetime occurrence")
                })?;
                if rows.next().is_some() || row.binder != *class {
                    return Err(bad(
                        "fresh nominal original lifetime occurrence differs from its full Type instance",
                    ));
                }
                if !bind(&mut assigned, *class, term, actual, b)? {
                    return Ok(None);
                }
            }
            (DynPatternActual::Const(pattern), LifeNominalActual::Const(value)) => {
                if matching::constant(module, pattern, &value.ty(), args, b)? != *value {
                    return Ok(None);
                }
            }
            (DynPatternActual::Type(pattern), LifeNominalActual::Type(ty)) => {
                let child = if let Type::BorrowedStorage { life, .. } = ty {
                    child_raw(actual, ty, life, ordinal as u32, b)?
                } else {
                    values::closed(module, ty, b)?;
                    empty(ty, b)?.raw
                };
                let Some(value) = walk(module, pattern, &child, receiver, args, b, depth + 1)?
                else {
                    return Ok(None);
                };
                if !assign_child(actual, ordinal as u32, &value, &mut assigned, b)? {
                    return Ok(None);
                }
            }
            (DynPatternActual::Type(pattern), LifeNominalActual::QualifiedType { ty, life }) => {
                let child = child_raw(actual, ty, life, ordinal as u32, b)?;
                let Some(value) = walk(module, pattern, &child, receiver, args, b, depth + 1)?
                else {
                    return Ok(None);
                };
                if !assign_child(actual, ordinal as u32, &value, &mut assigned, b)? {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        }
    }
    let terms = assigned
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            bad("fresh nominal has an intrinsic class without a real substitution origin")
        })?;
    let (partition, unique) = terms::partition(&terms, b)?;
    // A fresh constructor may merge only equal intrinsic origins. It cannot
    // merge distinct a/b because images coincide, or split repeated a/a.
    if unique.len() != terms.len() || partition.iter().enumerate().any(|(i, p)| *p as usize != i) {
        return Ok(None);
    }
    Ok(Some(terms::Value {
        raw: actual.clone(),
        terms,
    }))
}
fn assign_child(
    parent: &DynTypeValue,
    ordinal: u32,
    child: &terms::Value,
    assigned: &mut [Option<terms::LifeTerm>],
    b: &mut Budget,
) -> Check<bool> {
    for (class, term) in child.terms.iter().enumerate() {
        b.spend(parent.life.retained.len().saturating_add(1))?;
        let mut rows = parent.life.retained.iter().filter(|r| {
            r.path == [LifeTypeProjection::TypeArgument(ordinal)] && r.class as usize == class
        });
        let row = rows
            .next()
            .ok_or_else(|| bad("fresh nominal omits a complete child origin occurrence"))?;
        if rows.next().is_some() {
            return Err(bad("fresh nominal repeats a child origin occurrence"));
        }
        if !bind(assigned, row.binder, *term, parent, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn walk(
    module: &TypedModule,
    pattern: &DynTypePattern,
    actual: &DynTypeValue,
    receiver: &terms::Value,
    args: &mut matching::Arguments<'_>,
    b: &mut Budget,
    depth: usize,
) -> Check<Option<terms::Value>> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("fresh type substitution depth exceeded"));
    }
    let expected = match pattern {
        DynTypePattern::SelfType => {
            dynamic_metadata::recipe(&receiver.raw, b)?;
            receiver.clone()
        }
        DynTypePattern::Parameter(i) | DynTypePattern::MethodParameter(i) => actual_type(
            module,
            matches!(pattern, DynTypePattern::MethodParameter(_)),
            *i,
            args,
            b,
        )?,
        DynTypePattern::Concrete(ty) => {
            values::closed(module, ty, b)?;
            empty(ty, b)?
        }
        DynTypePattern::Nominal(pattern) => {
            return nominal(module, pattern, actual, receiver, args, b, depth + 1);
        }
        DynTypePattern::AssociatedProjection { .. } => {
            return Err(bad(
                "fresh type substitution requires its complete associated query",
            ));
        }
        _ => {
            return Err(bad(
                "fresh structural constructor requires its complete intrinsic term projection",
            ));
        }
    };
    dynamic_metadata::recipe(actual, b)?;
    Ok((expected.raw == *actual).then_some(expected))
}
pub(super) fn against(
    module: &TypedModule,
    pattern: &DynTypePattern,
    actual: &DynTypeValue,
    receiver: &DynTypeValue,
    args: &mut matching::Arguments<'_>,
    b: &mut Budget,
) -> Check<terms::Value> {
    dynamic_metadata::recipe(pattern, b)?;
    let receiver = terms::Value::anchored(receiver, args.owner, terms::Root::Receiver, b)?;
    walk(module, pattern, actual, &receiver, args, b, 0)?.ok_or_else(|| {
        bad("fresh recipe differs from the full intrinsic type constructed by its origin terms")
    })
}

//! Alpha alignment of an ordinary external wrapper domain. The complete inner
//! semantic Type and all child intrinsic selectors are kept unchanged.
use super::*;

fn pair(
    map: &mut [Option<u32>],
    reverse: &mut [Option<u32>],
    from: u32,
    to: u32,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(1)?;
    let a = map
        .get_mut(from as usize)
        .ok_or_else(|| bad("external occurrence binder exceeds its complete domain"))?;
    let z = reverse
        .get_mut(to as usize)
        .ok_or_else(|| bad("external target binder exceeds its complete domain"))?;
    if a.is_some_and(|known| known != to) || z.is_some_and(|known| known != from) {
        return Ok(false);
    }
    *a = Some(to);
    *z = Some(from);
    Ok(true)
}
/// Returns from-index -> to-index. This never aligns an embedded qualified or
/// opaque intrinsic domain, and it never infers equality from class images.
pub(super) fn alignment(
    from: &DynTypeValue,
    to: &DynTypeValue,
    b: &mut Budget,
) -> Check<Option<Vec<u32>>> {
    dynamic_metadata::recipe(from, b)?;
    dynamic_metadata::recipe(to, b)?;
    if from.ty != to.ty || from.life.binder_count != to.life.binder_count {
        return Ok(None);
    }
    if !matches!(
        from.ty,
        Type::Address { .. } | Type::Desc(_) | Type::Managed(_)
    ) {
        return Ok((from.life == to.life).then(|| (0..from.life.binder_count).collect()));
    }
    canonical_indices(from, b)?;
    canonical_indices(to, b)?;
    let length = from.life.binder_count as usize;
    if length > b.limits.max_nodes {
        return Err(limit("external alpha domain exceeds selected bound"));
    }
    b.spend(length.saturating_mul(2))?;
    let mut map = vec![None; length];
    let mut reverse = vec![None; length];
    if from.life.slots.len() != to.life.slots.len()
        || from.life.retained.len() != to.life.retained.len()
        || from.life.nominal_arguments.len() != to.life.nominal_arguments.len()
    {
        return Ok(None);
    }
    for a in &from.life.slots {
        b.spend(to.life.slots.len())?;
        let mut rows = to
            .life
            .slots
            .iter()
            .filter(|z| a.path == z.path && a.mode == z.mode);
        let Some(z) = rows.next() else {
            return Ok(None);
        };
        if rows.next().is_some() {
            return Err(bad("external alpha slot key is duplicated"));
        }
        if !pair(&mut map, &mut reverse, a.binder, z.binder, b)? {
            return Ok(None);
        }
    }
    for a in &from.life.retained {
        b.spend(to.life.retained.len())?;
        let mut rows = to
            .life
            .retained
            .iter()
            .filter(|z| a.path == z.path && a.class == z.class);
        let Some(z) = rows.next() else {
            return Ok(None);
        };
        if rows.next().is_some() {
            return Err(bad("external alpha child selector is duplicated"));
        }
        if !pair(&mut map, &mut reverse, a.binder, z.binder, b)? {
            return Ok(None);
        }
    }
    for a in &from.life.nominal_arguments {
        b.spend(to.life.nominal_arguments.len())?;
        let mut rows = to.life.nominal_arguments.iter().filter(|z| {
            a.path == z.path && a.declaration == z.declaration && a.argument == z.argument
        });
        let Some(z) = rows.next() else {
            return Ok(None);
        };
        if rows.next().is_some() {
            return Err(bad(
                "external alpha original nominal selector is duplicated",
            ));
        }
        if !pair(&mut map, &mut reverse, a.binder, z.binder, b)? {
            return Ok(None);
        }
    }
    if map.contains(&None) || reverse.contains(&None) {
        return Err(bad("external alpha domain has an unrepresented class"));
    }
    Ok(Some(map.into_iter().map(Option::unwrap).collect()))
}
pub(super) fn equal(from: &DynTypeValue, to: &DynTypeValue, b: &mut Budget) -> Check<bool> {
    let Some(map) = alignment(from, to, b)? else {
        return Ok(false);
    };
    if from.classes.len() != map.len() || to.classes.len() != map.len() {
        return Err(bad("external alpha image directory is incomplete"));
    }
    Ok(map
        .iter()
        .enumerate()
        .all(|(i, j)| from.classes[i] == to.classes[*j as usize]))
}
/// Complete origin equality after alpha alignment. Slot/child/formal identity
/// determines the bijection; repeated outside images never determine it.
pub(super) fn terms_equal(from: &terms::Value, to: &terms::Value, b: &mut Budget) -> Check<bool> {
    let Some(map) = alignment(&from.raw, &to.raw, b)? else {
        return Ok(false);
    };
    if from.terms.len() != map.len()
        || to.terms.len() != map.len()
        || from.raw.classes.len() != map.len()
        || to.raw.classes.len() != map.len()
    {
        return Err(bad("external alpha origin directory is incomplete"));
    }
    Ok(map.iter().enumerate().all(|(i, j)| {
        from.terms[i] == to.terms[*j as usize]
            && from.raw.classes.get(i) == to.raw.classes.get(*j as usize)
    }))
}

/// Stable private origin labels for an external wrapper. Only outer variable
/// numbering is normalized; complete inner type/class selectors are untouched.
pub(super) fn canonical_indices(value: &DynTypeValue, b: &mut Budget) -> Check<Vec<u32>> {
    let length = value.life.binder_count as usize;
    if length > b.limits.max_nodes {
        return Err(limit("external origin domain exceeds selected bound"));
    }
    if !matches!(
        value.ty,
        Type::Address { .. } | Type::Desc(_) | Type::Managed(_)
    ) {
        b.spend(length)?;
        return Ok((0..value.life.binder_count).collect());
    }
    let total = value
        .life
        .slots
        .len()
        .saturating_add(value.life.retained.len())
        .saturating_add(value.life.nominal_arguments.len());
    let paths = value
        .life
        .slots
        .iter()
        .map(|r| r.path.len().saturating_add(1))
        .chain(
            value
                .life
                .retained
                .iter()
                .map(|r| r.path.len().saturating_add(1)),
        )
        .chain(
            value
                .life
                .nominal_arguments
                .iter()
                .map(|r| r.path.len().saturating_add(1)),
        )
        .fold(0usize, usize::saturating_add);
    b.spend(
        paths
            .saturating_mul(total.checked_ilog2().unwrap_or(0) as usize + 2)
            .saturating_add(length),
    )?;
    let mut slots: Vec<_> = value.life.slots.iter().collect();
    slots.sort_by(|a, z| (&a.path, a.mode).cmp(&(&z.path, z.mode)));
    if slots
        .windows(2)
        .any(|rows| rows[0].path == rows[1].path && rows[0].mode == rows[1].mode)
    {
        return Err(bad("external origin has duplicate physical slot selectors"));
    }
    let mut retained: Vec<_> = value.life.retained.iter().collect();
    retained.sort_by(|a, z| (&a.path, a.class).cmp(&(&z.path, z.class)));
    if retained
        .windows(2)
        .any(|rows| rows[0].path == rows[1].path && rows[0].class == rows[1].class)
    {
        return Err(bad("external origin has duplicate whole-child selectors"));
    }
    let mut nominal: Vec<_> = value.life.nominal_arguments.iter().collect();
    nominal.sort_by(|a, z| {
        (&a.path, a.declaration, a.argument).cmp(&(&z.path, z.declaration, z.argument))
    });
    if nominal.windows(2).any(|rows| {
        rows[0].path == rows[1].path
            && rows[0].declaration == rows[1].declaration
            && rows[0].argument == rows[1].argument
    }) {
        return Err(bad(
            "external origin has duplicate original formal selectors",
        ));
    }
    let mut indices = vec![None; length];
    let mut next = 0u32;
    for binder in slots
        .iter()
        .map(|r| r.binder)
        .chain(retained.iter().map(|r| r.binder))
        .chain(nominal.iter().map(|r| r.binder))
    {
        let slot = indices
            .get_mut(binder as usize)
            .ok_or_else(|| bad("external origin selector is outside its domain"))?;
        if slot.is_none() {
            *slot = Some(next);
            next += 1;
        }
    }
    indices
        .into_iter()
        .map(|index| index.ok_or_else(|| bad("external origin class has no actual occurrence")))
        .collect()
}
/// Preserve an already constructed meaning while adopting an alpha-equivalent
/// external representation. No class image or intrinsic child index changes.
pub(super) fn in_shape(
    value: &terms::Value,
    actual: &DynTypeValue,
    b: &mut Budget,
) -> Check<Option<terms::Value>> {
    let Some(map) = alignment(&value.raw, actual, b)? else {
        return Ok(None);
    };
    if value.terms.len() != map.len()
        || value.classes.len() != map.len()
        || actual.classes.len() != map.len()
    {
        return Err(bad("external substitution class/origin coverage differs"));
    }
    b.spend(map.len())?;
    let mut terms = vec![None; map.len()];
    for (from, to) in map.into_iter().enumerate() {
        if value.classes[from] != actual.classes[to as usize] {
            return Ok(None);
        }
        terms[to as usize] = Some(value.terms[from]);
    }
    dynamic_metadata::recipe(actual, b)?;
    Ok(Some(terms::Value {
        raw: actual.clone(),
        terms: terms
            .into_iter()
            .map(|term| term.ok_or_else(|| bad("external substitution loses an origin")))
            .collect::<Check<Vec<_>>>()?,
    }))
}

pub(super) fn actuals_equal(
    from: &[DynGenericActual],
    to: &[DynGenericActual],
    b: &mut Budget,
) -> Check<bool> {
    b.spend(from.len().saturating_add(to.len()))?;
    if from.len() != to.len() {
        return Ok(false);
    }
    for (a, z) in from.iter().zip(to) {
        dynamic_metadata::recipe(a, b)?;
        dynamic_metadata::recipe(z, b)?;
        let same = match (a, z) {
            (DynGenericActual::QualifiedType(a), DynGenericActual::QualifiedType(z)) => {
                equal(a, z, b)?
            }
            (DynGenericActual::Type(a), DynGenericActual::QualifiedType(z))
            | (DynGenericActual::QualifiedType(z), DynGenericActual::Type(a)) => {
                *a == z.ty
                    && z.life.binder_count == 0
                    && z.life.slots.is_empty()
                    && z.life.retained.is_empty()
                    && z.life.nominal_arguments.is_empty()
                    && z.classes.is_empty()
            }
            _ => a == z,
        };
        if !same {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn applications_equal(
    from: &DynTraitApplication,
    to: &DynTraitApplication,
    b: &mut Budget,
) -> Check<bool> {
    Ok(from.declaration == to.declaration && actuals_equal(&from.actuals, &to.actuals, b)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn values() -> (DynTypeValue, DynTypeValue) {
        let declaration = LifeDeclarationId {
            module: zil_base::ModuleId(0),
            declaration: zil_base::NodeId(9),
            compiler: false,
        };
        let inner_life = StorageLifeShape {
            binder_count: 1,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![NominalLifeArgument {
                path: vec![],
                declaration,
                argument: 0,
                binder: 0,
            }],
        };
        let ty = Type::Address {
            mutable: false,
            inner: Box::new(Type::BorrowedStorage {
                storage: Box::new(Type::Record {
                    name: "Tag".into(),
                    fields: vec![("code".into(), Type::I32)],
                }),
                life: inner_life,
            }),
        };
        let a = DynTypeValue {
            ty,
            life: StorageLifeShape {
                binder_count: 2,
                slots: vec![StorageLifeSlot {
                    path: vec![],
                    mode: StoredBorrowMode::SharedAddress,
                    binder: 0,
                }],
                retained: vec![StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0,
                    binder: 1,
                }],
                nominal_arguments: vec![NominalLifeArgument {
                    path: vec![LifeTypeProjection::Pointee],
                    declaration,
                    argument: 0,
                    binder: 1,
                }],
            },
            classes: vec![StorageBinder::Fixed(0), StorageBinder::Fixed(1)],
        };
        let mut z = a.clone();
        z.life.slots[0].binder = 1;
        z.life.retained[0].binder = 0;
        z.life.nominal_arguments[0].binder = 0;
        z.classes.swap(0, 1);
        (a, z)
    }
    #[test]
    fn external_alpha_uses_complete_occurrence_bijection_and_preserves_intrinsic_types() {
        let (mut a, mut z) = values();
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        assert!(equal(&a, &z, &mut b).unwrap());
        let owner = terms::Owner::Function(FunctionId(0));
        let first = terms::Value::anchored(&a, owner, terms::Root::Receiver, &mut b).unwrap();
        let second = terms::Value::anchored(&z, owner, terms::Root::Receiver, &mut b).unwrap();
        assert!(terms_equal(&first, &second, &mut b).unwrap());
        a.classes.fill(StorageBinder::Fixed(0));
        z.classes.fill(StorageBinder::Fixed(0));
        assert!(equal(&a, &z, &mut b).unwrap());
        let mut compressed = a.clone();
        compressed.life.binder_count = 1;
        compressed.life.retained[0].binder = 0;
        compressed.life.nominal_arguments[0].binder = 0;
        compressed.classes.pop();
        assert!(!equal(&a, &compressed, &mut b).unwrap());
        let mut wrong = z.clone();
        wrong.life.retained[0].class = 1;
        assert!(!equal(&a, &wrong, &mut b).unwrap());
        let mut wrong = z.clone();
        wrong.life.nominal_arguments[0].argument = 1;
        assert!(!equal(&a, &wrong, &mut b).unwrap());
        let mut wrong = z.clone();
        let Type::Address { inner, .. } = &mut wrong.ty else {
            unreachable!()
        };
        let Type::BorrowedStorage { storage, .. } = inner.as_mut() else {
            unreachable!()
        };
        let Type::Record { name, .. } = storage.as_mut() else {
            unreachable!()
        };
        *name = "OtherTag".into();
        assert!(!equal(&a, &wrong, &mut b).unwrap());
        let mut duplicate = z;
        duplicate.life.slots.push(duplicate.life.slots[0].clone());
        let mut duplicate_a = a;
        duplicate_a
            .life
            .slots
            .push(duplicate_a.life.slots[0].clone());
        assert!(equal(&duplicate_a, &duplicate, &mut b).is_err());
    }
    #[test]
    fn opaque_fixed_order_is_not_an_external_alpha_domain_and_work_is_prepaid() {
        let (a, z) = values();
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        assert!(equal(&a, &z, &mut b).unwrap());
        let cost = limits.max_analysis_steps - b.remaining;
        let mut short = limits.clone();
        short.max_analysis_steps = cost - 1;
        let error = equal(&a, &z, &mut Budget::new(&short)).unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        let mut fresh = limits.clone();
        fresh.max_analysis_steps = cost;
        assert!(equal(&a, &z, &mut Budget::new(&fresh)).unwrap());
        let mut opaque = DynTypeValue {
            ty: Type::Closure(ClosureId(0)),
            life: StorageLifeShape {
                binder_count: 2,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes: vec![StorageBinder::Fixed(0), StorageBinder::Fixed(1)],
        };
        let saved = opaque.clone();
        opaque.classes.swap(0, 1);
        assert!(!equal(&saved, &opaque, &mut Budget::new(&limits)).unwrap());
    }
}

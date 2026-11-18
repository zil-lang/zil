//! Retained Task ports are opaque signature occurrences, not ordinary address
//! slots in the Task payload. Transport them through the exact whole input's
//! class relation; never create an address or a runtime loan from a port type.
use super::*;

fn domain(ty: &Type, depth: usize, limits: &Limits, left: &mut usize) -> R<Option<usize>> {
    path(left, limits, depth)?;
    match ty {
        Type::Task { retained_life, .. } => Ok(Some(retained_life.binder_count as usize)),
        Type::Managed(inner) => domain(inner, depth + 1, limits, left),
        _ => Ok(None),
    }
}
// A transparent Managed child has a complete whole-Type domain, but is not
// a second signature boundary. Its exact child->parent forest rows transport
// the domain down Pointee; only the actual Task's local class directory emits.
fn transport_managed_rows(
    intrinsic: &[LifeTypeProjection], images: &[usize], life: &StorageLifeShape,
    transported: &mut [bool], limits: &Limits, left: &mut usize,
) -> R<()> {
    if images.len() > limits.max_nodes {
        return Err(Diagnostic::limit(Phase::Types, "Managed Task domain exceeds node limit", None));
    }
    pay(left, images.len().saturating_add(life.retained.len()))?;
    let mut seen = vec![false; images.len()];
    for (ordinal, row) in life.retained.iter().enumerate() {
        path(left, limits, row.path.len())?;
        if row.path.as_slice() != intrinsic { continue; }
        let class = row.class as usize;
        let image = images.get(class)
            .ok_or_else(|| bad("Managed Task transport exceeds its complete child ordinal"))?;
        if row.binder as usize != *image || seen[class] {
            return Err(bad("Managed Task transport changes one exact child-to-parent mapping"));
        }
        seen[class] = true;
        let represented = transported.get_mut(ordinal)
            .ok_or_else(|| bad("Managed Task transport loses its original forest coordinate"))?;
        *represented = true;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn leaf(
    ty: &Type,
    images: &[usize],
    source: &[u32],
    base: StorageBase,
    physical: &mut Vec<ContractProjection>,
    intrinsic: &mut Vec<LifeTypeProjection>,
    bindings: &mut Vec<(StoragePosition, u32)>,
    classes: &mut Vec<(SignatureLifeClass, u32)>,
    covered: &mut [bool],
    life: &StorageLifeShape,
    expanded: &mut [bool],
    limits: &Limits,
    left: &mut usize,
    depth: usize,
) -> R<()> {
    path(left, limits, depth)?;
    if let Type::Managed(inner) = ty {
        transport_managed_rows(intrinsic, images, life, expanded, limits, left)?;
        intrinsic.push(LifeTypeProjection::Pointee);
        let result = leaf(
            inner,
            images,
            source,
            base,
            physical,
            intrinsic,
            bindings,
            classes,
            covered,
            life,
            expanded,
            limits,
            left,
            depth + 1,
        );
        intrinsic.pop();
        return result;
    }
    let Type::Task { retained_life, .. } = ty else {
        return Err(bad(
            "Task header transport changes its actual sealed payload",
        ));
    };
    if images.len() != retained_life.binder_count as usize {
        return Err(bad("Task header omits its complete intrinsic input domain"));
    }
    pay(left, retained_life.ports.len().saturating_add(images.len()))?;
    for (port, row) in retained_life.ports.iter().enumerate() {
        let index = *images
            .get(row.binder as usize)
            .ok_or_else(|| bad("Task header port exceeds its complete input domain"))?;
        let image = source
            .get(index)
            .ok_or_else(|| bad("Task header port leaves its original whole input"))?;
        path(left, limits, physical.len().saturating_add(1))?;
        pay(left, 1)?;
        let mut position = physical.clone();
        position.push(ContractProjection::RetainedPort(
            u32::try_from(port).map_err(|_| bad("Task header port ordinal overflows"))?,
        ));
        bindings.push((
            StoragePosition {
                base,
                path: position,
            },
            image.clone(),
        ));
        covered[index] = true;
    }
    // Ports need not cover phantom Context classes. Keep every class and its
    // full intrinsic ordinal, independently of physical port occurrences.
    for (class, index) in images.iter().copied().enumerate() {
        let image = source
            .get(index)
            .ok_or_else(|| bad("Task intrinsic class leaves its original whole input"))?;
        path(left, limits, intrinsic.len())?;
        pay(left, 1)?;
        classes.push((
            SignatureLifeClass {
                base,
                path: intrinsic.clone(),
                class: u32::try_from(class)
                    .map_err(|_| bad("Task header class ordinal overflows"))?,
            },
            image.clone(),
        ));
        covered[index] = true;
        // Preserve this exact Task boundary. Transparent Managed rows were
        // independently checked while descending; they remain in the full
        // value forest, but are not additional signature boundary rows.
        pay(left, life.retained.len())?;
        let mut represented = None;
        for (ordinal, row) in life.retained.iter().enumerate() {
            path(left, limits, row.path.len())?;
            if row.path != *intrinsic || row.class as usize != class { continue; }
            if row.binder as usize != index || represented.replace(ordinal).is_some() {
                return Err(bad("Task header child forest conflicts at one complete intrinsic coordinate"));
            }
        }
        if let Some(row) = represented { expanded[row] = true; }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn child(
    ty: &Type,
    projection: LifeTypeProjection,
    selector: Option<ContractProjection>,
    life: &StorageLifeShape,
    source: &[u32],
    base: StorageBase,
    bindings: &mut Vec<(StoragePosition, u32)>,
    classes: &mut Vec<(SignatureLifeClass, u32)>,
    covered: &mut [bool],
    expanded: &mut [bool],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let Some(count) = domain(ty, 0, limits, left)? else {
        return Ok(());
    };
    if count > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "Task header intrinsic domain exceeds node limit",
            None,
        ));
    }
    pay(left, count)?;
    let mut images = Vec::with_capacity(count);
    for class in 0..count {
        pay(left, life.retained.len())?;
        let mut selected = None;
        for (index, row) in life.retained.iter().enumerate() {
            path(left, limits, row.path.len())?;
            if row.path.as_slice() != std::slice::from_ref(&projection)
                || row.class as usize != class
            {
                continue;
            }
            if selected.replace((index, row.binder as usize)).is_some() {
                return Err(bad("Task header child has ambiguous original class images"));
            }
        }
        let (_, image) = selected
            .ok_or_else(|| bad("Task header child omits an original retained class image"))?;
        if image >= source.len() {
            return Err(bad("Task header child exceeds its full original domain"));
        }
        images.push(image);
    }
    let mut physical = selector.into_iter().collect::<Vec<_>>();
    let mut intrinsic = vec![projection];
    leaf(
        ty,
        &images,
        source,
        base,
        &mut physical,
        &mut intrinsic,
        bindings,
        classes,
        covered,
        life,
        expanded,
        limits,
        left,
        0,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn append(
    ty: &Type,
    life: &StorageLifeShape,
    source: &[u32],
    base: StorageBase,
    bindings: &mut Vec<(StoragePosition, u32)>,
    classes: &mut Vec<(SignatureLifeClass, u32)>,
    covered: &mut [bool],
    expanded: &mut [bool],
    limits: &Limits,
    left: &mut usize,
) -> R<bool> {
    pay(left, 1)?;
    match ty {
        Type::Task { .. } => {
            pay(left, source.len())?;
            let images = (0..source.len()).collect::<Vec<_>>();
            leaf(
                ty,
                &images,
                source,
                base,
                &mut Vec::new(),
                &mut Vec::new(),
                bindings,
                classes,
                covered,
                life,
                expanded,
                limits,
                left,
                0,
            )?;
            Ok(true)
        }
        Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => {
            child(
                inner,
                LifeTypeProjection::Pointee,
                None,
                life,
                source,
                base,
                bindings,
                classes,
                covered,
                expanded,
                limits,
                left,
            )?;
            Ok(false)
        }
        Type::Tuple(fields) => {
            pay(left, fields.len())?;
            for (index, field) in fields.iter().enumerate() {
                let index =
                    u32::try_from(index).map_err(|_| bad("Task input tuple ordinal overflows"))?;
                let projection = PlaceProjection::Field(index);
                child(
                    field,
                    LifeTypeProjection::Place(projection),
                    Some(ContractProjection::Place(projection)),
                    life,
                    source,
                    base,
                    bindings,
                    classes,
                    covered,
                    expanded,
                    limits,
                    left,
                )?;
            }
            Ok(false)
        }
        Type::ErrorUnion { error, result } => {
            for (is_error, arm) in [(true, error), (false, result)] {
                let projection = PlaceProjection::ErrorArm(is_error);
                child(
                    arm,
                    LifeTypeProjection::Place(projection),
                    Some(ContractProjection::Place(projection)),
                    life,
                    source,
                    base,
                    bindings,
                    classes,
                    covered,
                    expanded,
                    limits,
                    left,
                )?;
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_ports_preserve_occurrences_phantom_classes_and_parent_images() {
        let classes = (0..3)
            .map(|ordinal| SemanticClassExpr::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            })
            .collect::<Vec<_>>();
        let port = RetainedBorrowPort {
            leaf: Type::Address {
                mutable: true,
                inner: Box::new(Type::I32),
            },
            mode: StoredBorrowMode::ExclusiveAddress,
            binder: 0,
        };
        let value = SemanticTypeValue::Resolved {
            ty: Type::Address {
                mutable: false,
                inner: Box::new(Type::Managed(Box::new(Type::Task {
                    result: Box::new(Type::I32),
                    retained_life: RetainedLifeShape {
                        binder_count: 2,
                        ports: vec![port.clone(), port],
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                }))),
            },
            life: StorageLifeShape {
                binder_count: 3,
                slots: vec![StorageLifeSlot {
                    path: Vec::new(),
                    binder: 2,
                    mode: StoredBorrowMode::SharedAddress,
                }],
                retained: vec![
                    StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Pointee],
                        class: 0,
                        binder: 1,
                    },
                    StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Pointee],
                        class: 1,
                        binder: 0,
                    },
                ],
                nominal_arguments: Vec::new(),
            },
            classes: classes.clone(),
        };
        let mut bindings = Vec::new();
        let mut intrinsic = Vec::new();
        let mut statics = Vec::new();
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        super::super::append(
            &value,
            StorageBase::Parameter(0),
            &mut bindings,
            &mut intrinsic,
            &mut statics,
            &limits,
            &mut left,
        )
        .unwrap();
        assert_eq!(
            bindings,
            vec![
                (
                    StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: Vec::new()
                    },
                    classes[2].clone()
                ),
                (
                    StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![ContractProjection::RetainedPort(0)]
                    },
                    classes[1].clone()
                ),
                (
                    StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![ContractProjection::RetainedPort(1)]
                    },
                    classes[1].clone()
                ),
            ]
        );
        assert_eq!(
            intrinsic,
            vec![
                (
                    SignatureLifeClass {
                        base: StorageBase::Parameter(0),
                        path: vec![LifeTypeProjection::Pointee, LifeTypeProjection::Pointee],
                        class: 0
                    },
                    classes[1].clone()
                ),
                (
                    SignatureLifeClass {
                        base: StorageBase::Parameter(0),
                        path: vec![LifeTypeProjection::Pointee, LifeTypeProjection::Pointee],
                        class: 1
                    },
                    classes[0].clone()
                ),
            ]
        );
        assert!(statics.is_empty());
        // A repeated transparent selector is not a second permitted row.
        let mut repeated = value.clone();
        let SemanticTypeValue::Resolved { life, .. } = &mut repeated else { unreachable!() };
        life.retained.push(life.retained[0].clone());
        assert!(super::super::append(
            &repeated, StorageBase::Parameter(0), &mut Vec::new(), &mut Vec::new(),
            &mut Vec::new(), &limits, &mut left,
        ).is_err());
        // Even equal outside images cannot authorize a contradictory mapping
        // between the complete Managed domain and the actual Task child.
        let mut conflict = value.clone();
        let SemanticTypeValue::Resolved { life, classes, .. } = &mut conflict else { unreachable!() };
        classes[0] = classes[1].clone();
        life.retained.push(StorageRetainedBinding {
            path: vec![LifeTypeProjection::Pointee, LifeTypeProjection::Pointee],
            class: 0, binder: 0,
        });
        assert!(super::super::append(
            &conflict, StorageBase::Parameter(0), &mut Vec::new(), &mut Vec::new(),
            &mut Vec::new(), &limits, &mut left,
        ).is_err());
        let mut omitted = value;
        let SemanticTypeValue::Resolved { life, .. } = &mut omitted else {
            unreachable!()
        };
        life.retained.pop();
        assert!(
            super::super::append(
                &omitted,
                StorageBase::Parameter(0),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut Vec::new(),
                &limits,
                &mut left
            )
            .is_err()
        );
    }
}

//! Semantic class equations are not target origins or access paths.
use super::*;

pub(super) fn charge(c: &StorageLifeContract, b: &mut Budget) -> CompileResult<()> {
    let rows = c.bindings.len().saturating_add(c.classes.len());
    b.spend(
        (c.fixed_binder_count as usize)
            .saturating_add(c.invocation_binder_count as usize)
            .saturating_add(rows.saturating_mul(rows))
            .saturating_add(rows.saturating_mul(2))
            .saturating_add(2),
        "signature relation row reservation",
    )?;
    for row in &c.bindings {
        b.spend(row.position.path.len(), "signature actual borrow path")?;
    }
    for row in &c.classes {
        b.spend(row.position.path.len(), "signature semantic class path")?;
    }
    Ok(())
}

pub(super) fn outer(c: &StorageLifeContract, base: StorageBase) -> Option<StorageBinder> {
    c.bindings
        .iter()
        .find(|r| r.position.base == base && r.position.path.is_empty())
        .map(|r| r.binder)
}

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
) -> CompileResult<()> {
    if c.fixed_binder_count == 0 && c.invocation_binder_count == 0 {
        return Err(bad(
            "vacuous borrowed signature must use the ordinary callable type",
        ));
    }
    struct Cursor<'a> {
        c: &'a StorageLifeContract,
        borrow: usize,
        class: usize,
        fixed: u32,
        invocation: u32,
        grounded: Vec<bool>,
        inputs: Option<(u32, u32)>,
    }
    impl Cursor<'_> {
        fn binder(&mut self, b: StorageBinder, retained: bool) -> CompileResult<()> {
            let (id, count, next, input) = match b {
                StorageBinder::Fixed(i) => (
                    i,
                    self.c.fixed_binder_count,
                    &mut self.fixed,
                    self.inputs.map(|x| x.0),
                ),
                StorageBinder::Invocation(i) => (
                    i,
                    self.c.invocation_binder_count,
                    &mut self.invocation,
                    self.inputs.map(|x| x.1),
                ),
            };
            if id >= count || id > *next || input.is_some_and(|n| id >= n) {
                return Err(bad("signature class is noncanonical or result-only"));
            }
            if id == *next {
                *next += 1;
            }
            if retained && let StorageBinder::Fixed(i) = b {
                self.grounded[i as usize] = true;
            }
            Ok(())
        }
        fn ty(&mut self, ty: &Type, base: StorageBase) -> CompileResult<()> {
            let (target, pointed) = if let Type::Address { inner, .. } = ty {
                let row = self
                    .c
                    .bindings
                    .get(self.borrow)
                    .ok_or_else(|| bad("signature omits actual outer borrow"))?;
                if row.position.base != base || !row.position.path.is_empty() {
                    return Err(bad(
                        "signature borrow position is flattened or noncanonical",
                    ));
                }
                self.borrow += 1;
                self.binder(row.binder, false)?;
                (inner.as_ref(), true)
            } else {
                (ty, false)
            };
            if let Type::BorrowedStorage { storage, life } = target {
                if !matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                    || !life.retained.is_empty()
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "signature qualified type needs its owning/nested consumer",
                        None,
                    )]);
                }
                if life.binder_count == 0
                    || life.binder_count as usize > self.c.classes.len().saturating_sub(self.class)
                {
                    return Err(bad("signature omits qualified class domain"));
                }
                for local in 0..life.binder_count {
                    let row = &self.c.classes[self.class];
                    if row.position.base != base
                        || row.position.class != local
                        || if pointed {
                            row.position.path.as_slice() != [LifeTypeProjection::Pointee]
                        } else {
                            !row.position.path.is_empty()
                        }
                    {
                        return Err(bad(
                            "signature class must name the exact qualified type boundary",
                        ));
                    }
                    self.class += 1;
                    self.binder(row.binder, self.inputs.is_none())?;
                }
            } else if pointed {
                if !target.is_address_pointee() {
                    return Err(bad("signature pointee is outside the implemented profile"));
                }
            } else if !target.is_copy_value() {
                return Err(bad("signature value needs its ownership consumer"));
            }
            Ok(())
        }
    }
    let mut cursor = Cursor {
        c,
        borrow: 0,
        class: 0,
        fixed: 0,
        invocation: 0,
        grounded: vec![false; c.fixed_binder_count as usize],
        inputs: None,
    };
    for (i, p) in parameters.iter().enumerate() {
        let i = u32::try_from(i).map_err(|_| bad("signature parameter ordinal overflow"))?;
        cursor.ty(p, StorageBase::Parameter(i))?;
    }
    cursor.inputs = Some((cursor.fixed, cursor.invocation));
    cursor.ty(result, StorageBase::Result)?;
    if cursor.borrow != c.bindings.len()
        || cursor.class != c.classes.len()
        || cursor.fixed != c.fixed_binder_count
        || cursor.invocation != c.invocation_binder_count
        || cursor.grounded.iter().any(|g| !g)
    {
        return Err(bad(
            "signature relation has missing/extra rows or ungrounded Fixed classes",
        ));
    }
    // A declared Address result still needs an actual authority-bearing input;
    // the body proof separately checks the exact returned target.
    if let Type::Address { mutable, .. } = result {
        let out = outer(c, StorageBase::Result);
        if !parameters.iter().enumerate().any(|(i, p)| {
            matches!(p,Type::Address{mutable:m,..}if !mutable||*m)
                && outer(c, StorageBase::Parameter(i as u32)) == out
        }) && (*mutable
            || !c.classes.iter().any(|r| {
                matches!(r.position.base, StorageBase::Parameter(_)) && Some(r.binder) == out
            }))
        {
            return Err(bad(
                "signature Address result has no eligible declared input",
            ));
        }
    }
    Ok(())
}

/// Each equality required by the concrete target must hold in the pointer's
/// universally callable relation. Distinct variables are not disjointness.
pub(super) fn entails(required: &StorageLifeContract, supplied: &StorageLifeContract) -> bool {
    if required.bindings.len() != supplied.bindings.len()
        || required.classes.len() != supplied.classes.len()
    {
        return false;
    }
    if !required
        .bindings
        .iter()
        .zip(&supplied.bindings)
        .all(|(a, z)| a.position == z.position)
        || !required
            .classes
            .iter()
            .zip(&supplied.classes)
            .all(|(a, z)| a.position == z.position)
    {
        return false;
    }
    let rows = required
        .bindings
        .iter()
        .map(|r| r.binder)
        .chain(required.classes.iter().map(|r| r.binder));
    let actual = supplied
        .bindings
        .iter()
        .map(|r| r.binder)
        .chain(supplied.classes.iter().map(|r| r.binder));
    for (left, mapped) in rows.clone().zip(actual.clone()) {
        for (right, other) in rows.clone().zip(actual.clone()) {
            if left == right && mapped != other {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tag() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "Tag".into(),
                fields: vec![("n".into(), Type::I32)],
            }),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![NominalLifeArgument {
                    path: vec![],
                    declaration: LifeDeclarationId {
                        module: zil_base::ModuleId(0),
                        declaration: zil_base::NodeId(1),
                        compiler: false,
                    },
                    argument: 0,
                    binder: 0,
                }],
            },
        }
    }
    fn address(t: Type) -> Type {
        Type::Address {
            mutable: false,
            inner: Box::new(t),
        }
    }
    fn relation2(class: u32) -> StorageLifeContract {
        StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 2,
            bindings: (0..2)
                .map(|i| StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(i),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(i),
                })
                .collect(),
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Parameter(0),
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0,
                },
                binder: StorageBinder::Invocation(class),
            }],
        }
    }
    #[test]
    fn qualified_class_is_joint_with_outer_inputs_and_never_a_physical_borrow_row() {
        let p = vec![address(tag()), address(Type::I32)];
        let one = relation2(0);
        let two = relation2(1);
        relation(&p, &Type::I32, &one).unwrap();
        relation(&p, &Type::I32, &two).unwrap();
        assert!(!entails(&one, &two));
        assert!(!entails(&two, &one));
        assert!(entails(&one, &one));
        let mut flat = one.clone();
        flat.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
            },
            binder: StorageBinder::Invocation(0),
        });
        assert!(relation(&p, &Type::I32, &flat).is_err());
        let mut omitted = one;
        omitted.classes.clear();
        assert!(relation(&p, &Type::I32, &omitted).is_err());
    }
}

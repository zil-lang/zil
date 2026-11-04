//! Complete class rows for the first ordinary qualified callable signatures.
//! This checks declared type relations; body target and loan proofs stay in
//! the independent kernels. Qualified storage is a semantic boundary here.
use super::*;

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    life: &StorageLifeContract,
    span: Span,
) -> R<()> {
    struct Check<'a> {
        life: &'a StorageLifeContract,
        binding: usize,
        class: usize,
        next_fixed: u32,
        next_invocation: u32,
        input_fixed: u32,
        input_invocation: u32,
        grounded: Vec<bool>,
        span: Span,
    }
    impl Check<'_> {
        fn invalid(&self, message: &str) -> Diagnostic {
            error("E-FUNCTION-LIFE", self.span, message)
        }
        fn binder(&mut self, binder: StorageBinder, result: bool, input_class: bool) -> R<()> {
            let (index, count, next, inputs) = match binder {
                StorageBinder::Fixed(index) => (
                    index,
                    self.life.fixed_binder_count,
                    &mut self.next_fixed,
                    self.input_fixed,
                ),
                StorageBinder::Invocation(index) => (
                    index,
                    self.life.invocation_binder_count,
                    &mut self.next_invocation,
                    self.input_invocation,
                ),
            };
            if index >= count || index > *next || (result && index >= inputs) {
                return Err(self.invalid("noncanonical or ungrounded qualified callable binder"));
            }
            if index == *next {
                *next += 1;
            }
            if input_class && let StorageBinder::Fixed(index) = binder {
                self.grounded[index as usize] = true;
            }
            Ok(())
        }
        fn address(&mut self, base: StorageBase, result: bool) -> R<()> {
            let row = self
                .life
                .bindings
                .get(self.binding)
                .ok_or_else(|| self.invalid("missing actual outer address relation"))?;
            if row.position.base != base || !row.position.path.is_empty() {
                return Err(self.invalid("wrong or flattened qualified callable borrow position"));
            }
            self.binding += 1;
            self.binder(row.binder, result, false)
        }
        fn classes(
            &mut self,
            ty: &Type,
            base: StorageBase,
            pointee: bool,
            result: bool,
        ) -> R<bool> {
            let Type::BorrowedStorage { storage, life } = ty else {
                return Ok(false);
            };
            // This source/native lane has already checked the actual shared
            // Record shape. Other owning/nested/opaque lanes remain explicit.
            if !matches!(storage.as_ref(), Type::Record { .. })
                || !life.retained.is_empty()
                || life
                    .slots
                    .iter()
                    .any(|s| s.mode != StoredBorrowMode::SharedAddress)
            {
                return Err(unsupported(
                    self.span,
                    "qualified callable storage requires its owning or nested consumer",
                ));
            }
            let count = life.binder_count as usize;
            if count == 0 || count > self.life.classes.len().saturating_sub(self.class) {
                return Err(self.invalid("qualified callable class domain is absent or incomplete"));
            }
            for class in 0..count {
                let row = &self.life.classes[self.class];
                let path_matches = if pointee {
                    row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                } else {
                    row.position.path.is_empty()
                };
                if row.position.base != base
                    || row.position.class as usize != class
                    || !path_matches
                {
                    return Err(self.invalid(
                        "qualified callable class must select its actual complete type boundary",
                    ));
                }
                self.class += 1;
                self.binder(row.binder, result, !result)?;
            }
            Ok(true)
        }
        fn ty(&mut self, ty: &Type, base: StorageBase, result: bool) -> R<()> {
            match ty {
                Type::Address { inner, .. } => {
                    self.address(base, result)?;
                    if !self.classes(inner, base, true, result)? && !inner.is_address_pointee() {
                        return Err(unsupported(
                            self.span,
                            "qualified callable pointee needs its complete consumer",
                        ));
                    }
                }
                Type::BorrowedStorage { .. } => {
                    self.classes(ty, base, false, result)?;
                }
                ty if ty.is_copy_value() => {}
                _ => {
                    return Err(unsupported(
                        self.span,
                        "qualified callable value needs its complete ownership consumer",
                    ));
                }
            }
            Ok(())
        }
    }
    // Every caller prepays metadata_work before this count-bounded storage.
    // No Type/row/path clone is made by this relation checker.
    let mut check = Check {
        life,
        binding: 0,
        class: 0,
        next_fixed: 0,
        next_invocation: 0,
        input_fixed: 0,
        input_invocation: 0,
        grounded: vec![false; life.fixed_binder_count as usize],
        span,
    };
    for (index, parameter) in parameters.iter().enumerate() {
        let ordinal = u32::try_from(index)
            .map_err(|_| check.invalid("callable parameter ordinal overflow"))?;
        check.ty(parameter, StorageBase::Parameter(ordinal), false)?;
    }
    check.input_fixed = check.next_fixed;
    check.input_invocation = check.next_invocation;
    check.ty(result, StorageBase::Result, true)?;
    if check.binding != life.bindings.len()
        || check.class != life.classes.len()
        || check.next_fixed != life.fixed_binder_count
        || check.next_invocation != life.invocation_binder_count
        || check.grounded.iter().any(|grounded| !grounded)
    {
        return Err(
            check.invalid("qualified callable contains missing, redundant or ungrounded relations")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn phantom() -> Type {
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
    fn contract(second_class: bool) -> StorageLifeContract {
        let mut life = StorageLifeContract::from_flat(FunctionLifeContract {
            binder_count: 2,
            parameter_lives: vec![Some(0), Some(1)],
            result_life: None,
        })
        .unwrap();
        life.classes.push(SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Invocation(u32::from(second_class)),
        });
        life
    }
    #[test]
    fn qualified_phantom_class_is_distinct_from_the_outer_address_relation() {
        let parameters = vec![
            Type::Address {
                mutable: false,
                inner: Box::new(phantom()),
            },
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I64),
            },
        ];
        let same = contract(false);
        let different = contract(true);
        assert!(super::relation(&parameters, &Type::I32, &same, Span::default()).is_ok());
        assert!(super::relation(&parameters, &Type::I32, &different, Span::default()).is_ok());
        assert_ne!(same.fingerprint(), different.fingerprint());
        for n in 0..5 {
            let mut bad = same.clone();
            match n {
                0 => bad.classes[0].position.path = vec![LifeTypeProjection::TypeArgument(0)],
                1 => bad.classes[0].position.class = 1,
                2 => bad.classes[0].position.base = StorageBase::Parameter(1),
                3 => bad.classes.push(bad.classes[0].clone()),
                _ => bad.bindings.push(StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
                    },
                    binder: StorageBinder::Invocation(0),
                }),
            }
            assert!(
                super::relation(&parameters, &Type::I32, &bad, Span::default()).is_err(),
                "mutation {n}"
            );
        }
    }
}

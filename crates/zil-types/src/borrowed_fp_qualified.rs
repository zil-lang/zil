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
    relation_inner(parameters, None, result, life, span)
}

pub(super) fn capture_relation(
    parameters: &[Type],
    captures: &[Type],
    result: &Type,
    life: &StorageLifeContract,
    span: Span,
) -> R<()> {
    let scalar = |ty: &Type| {
        matches!(
            ty,
            Type::Unit
                | Type::Bool
                | Type::I32
                | Type::I64
                | Type::U64
                | Type::Usize
                | Type::U8
                | Type::U16
                | Type::F32
                | Type::F64
        )
    };
    if !scalar(result) || parameters.iter().any(|ty| matches!(ty, Type::Address { mutable: true, .. }))
        || captures.iter().any(|ty| !(scalar(ty) || matches!(ty, Type::Address { mutable: false, inner } if scalar(inner) || matches!(inner.as_ref(), Type::BorrowedStorage { .. })))) {
        return Err(unsupported(span, "captured Fixed requires scalar result and shared finite environment"));
    }
    relation_inner(parameters, Some(captures), result, life, span)
}

fn relation_inner(
    parameters: &[Type],
    captures: Option<&[Type]>,
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
        mutable_fixed: Vec<bool>,
        mutable_invocation: Vec<bool>,
        span: Span,
        captured: bool,
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
        fn address(&mut self, base: StorageBase, result: bool, mutable: bool) -> R<()> {
            let row = self
                .life
                .bindings
                .get(self.binding)
                .ok_or_else(|| self.invalid("missing actual outer address relation"))?;
            if row.position.base != base || !row.position.path.is_empty() {
                return Err(self.invalid("wrong or flattened qualified callable borrow position"));
            }
            self.binding += 1;
            let binder = row.binder;
            let capture = matches!(base, StorageBase::Capture(_));
            if capture && !matches!(binder, StorageBinder::Fixed(_)) {
                return Err(self.invalid("captured Address must retain a Fixed environment class"));
            }
            self.binder(binder, result, capture)?;
            let allowed = match binder {
                StorageBinder::Fixed(index) => &mut self.mutable_fixed[index as usize],
                StorageBinder::Invocation(index) => &mut self.mutable_invocation[index as usize],
            };
            if mutable && result && !*allowed {
                return Err(
                    self.invalid("exclusive callable result needs a matching mutable direct input")
                );
            }
            if mutable && !result {
                *allowed = true;
            }
            Ok(())
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
            if !(matches!(storage.as_ref(), Type::Record { .. })
                || self.captured && matches!(storage.as_ref(), Type::Tuple(_)))
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
                let capture = matches!(base, StorageBase::Capture(_));
                if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                    return Err(self.invalid("captured qualified class must be Fixed"));
                }
                self.binder(row.binder, result, capture || !self.captured && !result)?;
            }
            Ok(true)
        }
        fn ty(&mut self, ty: &Type, base: StorageBase, result: bool) -> R<()> {
            match ty {
                Type::Address { inner, mutable } => {
                    self.address(base, result, *mutable)?;
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
    let declared =
        (life.fixed_binder_count as usize).checked_add(life.invocation_binder_count as usize);
    let occurrences = life.bindings.len().checked_add(life.classes.len());
    if captures.is_none() && life.fixed_binder_count as usize > life.classes.len()
        || declared
            .zip(occurrences)
            .is_none_or(|(declared, occurrences)| declared > occurrences)
    {
        return Err(error(
            "E-FUNCTION-LIFE",
            span,
            "callable binder domain exceeds actual relation occurrences",
        ));
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
        mutable_fixed: vec![false; life.fixed_binder_count as usize],
        mutable_invocation: vec![false; life.invocation_binder_count as usize],
        span,
        captured: captures.is_some(),
    };
    for (index, parameter) in parameters.iter().enumerate() {
        let ordinal = u32::try_from(index)
            .map_err(|_| check.invalid("callable parameter ordinal overflow"))?;
        check.ty(parameter, StorageBase::Parameter(ordinal), false)?;
    }
    if let Some(captures) = captures {
        for (index, capture) in captures.iter().enumerate() {
            let ordinal =
                u32::try_from(index).map_err(|_| check.invalid("capture ordinal overflow"))?;
            check.ty(capture, StorageBase::Capture(ordinal), false)?;
        }
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
    fn capture_relation_is_joint_and_cannot_invent_or_freshen_fixed() {
        let ty = Type::Address {
            mutable: false,
            inner: Box::new(phantom()),
        };
        let mut life = contract(false);
        life.fixed_binder_count = 2;
        life.invocation_binder_count = 1;
        life.bindings[1].position.base = StorageBase::Capture(0);
        life.bindings[1].binder = StorageBinder::Fixed(1);
        life.classes[0].binder = StorageBinder::Fixed(0);
        let mut capture = life.classes[0].clone();
        capture.position.base = StorageBase::Capture(0);
        life.classes.push(capture);
        capture_relation(
            std::slice::from_ref(&ty),
            std::slice::from_ref(&ty),
            &Type::I32,
            &life,
            Span::default(),
        )
        .unwrap();
        for mutation in 0..5 {
            let mut invalid = life.clone();
            match mutation {
                0 => invalid.bindings[1].binder = StorageBinder::Invocation(0),
                1 => invalid.classes[1].binder = StorageBinder::Invocation(0),
                2 => invalid.classes[1].position.class = 1,
                3 => invalid.bindings.swap(0, 1),
                _ => invalid.classes[1].position.path = vec![],
            }
            assert!(
                capture_relation(
                    std::slice::from_ref(&ty),
                    std::slice::from_ref(&ty),
                    &Type::I32,
                    &invalid,
                    Span::default()
                )
                .is_err(),
                "mutation {mutation}"
            );
        }
        assert!(
            capture_relation(
                std::slice::from_ref(&ty),
                &[],
                &Type::I32,
                &life,
                Span::default()
            )
            .is_err()
        );
    }
    #[test]
    fn static_function_abstraction_keeps_distinct_domains_and_joint_occurrence_order() {
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
        let mut definition = contract(false);
        definition.fixed_binder_count = 1;
        definition.classes[0].binder = StorageBinder::Fixed(0);
        assert!(
            borrowed_fp::relation(&parameters, &Type::I32, &definition, Span::default()).is_err(),
            "a code value cannot invent the definition's enclosing Fixed context"
        );
        let original = definition.clone();
        let code = borrowed_fp::static_pointer_life(&parameters, &Type::I32, &definition).unwrap();
        assert_eq!(definition, original);
        assert_eq!(code.fixed_binder_count, 0);
        assert_eq!(code.invocation_binder_count, 3);
        assert_eq!(code.bindings[0].binder, StorageBinder::Invocation(0));
        assert_eq!(code.classes[0].binder, StorageBinder::Invocation(1));
        assert_eq!(code.bindings[1].binder, StorageBinder::Invocation(2));
        let mut tied = definition.clone();
        tied.bindings[0].binder = StorageBinder::Fixed(0);
        tied.bindings[1].binder = StorageBinder::Invocation(0);
        tied.invocation_binder_count = 1;
        let code = borrowed_fp::static_pointer_life(&parameters, &Type::I32, &tied).unwrap();
        assert_eq!(code.invocation_binder_count, 2);
        assert_eq!(code.classes[0].binder, code.bindings[0].binder);
        for counts in [(u32::MAX, 0), (0, u32::MAX)] {
            let mut malformed = definition.clone();
            malformed.fixed_binder_count = counts.0;
            malformed.invocation_binder_count = counts.1;
            assert!(relation(&parameters, &Type::I32, &malformed, Span::default()).is_err());
        }
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
        let mut returns_second = same.clone();
        returns_second.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
            binder: StorageBinder::Invocation(1),
        });
        let mutable_result = Type::Address {
            mutable: true,
            inner: Box::new(Type::I64),
        };
        assert!(
            super::relation(
                &parameters,
                &mutable_result,
                &returns_second,
                Span::default()
            )
            .is_err()
        );
        let mut mutable_parameters = parameters.clone();
        mutable_parameters[1] = mutable_result.clone();
        assert!(
            super::relation(
                &mutable_parameters,
                &mutable_result,
                &returns_second,
                Span::default()
            )
            .is_ok()
        );
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

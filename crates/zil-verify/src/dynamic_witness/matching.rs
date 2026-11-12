//! Original recipe matching occurs in one actual lexical domain, before any
//! table Context map. Full nominal intrinsic shapes are never coalesced here.
use super::*;

pub(super) struct Arguments<'a> {
    pub declaration: &'a [DynGenericActual],
    pub method: &'a [DynGenericActual],
    pub invocation: &'a [DynLifeEmbedding],
    pub implicit: Vec<Option<StorageBinder>>,
}
impl Arguments<'_> {
    fn life(
        &mut self,
        pattern: DynLifePattern,
        actual: StorageBinder,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?;
        let expected = match pattern {
            DynLifePattern::DeclarationParameter(i) => self.declaration.get(i as usize),
            DynLifePattern::MethodParameter(i) => self.method.get(i as usize),
            DynLifePattern::Invocation(i) => {
                b.spend(self.invocation.len())?;
                let mut rows = self
                    .invocation
                    .iter()
                    .filter(|row| row.from == StorageBinder::Invocation(i));
                let row = rows
                    .next()
                    .ok_or_else(|| bad("original Invocation has no actual header embedding"))?;
                if rows.next().is_some() || row.to != actual {
                    return Err(bad(
                        "original Invocation differs from its exact actual header occurrence",
                    ));
                }
                return Ok(());
            }
            DynLifePattern::ImplicitDeclaration(i) => {
                let slot = self.implicit.get_mut(i as usize).ok_or_else(|| {
                    bad("original implicit life is outside its declaration directory")
                })?;
                match slot {
                    Some(expected) if *expected != actual => {
                        return Err(bad(
                            "one original implicit life has inconsistent actual occurrences",
                        ));
                    }
                    None => *slot = Some(actual),
                    _ => {}
                }
                return Ok(());
            }
            DynLifePattern::Bound { .. } => {
                return Err(bad(
                    "bound life requires the fresh quantified obligation domain",
                ));
            }
        };
        if !matches!(expected, Some(DynGenericActual::Life(class)) if *class == actual) {
            return Err(bad(
                "original life formal differs from its exact actual occurrence",
            ));
        }
        Ok(())
    }
    fn formal(&self, index: u32, method: bool) -> Check<&DynGenericActual> {
        (if method {
            self.method
        } else {
            self.declaration
        })
        .get(index as usize)
        .ok_or_else(|| bad("original full formal ordinal has no actual argument"))
    }
}

fn nominal<'a>(
    module: &'a TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> Check<&'a LifeNominalInstance> {
    dynamic_metadata::ty(ty, b)?;
    b.spend(module.life_instances.len())?;
    let mut selected = None;
    for row in &module.life_instances {
        dynamic_metadata::ty(&row.ty, b)?;
        if row.ty == *ty && selected.replace(row).is_some() {
            return Err(bad(
                "full actual nominal type has ambiguous declaration instances",
            ));
        }
    }
    selected.ok_or_else(|| bad("full actual nominal type has no declaration instance"))
}
fn class(value: &DynTypeValue, intrinsic: u32, b: &mut Budget) -> Check<StorageBinder> {
    b.spend(1)?;
    value
        .classes
        .get(intrinsic as usize)
        .copied()
        .ok_or_else(|| bad("actual nominal occurrence has no complete intrinsic class map"))
}
fn nested(
    parent: &DynTypeValue,
    ty: &Type,
    life: &StorageLifeShape,
    path: LifeTypeProjection,
    b: &mut Budget,
) -> Check<DynTypeValue> {
    dynamic_metadata::ty(ty, b)?;
    if life.binder_count as usize > b.limits.max_nodes {
        return Err(limit("nested actual type scope exceeds selected bound"));
    }
    b.spend((life.binder_count as usize).saturating_add(parent.life.retained.len()))?;
    let mut classes = Vec::with_capacity(life.binder_count as usize);
    for intrinsic in 0..life.binder_count {
        let mut rows = parent
            .life
            .retained
            .iter()
            .filter(|row| row.path == [path] && row.class == intrinsic);
        let row = rows
            .next()
            .ok_or_else(|| bad("actual type argument omits a complete child class occurrence"))?;
        if rows.next().is_some() {
            return Err(bad("actual type argument repeats a child class occurrence"));
        }
        classes.push(class(parent, row.binder, b)?);
    }
    Ok(DynTypeValue {
        ty: ty.clone(),
        life: life.clone(),
        classes,
    })
}
fn empty(ty: &Type, b: &mut Budget) -> Check<DynTypeValue> {
    dynamic_metadata::ty(ty, b)?;
    Ok(DynTypeValue {
        ty: ty.clone(),
        life: StorageLifeShape {
            binder_count: 0,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
        classes: vec![],
    })
}

/// Match a complete actual value, preserving its intrinsic directory even when
/// two of its class images are the same lexical binder.
pub(super) fn ty(
    module: &TypedModule,
    pattern: &DynTypePattern,
    actual: &DynTypeValue,
    receiver: &DynTypeValue,
    args: &mut Arguments<'_>,
    b: &mut Budget,
) -> Check<()> {
    dynamic_metadata::recipe(pattern, b)?;
    walk(module, pattern, actual, receiver, args, b, 0)
}
fn walk(
    module: &TypedModule,
    pattern: &DynTypePattern,
    actual: &DynTypeValue,
    receiver: &DynTypeValue,
    args: &mut Arguments<'_>,
    b: &mut Budget,
    depth: usize,
) -> Check<()> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("original actual type matching depth exceeded"));
    }
    match pattern {
        DynTypePattern::SelfType => {
            dynamic_metadata::recipe(receiver, b)?;
            dynamic_metadata::recipe(actual, b)?;
            if receiver != actual {
                return Err(bad("Self recipe differs from the complete actual receiver"));
            }
        }
        DynTypePattern::Concrete(expected) => {
            values::closed(module, expected, b)?;
            if &actual.ty != expected || actual.life.binder_count != 0 || !actual.classes.is_empty()
            {
                return Err(bad("closed recipe differs from the complete actual type"));
            }
        }
        DynTypePattern::Parameter(i) | DynTypePattern::MethodParameter(i) => {
            let expected =
                args.formal(*i, matches!(pattern, DynTypePattern::MethodParameter(_)))?;
            dynamic_metadata::recipe(expected, b)?;
            match expected {
                DynGenericActual::Type(expected) => {
                    values::closed(module, expected, b)?;
                    if &actual.ty != expected
                        || actual.life.binder_count != 0
                        || !actual.classes.is_empty()
                    {
                        return Err(bad("original Type formal differs from its closed actual"));
                    }
                }
                DynGenericActual::QualifiedType(expected) if expected == actual => {}
                _ => {
                    return Err(bad(
                        "original Type formal differs from its full intrinsic type and lexical map",
                    ));
                }
            }
        }
        DynTypePattern::Nominal(pattern) => {
            let instance = nominal(module, &actual.ty, b)?;
            if instance.declaration != pattern.declaration
                || instance.arguments.len() != pattern.arguments.len()
            {
                return Err(bad(
                    "original nominal declaration or complete actual order differs",
                ));
            }
            for (ordinal, (pattern, argument)) in pattern
                .arguments
                .iter()
                .zip(&instance.arguments)
                .enumerate()
            {
                b.spend(1)?;
                match (pattern, argument) {
                    (DynPatternActual::Life(pattern), LifeNominalActual::Lifetime(intrinsic)) => {
                        // The actual declaration's full formal occurrence is
                        // independently bound, not inferred by reversing Context.
                        b.spend(actual.life.nominal_arguments.len())?;
                        let mut rows = actual.life.nominal_arguments.iter().filter(|row| {
                            row.path.is_empty()
                                && row.declaration == instance.declaration
                                && row.argument as usize == ordinal
                        });
                        let row = rows.next().ok_or_else(|| {
                            bad("actual original nominal life occurrence is absent")
                        })?;
                        if rows.next().is_some() || row.binder != *intrinsic {
                            return Err(bad(
                                "actual original nominal life occurrence differs from its full instance",
                            ));
                        }
                        args.life(*pattern, class(actual, *intrinsic, b)?, b)?;
                    }
                    (DynPatternActual::Type(pattern), LifeNominalActual::Type(child)) => {
                        let child = match child {
                            Type::BorrowedStorage { life, .. } => nested(
                                actual,
                                child,
                                life,
                                LifeTypeProjection::TypeArgument(ordinal as u32),
                                b,
                            )?,
                            _ => {
                                values::closed(module, child, b)?;
                                empty(child, b)?
                            }
                        };
                        walk(module, pattern, &child, receiver, args, b, depth + 1)?;
                    }
                    (
                        DynPatternActual::Type(pattern),
                        LifeNominalActual::QualifiedType { ty, life },
                    ) => {
                        let child = nested(
                            actual,
                            ty,
                            life,
                            LifeTypeProjection::TypeArgument(ordinal as u32),
                            b,
                        )?;
                        walk(module, pattern, &child, receiver, args, b, depth + 1)?;
                    }
                    (DynPatternActual::Const(pattern), LifeNominalActual::Const(actual)) => {
                        if constant(module, pattern, &actual.ty(), args, b)? != *actual {
                            return Err(bad(
                                "original nominal constant differs from its exact actual bits",
                            ));
                        }
                    }
                    _ => return Err(bad("original nominal full actual kind differs")),
                }
            }
        }
        DynTypePattern::AssociatedProjection { .. } => {
            return Err(bad(
                "associated type matching requires its complete active query graph",
            ));
        }
        _ => structural(module, pattern, actual, receiver, args, b, depth + 1)?,
    }
    Ok(())
}
fn intrinsic_child(
    module: &TypedModule,
    parent: &DynTypeValue,
    child: &Type,
    path: LifeTypeProjection,
    b: &mut Budget,
) -> Check<DynTypeValue> {
    match child {
        Type::BorrowedStorage { life, .. } => nested(parent, child, life, path, b),
        Type::Closure(_) | Type::Task { .. } | Type::Dyn { .. } | Type::OwnedDyn { .. } => {
            let domain = values::intrinsic(module, child, b)?;
            let life = StorageLifeShape {
                binder_count: domain.count,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            };
            nested(parent, child, &life, path, b)
        }
        _ => {
            values::closed(module, child, b)?;
            empty(child, b)
        }
    }
}
fn structural(
    module: &TypedModule,
    pattern: &DynTypePattern,
    actual: &DynTypeValue,
    receiver: &DynTypeValue,
    args: &mut Arguments<'_>,
    b: &mut Budget,
    depth: usize,
) -> Check<()> {
    let physical = match &actual.ty {
        Type::BorrowedStorage { storage, .. } => storage.as_ref(),
        ty => ty,
    };
    match (pattern, physical) {
        (
            DynTypePattern::Address {
                mutable,
                lifetime,
                inner,
            },
            Type::Address {
                mutable: actual_mut,
                inner: child,
            },
        ) if mutable == actual_mut => {
            let life = lifetime.ok_or_else(|| bad("original Address lifetime is Unknown"))?;
            if actual.life.slots.len() != 1 || !actual.life.slots[0].path.is_empty() {
                return Err(bad("actual Address outer occurrence is absent"));
            }
            args.life(life, class(actual, actual.life.slots[0].binder, b)?, b)?;
            let child = intrinsic_child(module, actual, child, LifeTypeProjection::Pointee, b)?;
            walk(module, inner, &child, receiver, args, b, depth)?;
        }
        (DynTypePattern::Desc { lifetime, inner }, Type::Desc(child)) => {
            let life = lifetime.ok_or_else(|| bad("original Description lifetime is Unknown"))?;
            if actual.life.slots.len() != 1 || !actual.life.slots[0].path.is_empty() {
                return Err(bad("actual Description outer occurrence is absent"));
            }
            args.life(life, class(actual, actual.life.slots[0].binder, b)?, b)?;
            let child = intrinsic_child(module, actual, child, LifeTypeProjection::Pointee, b)?;
            walk(module, inner, &child, receiver, args, b, depth)?;
        }
        (DynTypePattern::Managed(pattern), Type::Managed(child)) => {
            let child = intrinsic_child(module, actual, child, LifeTypeProjection::Pointee, b)?;
            walk(module, pattern, &child, receiver, args, b, depth)?;
        }
        (DynTypePattern::Tuple(patterns), Type::Tuple(fields))
            if patterns.len() == fields.len() =>
        {
            for (i, (pattern, child)) in patterns.iter().zip(fields).enumerate() {
                let child = intrinsic_child(
                    module,
                    actual,
                    child,
                    LifeTypeProjection::Place(PlaceProjection::Field(i as u32)),
                    b,
                )?;
                walk(module, pattern, &child, receiver, args, b, depth)?;
            }
        }
        (
            DynTypePattern::Array { element, length },
            Type::Array {
                element: child,
                length: actual_length,
            },
        ) => {
            if constant(module, length, &Type::Usize, args, b)?
                != Constant::Usize(u64::from(*actual_length))
            {
                return Err(bad(
                    "original array length differs from its actual typed length",
                ));
            }
            let child = intrinsic_child(module, actual, child, LifeTypeProjection::Element, b)?;
            walk(module, element, &child, receiver, args, b, depth)?;
        }
        (DynTypePattern::Slice(pattern), Type::Slice { element: child })
        | (DynTypePattern::OwnedStore(pattern), Type::OwnedStore { element: child }) => {
            let child = intrinsic_child(module, actual, child, LifeTypeProjection::Element, b)?;
            walk(module, pattern, &child, receiver, args, b, depth)?;
        }
        (
            DynTypePattern::ErrorUnion { error, result },
            Type::ErrorUnion {
                error: actual_error,
                result: actual_result,
            },
        ) => {
            for (arm, pattern, child) in [
                (true, error.as_ref(), actual_error.as_ref()),
                (false, result.as_ref(), actual_result.as_ref()),
            ] {
                let child = intrinsic_child(
                    module,
                    actual,
                    child,
                    LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)),
                    b,
                )?;
                walk(module, pattern, &child, receiver, args, b, depth)?;
            }
        }
        _ => {
            return Err(bad(
                "original constructor differs from the complete actual semantic type",
            ));
        }
    }
    Ok(())
}

/// Match the complete trait application in this same lexical scope. Actual
/// kinds follow the original mixed declaration order, including phantom lives.
pub(super) fn application(
    module: &TypedModule,
    pattern: &DynSelfPattern,
    actual: &DynTraitApplication,
    receiver: &DynTypeValue,
    args: &mut Arguments<'_>,
    b: &mut Budget,
) -> Check<()> {
    if pattern.declaration != actual.declaration || pattern.arguments.len() != actual.actuals.len()
    {
        return Err(bad(
            "original implemented trait differs from its complete actual application",
        ));
    }
    b.spend(pattern.arguments.len())?;
    for (pattern, actual) in pattern.arguments.iter().zip(&actual.actuals) {
        match (pattern, actual) {
            (DynPatternActual::Life(pattern), DynGenericActual::Life(actual)) => {
                args.life(*pattern, *actual, b)?
            }
            (DynPatternActual::Const(pattern), DynGenericActual::Const(actual)) => {
                if constant(module, pattern, &actual.ty(), args, b)? != *actual {
                    return Err(bad(
                        "original implemented trait constant differs from its actual bits",
                    ));
                }
            }
            (DynPatternActual::Type(pattern), DynGenericActual::QualifiedType(actual)) => {
                ty(module, pattern, actual, receiver, args, b)?;
            }
            (DynPatternActual::Type(pattern), DynGenericActual::Type(actual)) => {
                values::closed(module, actual, b)?;
                let actual = empty(actual, b)?;
                ty(module, pattern, &actual, receiver, args, b)?;
            }
            _ => {
                return Err(bad(
                    "original implemented trait actual has a different full formal kind",
                ));
            }
        }
    }
    Ok(())
}

fn carrier(
    module: &TypedModule,
    pattern: &DynTypePattern,
    args: &Arguments<'_>,
    b: &mut Budget,
) -> Check<Type> {
    let ty = match pattern {
        DynTypePattern::Concrete(ty) => ty,
        DynTypePattern::Parameter(i) | DynTypePattern::MethodParameter(i) => {
            match args.formal(*i, matches!(pattern, DynTypePattern::MethodParameter(_)))? {
                DynGenericActual::Type(ty) => ty,
                DynGenericActual::QualifiedType(value) if value.life.binder_count == 0 => &value.ty,
                _ => {
                    return Err(bad(
                        "scalar carrier formal has no complete zero-domain actual type",
                    ));
                }
            }
        }
        _ => {
            return Err(bad(
                "scalar carrier requires its associated type query proof",
            ));
        }
    };
    values::closed(module, ty, b)?;
    dynamic_metadata::ty(ty, b)?;
    Ok(ty.clone())
}
pub(super) fn constant(
    module: &TypedModule,
    pattern: &DynConstPattern,
    expected: &Type,
    args: &Arguments<'_>,
    b: &mut Budget,
) -> Check<Constant> {
    dynamic_metadata::recipe(pattern, b)?;
    fn substitute(
        module: &TypedModule,
        pattern: &DynConstPattern,
        args: &Arguments<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> Check<DynConstPattern> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("original constant substitution depth exceeded"));
        }
        Ok(match pattern {
            DynConstPattern::Parameter(i) | DynConstPattern::MethodParameter(i) => {
                match args.formal(*i, matches!(pattern, DynConstPattern::MethodParameter(_)))? {
                    DynGenericActual::Const(value) => {
                        dynamic_metadata::recipe(pattern, b)?;
                        DynConstPattern::Value(value.clone())
                    }
                    _ => return Err(bad("original Const formal has a different actual kind")),
                }
            }
            DynConstPattern::Unary { op, value, ty } => DynConstPattern::Unary {
                op: *op,
                value: Box::new(substitute(module, value, args, b, depth + 1)?),
                ty: Box::new(DynTypePattern::Concrete(carrier(module, ty, args, b)?)),
            },
            DynConstPattern::Binary {
                op,
                left,
                right,
                ty,
            } => DynConstPattern::Binary {
                op: *op,
                left: Box::new(substitute(module, left, args, b, depth + 1)?),
                right: Box::new(substitute(module, right, args, b, depth + 1)?),
                ty: Box::new(DynTypePattern::Concrete(carrier(module, ty, args, b)?)),
            },
            DynConstPattern::Convert { value, ty } => DynConstPattern::Convert {
                value: Box::new(substitute(module, value, args, b, depth + 1)?),
                ty: Box::new(DynTypePattern::Concrete(carrier(module, ty, args, b)?)),
            },
            DynConstPattern::AssociatedProjection { .. } => {
                return Err(bad(
                    "associated constant requires its complete active query graph",
                ));
            }
            literal => literal.clone(),
        })
    }
    let closed = substitute(module, pattern, args, b, 0)?;
    constants::closed(&closed, expected, module.target.pointer_width, b)?
        .ok_or_else(|| bad("original constant remains unresolved after complete substitution"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn nominal(count: u32) -> (TypedModule, DynTypeValue, DynTypePattern) {
        let (mut module, _) = crate::tests::baseline();
        let declaration = LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(50),
            compiler: false,
        };
        let life = StorageLifeShape {
            binder_count: count,
            slots: vec![],
            retained: vec![],
            nominal_arguments: (0..2)
                .map(|i| NominalLifeArgument {
                    path: vec![],
                    declaration,
                    argument: i,
                    binder: i % count,
                })
                .collect(),
        };
        let ty = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: format!("test.P{count}"),
                fields: vec![],
            }),
            life: life.clone(),
        };
        module.life_declarations.push(LifeNominalDeclaration {
            id: declaration,
            identity: LifeDeclarationIdentity {
                package: "test".into(),
                module_path: vec!["model".into()],
                declaration_path: vec!["P".into()],
                generation_path: vec![],
            },
            kind: LifeNominalKind::Record,
            parameters: vec![LifeParameterKind::Lifetime; 2],
        });
        module.life_instances.push(LifeNominalInstance {
            declaration,
            ty: ty.clone(),
            arguments: (0..2)
                .map(|i| LifeNominalActual::Lifetime(i % count))
                .collect(),
        });
        let actual = DynTypeValue {
            ty,
            life,
            classes: vec![StorageBinder::Fixed(0); count as usize],
        };
        let pattern = DynTypePattern::Nominal(Box::new(DynSelfPattern {
            declaration,
            arguments: (0..2)
                .map(|i| DynPatternActual::Life(DynLifePattern::DeclarationParameter(i)))
                .collect(),
        }));
        (module, actual, pattern)
    }
    fn run(
        module: &TypedModule,
        actual: &DynTypeValue,
        pattern: &DynTypePattern,
        declaration: &[DynGenericActual],
    ) -> Check<()> {
        let limits = Limits::default();
        let check = |module: &TypedModule| {
            let mut b = Budget::new(&limits);
            values::check(
                module,
                actual,
                DynInstantiationScope {
                    fixed_binder_count: 2,
                    invocation_binder_count: 0,
                },
                &mut b,
            )?;
            let mut arguments = Arguments {
                declaration,
                method: &[],
                invocation: &[],
                implicit: vec![],
            };
            ty(module, pattern, actual, actual, &mut arguments, &mut b)
        };
        let raw = check(module);
        let (_, plan) = crate::tests::baseline();
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = check(&decoded);
        assert_eq!(
            raw.as_ref().map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn full_nominal_occurrences_are_matched_before_any_outer_context_coalescing() {
        let (module, actual, pattern) = nominal(2);
        let equal_actuals = vec![DynGenericActual::Life(StorageBinder::Fixed(0)); 2];
        assert!(run(&module, &actual, &pattern, &equal_actuals).is_ok());
        assert_eq!(actual.life.binder_count, 2);
        let mut wrong_formal = equal_actuals.clone();
        wrong_formal[1] = DynGenericActual::Life(StorageBinder::Fixed(1));
        assert!(run(&module, &actual, &pattern, &wrong_formal).is_err());
        let mut wrong_instance = module.clone();
        wrong_instance.life_instances[0].arguments[1] = LifeNominalActual::Lifetime(0);
        assert!(run(&wrong_instance, &actual, &pattern, &equal_actuals).is_err());
        let (other, other_actual, _) = nominal(1);
        assert!(run(&other, &other_actual, &pattern, &equal_actuals).is_ok());
        assert_ne!(other_actual.ty, actual.ty);
    }
    #[test]
    fn original_constant_formals_substitute_before_exact_carrier_evaluation() {
        let (module, _) = crate::tests::baseline();
        let declaration = [
            DynGenericActual::Type(Type::I8),
            DynGenericActual::Const(Constant::I8(120)),
        ];
        let args = Arguments {
            declaration: &declaration,
            method: &[],
            invocation: &[],
            implicit: vec![],
        };
        let expression = DynConstPattern::Binary {
            op: BinaryOp::Add,
            left: Box::new(DynConstPattern::Parameter(1)),
            right: Box::new(DynConstPattern::IntegerLiteral("7".into())),
            ty: Box::new(DynTypePattern::Parameter(0)),
        };
        assert_eq!(
            constant(
                &module,
                &expression,
                &Type::I8,
                &args,
                &mut Budget::new(&Limits::default())
            )
            .unwrap(),
            Constant::I8(127)
        );
        let mut overflow = expression.clone();
        if let DynConstPattern::Binary { right, .. } = &mut overflow {
            *right = Box::new(DynConstPattern::IntegerLiteral("8".into()));
        }
        assert!(
            constant(
                &module,
                &overflow,
                &Type::I8,
                &args,
                &mut Budget::new(&Limits::default())
            )
            .is_err()
        );
        assert!(
            constant(
                &module,
                &expression,
                &Type::I16,
                &args,
                &mut Budget::new(&Limits::default())
            )
            .is_err()
        );
    }
}

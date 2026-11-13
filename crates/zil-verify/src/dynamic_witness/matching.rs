//! Original recipe matching occurs in one actual lexical domain, before any
//! table Context map. Full nominal intrinsic shapes are never coalesced here.
use super::*;

#[derive(Clone)]
pub(super) struct Arguments<'a> {
    pub owner: terms::Owner,
    pub terms: terms::Substitution,
    pub projections: Vec<(DynTypePattern, terms::Value)>,
    pub constants: Vec<(DynConstPattern, terms::Value, Constant)>,
    pub collecting_anchors: bool,
    pub declaration: &'a [DynGenericActual],
    pub method: &'a [DynGenericActual],
    pub invocation: &'a [DynLifeEmbedding],
    pub implicit: Vec<Option<StorageBinder>>,
}
impl Arguments<'_> {
    fn bind_life(
        &mut self,
        pattern: DynLifePattern,
        actual: terms::LifeTerm,
        b: &mut Budget,
    ) -> Check<()> {
        if self.collecting_anchors {
            if let Some(known) = self.terms.optional_life(pattern, b)? {
                if known.image != actual.image {
                    return Err(bad(
                        "application occurrence disagrees with an established life image",
                    ));
                }
                return Ok(());
            }
        }
        self.terms.bind(pattern, actual, b)
    }
    fn life(
        &mut self,
        pattern: DynLifePattern,
        actual: terms::LifeTerm,
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
                if rows.next().is_some() || row.to != actual.image {
                    return Err(bad(
                        "original Invocation differs from its exact actual header occurrence",
                    ));
                }
                self.bind_life(pattern, actual, b)?;
                return Ok(());
            }
            DynLifePattern::ImplicitDeclaration(i) => {
                let slot = self.implicit.get_mut(i as usize).ok_or_else(|| {
                    bad("original implicit life is outside its declaration directory")
                })?;
                match slot {
                    Some(expected) if *expected != actual.image => {
                        return Err(bad(
                            "one original implicit life has inconsistent actual occurrences",
                        ));
                    }
                    None => *slot = Some(actual.image),
                    _ => {}
                }
                self.bind_life(pattern, actual, b)?;
                return Ok(());
            }
            DynLifePattern::Bound { .. } => {
                return Err(bad(
                    "bound life requires the fresh quantified obligation domain",
                ));
            }
        };
        if !matches!(expected, Some(DynGenericActual::Life(class)) if *class == actual.image) {
            return Err(bad(
                "original life formal differs from its exact actual occurrence",
            ));
        }
        self.bind_life(pattern, actual, b)?;
        Ok(())
    }
    pub(super) fn formal(&self, index: u32, method: bool) -> Check<&DynGenericActual> {
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
    parent: &terms::Value,
    ty: &Type,
    life: &StorageLifeShape,
    path: LifeTypeProjection,
    b: &mut Budget,
) -> Check<terms::Value> {
    dynamic_metadata::ty(ty, b)?;
    if life.binder_count as usize > b.limits.max_nodes {
        return Err(limit("nested actual type scope exceeds selected bound"));
    }
    b.spend((life.binder_count as usize).saturating_add(parent.life.retained.len()))?;
    let mut classes = Vec::with_capacity(life.binder_count as usize);
    let mut terms = Vec::with_capacity(life.binder_count as usize);
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
        terms.push(parent.term(row.binder, b)?);
    }
    Ok(terms::Value {
        raw: DynTypeValue {
            ty: ty.clone(),
            life: life.clone(),
            classes,
        },
        terms,
    })
}
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
    let actual = terms::Value::anchored(actual, args.owner, terms::Root::Receiver, b)?;
    let receiver = terms::Value::anchored(receiver, args.owner, terms::Root::Receiver, b)?;
    walk(module, pattern, &actual, &receiver, args, b, 0)
}
fn walk(
    module: &TypedModule,
    pattern: &DynTypePattern,
    actual: &terms::Value,
    receiver: &terms::Value,
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
            dynamic_metadata::recipe(&receiver.raw, b)?;
            dynamic_metadata::recipe(&actual.raw, b)?;
            if receiver.raw != actual.raw
                || !args.collecting_anchors && receiver.terms != actual.terms
            {
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
                DynGenericActual::QualifiedType(expected) if expected == &actual.raw => {}
                _ => {
                    return Err(bad(
                        "original Type formal differs from its full intrinsic type and lexical map",
                    ));
                }
            }
            let method = matches!(pattern, DynTypePattern::MethodParameter(_));
            if args.collecting_anchors {
                if let Some(known) = args.terms.type_actual(method, *i, b)? {
                    if known.raw != actual.raw {
                        return Err(bad(
                            "application whole Type occurrence differs from its established actual",
                        ));
                    }
                    return Ok(());
                }
            }
            args.terms.bind_type(method, *i, actual, b)?;
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
                        args.life(*pattern, actual.term(*intrinsic, b)?, b)?;
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
            b.spend(args.projections.len())?;
            let expected = args
                .projections
                .iter()
                .find(|(p, _)| p == pattern)
                .map(|(_, v)| v)
                .ok_or_else(|| {
                    bad("associated type matching requires its complete active query graph")
                })?;
            dynamic_metadata::recipe(&expected.raw, b)?;
            if expected.raw != actual.raw || expected.terms != actual.terms {
                return Err(bad(
                    "associated type differs from its complete independently resolved value",
                ));
            }
        }
        _ => structural(module, pattern, actual, receiver, args, b, depth + 1)?,
    }
    Ok(())
}
fn intrinsic_child(
    module: &TypedModule,
    parent: &terms::Value,
    child: &Type,
    path: LifeTypeProjection,
    b: &mut Budget,
) -> Check<terms::Value> {
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
    actual: &terms::Value,
    receiver: &terms::Value,
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
            args.life(life, actual.term(actual.life.slots[0].binder, b)?, b)?;
            let child = intrinsic_child(module, actual, child, LifeTypeProjection::Pointee, b)?;
            walk(module, inner, &child, receiver, args, b, depth)?;
        }
        (DynTypePattern::Desc { lifetime, inner }, Type::Desc(child)) => {
            let life = lifetime.ok_or_else(|| bad("original Description lifetime is Unknown"))?;
            if actual.life.slots.len() != 1 || !actual.life.slots[0].path.is_empty() {
                return Err(bad("actual Description outer occurrence is absent"));
            }
            args.life(life, actual.term(actual.life.slots[0].binder, b)?, b)?;
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
                let expected = match *pattern {
                    DynLifePattern::DeclarationParameter(i) => args.declaration.get(i as usize),
                    DynLifePattern::MethodParameter(i) => args.method.get(i as usize),
                    _ => None,
                };
                if !matches!(expected, Some(DynGenericActual::Life(image)) if image == actual) {
                    return Err(bad(
                        "implemented trait lifetime differs from the original formal image",
                    ));
                }
                if let Some(term) = args.terms.optional_life(*pattern, b)? {
                    if term.image != *actual {
                        return Err(bad(
                            "implemented trait loses the matched intrinsic lifetime image",
                        ));
                    }
                }
            }
            (DynPatternActual::Const(pattern), DynGenericActual::Const(actual)) => {
                if constant(module, pattern, &actual.ty(), args, b)? != *actual {
                    return Err(bad(
                        "original implemented trait constant differs from its actual bits",
                    ));
                }
            }
            (DynPatternActual::Type(pattern), DynGenericActual::QualifiedType(actual)) => {
                super::instantiate::against(module, pattern, actual, receiver, args, b)?;
            }
            (DynPatternActual::Type(pattern), DynGenericActual::Type(actual)) => {
                values::closed(module, actual, b)?;
                let actual = empty(actual, b)?;
                super::instantiate::against(module, pattern, &actual.raw, receiver, args, b)?;
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

/// Collect every finer actual type occurrence before considering an unused
/// formal's lexical origin. This phase never rewrites an existing Self anchor.
pub(super) fn complete_anchors(
    module: &TypedModule,
    declaration: &DynDeclaration,
    actual: &DynTraitApplication,
    receiver: &DynTypeValue,
    args: &mut Arguments<'_>,
    b: &mut Budget,
) -> Check<()> {
    let pattern = declaration
        .implemented_trait
        .as_ref()
        .ok_or_else(|| bad("original implemented trait recipe is Unknown"))?;
    if pattern.declaration != actual.declaration || pattern.arguments.len() != actual.actuals.len()
    {
        return Err(bad(
            "application anchor directory changes the original trait or arity",
        ));
    }
    args.collecting_anchors = true;
    let receiver_value = terms::Value::anchored(receiver, args.owner, terms::Root::Receiver, b)?;
    for (ordinal, (pattern, actual)) in pattern.arguments.iter().zip(&actual.actuals).enumerate() {
        let DynPatternActual::Type(pattern) = pattern else {
            continue;
        };
        let value = match actual {
            DynGenericActual::QualifiedType(value) => {
                dynamic_metadata::recipe(value, b)?;
                value.clone()
            }
            DynGenericActual::Type(ty) => {
                values::closed(module, ty, b)?;
                empty(ty, b)?.raw
            }
            _ => return Err(bad("application type anchor has a different actual kind")),
        };
        let value = terms::Value::anchored(
            &value,
            args.owner,
            terms::Root::ApplicationType(ordinal as u32),
            b,
        )?;
        walk(module, pattern, &value, &receiver_value, args, b, 0)?;
    }
    args.collecting_anchors = false;
    if !matches!(args.owner, terms::Owner::Witness(..)) {
        return Err(bad(
            "function unused formals require complete actual header anchors before lexical fallback",
        ));
    }
    if declaration.parameters.len() != args.declaration.len() {
        return Err(bad("unused life anchor omits full original mixed formals"));
    }
    for (ordinal, (kind, actual)) in declaration
        .parameters
        .iter()
        .zip(args.declaration)
        .enumerate()
    {
        if !matches!(kind, LifeParameterKind::Lifetime) {
            continue;
        }
        let DynGenericActual::Life(image) = actual else {
            return Err(bad("unused life actual changes the original formal kind"));
        };
        let formal = DynLifePattern::DeclarationParameter(ordinal as u32);
        if args.terms.optional_life(formal, b)?.is_none() {
            args.terms.bind(
                formal,
                terms::LifeTerm {
                    origin: terms::Origin {
                        owner: args.owner,
                        root: terms::Root::Lexical(*image),
                        class: 0,
                    },
                    image: *image,
                },
                b,
            )?;
        }
    }
    // The collected equations must also reconstruct the original full actual
    // application. Repeated a/a cannot match a P2 just because images coincide.
    application(module, pattern, actual, receiver, args, b)
}

/// Preserve the matched origin substitution across the implemented-trait
/// boundary. Public actuals check images only; they never replace this map.
pub(super) fn transfer_application(
    module: &TypedModule,
    pattern: &DynSelfPattern,
    actual: &DynTraitApplication,
    receiver: &DynTypeValue,
    args: &mut Arguments<'_>,
    b: &mut Budget,
) -> Check<terms::Substitution> {
    application(module, pattern, actual, receiver, args, b)?;
    let mut result = terms::Substitution::default();
    for (i, (pattern, actual)) in pattern.arguments.iter().zip(&actual.actuals).enumerate() {
        b.spend(1)?;
        match (pattern, actual) {
            (DynPatternActual::Life(pattern), DynGenericActual::Life(image)) => {
                // An unused unanchored formal remains present in the full
                // original actual vector. It supplies no invented RHS origin.
                if let Some(term) = args.terms.optional_life(*pattern, b)? {
                    if term.image != *image {
                        return Err(bad("trait formal changes the matched lifetime image"));
                    }
                    result.bind(DynLifePattern::DeclarationParameter(i as u32), term, b)?;
                }
            }
            (DynPatternActual::Type(pattern), DynGenericActual::QualifiedType(actual)) => {
                let value =
                    super::instantiate::against(module, pattern, actual, receiver, args, b)?;
                result.bind_type(false, i as u32, &value, b)?;
            }
            (DynPatternActual::Type(pattern), DynGenericActual::Type(actual)) => {
                let actual = empty(actual, b)?;
                let value =
                    super::instantiate::against(module, pattern, &actual.raw, receiver, args, b)?;
                result.bind_type(false, i as u32, &value, b)?;
            }
            (DynPatternActual::Const(_), DynGenericActual::Const(_)) => {}
            _ => {
                return Err(bad(
                    "trait substitution changes an original mixed formal kind",
                ));
            }
        }
    }
    Ok(result)
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
        DynTypePattern::AssociatedProjection { .. } => {
            b.spend(args.projections.len())?;
            &args
                .projections
                .iter()
                .find(|(p, _)| p == pattern)
                .map(|(_, v)| v)
                .ok_or_else(|| bad("scalar carrier requires its associated type query proof"))?
                .raw
                .ty
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
                b.spend(args.constants.len())?;
                let (_, ty, value) = args
                    .constants
                    .iter()
                    .find(|(p, _, _)| p == pattern)
                    .ok_or_else(|| {
                        bad("associated constant requires its complete active query graph")
                    })?;
                if ty.raw.ty != value.ty() {
                    return Err(bad("associated constant cache carrier differs"));
                }
                DynConstPattern::Value(value.clone())
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
                owner: terms::Owner::Function(FunctionId(0)),
                terms: terms::Substitution::default(),
                projections: vec![],
                constants: vec![],
                collecting_anchors: false,
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
    fn fresh_nominal_rhs_uses_intrinsic_origins_not_equal_lexical_images() {
        let (mut module, p2, pab) = nominal(2);
        let (other, p1, _) = nominal(1);
        module.life_instances.extend(other.life_instances);
        let actuals = vec![DynGenericActual::Life(StorageBinder::Fixed(0)); 2];
        let mut args = Arguments {
            owner: terms::Owner::Function(FunctionId(0)),
            terms: terms::Substitution::default(),
            projections: vec![],
            constants: vec![],
            collecting_anchors: false,
            declaration: &actuals,
            method: &[],
            invocation: &[],
            implicit: vec![],
        };
        let mut b = Budget::new(&Limits::default());
        ty(&module, &pab, &p2, &p2, &mut args, &mut b).unwrap();
        let a = args
            .terms
            .life(DynLifePattern::DeclarationParameter(0), &mut b)
            .unwrap();
        let other = args
            .terms
            .life(DynLifePattern::DeclarationParameter(1), &mut b)
            .unwrap();
        assert_eq!(a.image, other.image);
        assert_ne!(a.origin, other.origin);
        super::super::instantiate::against(&module, &pab, &p2, &p2, &mut args, &mut b).unwrap();
        assert!(
            super::super::instantiate::against(&module, &pab, &p1, &p2, &mut args, &mut b).is_err()
        );
        let mut paa = pab.clone();
        if let DynTypePattern::Nominal(pattern) = &mut paa {
            pattern.arguments[1] = pattern.arguments[0].clone();
        }
        super::super::instantiate::against(&module, &paa, &p1, &p2, &mut args, &mut b).unwrap();
        assert!(
            super::super::instantiate::against(&module, &paa, &p2, &p2, &mut args, &mut b).is_err()
        );
        let trait_pattern = if let DynTypePattern::Nominal(pattern) = &pab {
            pattern.as_ref().clone()
        } else {
            unreachable!()
        };
        let application = DynTraitApplication {
            declaration: trait_pattern.declaration,
            actuals: actuals.clone(),
        };
        let transferred = transfer_application(
            &module,
            &trait_pattern,
            &application,
            &p2,
            &mut args,
            &mut b,
        )
        .unwrap();
        let mut default_args = Arguments {
            owner: args.owner,
            terms: transferred,
            projections: vec![],
            constants: vec![],
            collecting_anchors: false,
            declaration: &application.actuals,
            method: &[],
            invocation: &[],
            implicit: vec![],
        };
        super::super::instantiate::against(&module, &pab, &p2, &p2, &mut default_args, &mut b)
            .unwrap();
        assert!(
            super::super::instantiate::against(&module, &pab, &p1, &p2, &mut default_args, &mut b)
                .is_err()
        );
        let mut shared = Arguments {
            owner: args.owner,
            terms: terms::Substitution::default(),
            projections: vec![],
            constants: vec![],
            collecting_anchors: false,
            declaration: &actuals,
            method: &[],
            invocation: &[],
            implicit: vec![],
        };
        ty(&module, &pab, &p1, &p1, &mut shared, &mut b).unwrap();
        super::super::instantiate::against(&module, &pab, &p1, &p1, &mut shared, &mut b).unwrap();
        assert!(
            super::super::instantiate::against(&module, &pab, &p2, &p1, &mut shared, &mut b)
                .is_err()
        );
        let mut repeated = Arguments {
            owner: args.owner,
            terms: terms::Substitution::default(),
            projections: vec![],
            constants: vec![],
            collecting_anchors: false,
            declaration: &actuals,
            method: &[],
            invocation: &[],
            implicit: vec![],
        };
        assert!(ty(&module, &paa, &p2, &p2, &mut repeated, &mut b).is_err());
        let mut repeated = Arguments {
            owner: args.owner,
            terms: terms::Substitution::default(),
            projections: vec![],
            constants: vec![],
            collecting_anchors: false,
            declaration: &actuals,
            method: &[],
            invocation: &[],
            implicit: vec![],
        };
        ty(&module, &paa, &p1, &p1, &mut repeated, &mut b).unwrap();
    }

    #[test]
    fn original_constant_formals_substitute_before_exact_carrier_evaluation() {
        let (module, _) = crate::tests::baseline();
        let declaration = [
            DynGenericActual::Type(Type::I8),
            DynGenericActual::Const(Constant::I8(120)),
        ];
        let args = Arguments {
            owner: terms::Owner::Function(FunctionId(0)),
            terms: terms::Substitution::default(),
            projections: vec![],
            constants: vec![],
            collecting_anchors: false,
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
            **right = DynConstPattern::IntegerLiteral("8".into());
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

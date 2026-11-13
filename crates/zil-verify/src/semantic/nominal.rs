//! Exact nominal construction from its original declaration and complete
//! ordered actuals. A matching layout is never a nominal identity.
use super::*;

impl CheckedContext {
    pub(super) fn nominal(
        &self,
        module: &TypedModule,
        id: SemanticDefinitionId,
        recipes: &[SemanticActualRecipe],
        caller: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        let definition = self.definition(module, id, b)?;
        let SemanticDefinitionBody::Nominal {
            declaration,
            kind,
            body,
        } = &definition.body
        else {
            return Err(bad(
                "Nominal recipe selects another original definition kind",
            ));
        };
        b.spend(module.life_declarations.len())?;
        let mut declarations = module
            .life_declarations
            .iter()
            .filter(|row| row.id == *declaration);
        if let Some(actual) = declarations.next() {
            if declarations.next().is_some()
                || actual.identity != definition.identity
                || actual.kind != *kind
                || actual.parameters.len() != definition.parameters.len()
                || !actual
                    .parameters
                    .iter()
                    .zip(&definition.parameters)
                    .all(|(a, z)| {
                        matches!(
                            (a, z),
                            (LifeParameterKind::Lifetime, SemanticFormal::Lifetime)
                                | (LifeParameterKind::Type, SemanticFormal::Type { .. })
                                | (LifeParameterKind::Const(_), SemanticFormal::Const { .. })
                        )
                    })
            {
                return Err(bad(
                    "nominal source scope differs from its full original declaration identity or formals",
                ));
            }
        }
        if recipes.len() != definition.parameters.len() {
            return Err(bad("Nominal omits complete original mixed actual order"));
        }
        b.spend(recipes.len())?;
        let mut actuals = Vec::with_capacity(recipes.len());
        for recipe in recipes {
            actuals.push(self.evaluate_actual(module, recipe, caller, external, active, b)?);
        }
        let mut environment = caller.clone();
        // Only the original declaration's real parents are lexical ancestors.
        // The caller's established Γ remains a separate proof environment.
        environment.lexical = if let Some(parent) = definition.parent {
            let index = caller
                .lexical
                .iter()
                .position(|frame| frame.definition == parent)
                .ok_or_else(|| bad("nominal has no actual original lexical parent"))?;
            caller.lexical[index..].to_vec()
        } else {
            vec![]
        };
        let mut sealed = Vec::new();
        for formal in &definition.environment {
            sealed.push(self.evaluate_actual(
                module,
                &formal.source,
                &environment,
                external,
                active,
                b,
            )?);
        }
        let key = ScopeKey::Witness {
            definition: id,
            frame: active.fresh(b)?,
        };
        environment.lexical.insert(
            0,
            LexicalFrame {
                definition: id,
                key,
                actuals: actuals.clone(),
                environment: sealed,
                self_type: None,
            },
        );
        for (formal, actual) in definition.parameters.iter().zip(&actuals) {
            match (formal, actual) {
                (SemanticFormal::Lifetime, CheckedActual::Life(_)) => {}
                (SemanticFormal::Type { requirements }, CheckedActual::Type(_)) => {
                    self.evaluate_requirements(
                        module,
                        requirements,
                        &environment,
                        RequirementMode::ActualObligation,
                        external,
                        active,
                        b,
                    )?;
                }
                (
                    SemanticFormal::Const { carrier },
                    CheckedActual::Const {
                        carrier: actual, ..
                    },
                ) => {
                    let expected =
                        self.evaluate_type(module, carrier, &environment, external, active, b)?;
                    if !self.same_type(&environment, &expected, actual, b)? {
                        return Err(bad("nominal Const actual has a different original carrier"));
                    }
                }
                _ => return Err(bad("nominal actual changes its full original formal kind")),
            }
        }
        self.evaluate_requirements(
            module,
            &definition.requirements,
            &environment,
            RequirementMode::ActualObligation,
            external,
            active,
            b,
        )?;

        let expected = match body {
            SemanticNominalBody::Product { fields } => {
                b.spend(fields.len())?;
                let mut result = Vec::with_capacity(fields.len());
                for field in fields {
                    result.push((
                        field.name.clone(),
                        self.evaluate_type(module, &field.ty, &environment, external, active, b)?,
                    ));
                }
                Expected::Product(result)
            }
            SemanticNominalBody::Enum { variants } => {
                b.spend(variants.len())?;
                let mut result = Vec::with_capacity(variants.len());
                for variant in variants {
                    b.spend(variant.fields.len())?;
                    let mut fields = Vec::with_capacity(variant.fields.len());
                    for field in &variant.fields {
                        fields.push(self.evaluate_type(
                            module,
                            field,
                            &environment,
                            external,
                            active,
                            b,
                        )?);
                    }
                    result.push((variant.name.clone(), fields));
                }
                Expected::Enum(result)
            }
        };
        let mut selected = None;
        b.spend(module.life_instances.len())?;
        for instance in &module.life_instances {
            if instance.declaration != *declaration {
                continue;
            }
            if let Some(candidate) = self.nominal_candidate(
                module,
                &instance.ty,
                &instance.arguments,
                &actuals,
                &expected,
                caller,
                b,
            )? {
                if expected.physical(&candidate.ty, *kind, b)? {
                    retain(&mut selected, candidate, b)?;
                }
            }
        }
        // Known0 ordinary nominals may have no stored-life inventory. Their
        // real nominal scope instance supplies the exact original identity;
        // neither a type name nor a same-layout type search is accepted.
        if let Some(program) = &module.semantic {
            b.spend(program.instances.len())?;
            for instance in &program.instances {
                if instance.definition != id || instance.target != SemanticInstanceTarget::Scope {
                    continue;
                }
                let Some(value) = &instance.self_type else {
                    continue;
                };
                let (ty, life, _) = resolved(value)?;
                if life.binder_count != 0
                    || actuals.iter().any(|a| {
                        matches!(a, CheckedActual::Life(_))
                            || matches!(a, CheckedActual::Type(t) if !t.classes.is_empty())
                    })
                {
                    continue;
                }
                let actual_environment =
                    self.environment_for_instance(module, instance.id, active, b)?;
                let Some(frame) = actual_environment.lexical.first() else {
                    return Err(bad("nominal instance has no actual scope"));
                };
                if !same_actuals(self, caller, &actuals, &frame.actuals, b)? {
                    continue;
                }
                if expected.physical(ty, *kind, b)? {
                    let candidate = self.checked_value(module, ty, life, vec![], b)?;
                    retain(&mut selected, candidate, b)?;
                }
            }
        }
        selected.ok_or_else(|| {
            bad("nominal has no exact complete declaration/actual/intrinsic-partition instance")
        })
    }

    fn nominal_candidate(
        &self,
        module: &TypedModule,
        ty: &Type,
        recorded: &[LifeNominalActual],
        actuals: &[CheckedActual],
        expected: &Expected,
        caller: &EvalEnv,
        b: &mut Budget,
    ) -> Check<Option<CheckedType>> {
        dynamic_metadata::ty(ty, b)?;
        if recorded.len() != actuals.len() {
            return Ok(None);
        }
        let life = match ty {
            Type::BorrowedStorage { life, .. } => life.clone(),
            _ => empty_life(),
        };
        if life.binder_count as usize > b.limits.max_nodes {
            return Err(limit("nominal intrinsic domain exceeded"));
        }
        b.spend(life.binder_count as usize)?;
        let mut assigned: Vec<Option<(ClassKey, IntrinsicOrigin)>> =
            vec![None; life.binder_count as usize];
        for (ordinal, (recorded_actual, actual)) in recorded.iter().zip(actuals).enumerate() {
            match (recorded_actual, actual) {
                (LifeNominalActual::Lifetime(class), CheckedActual::Life(key)) => {
                    if !bind(
                        self,
                        caller,
                        &mut assigned,
                        *class,
                        key,
                        &IntrinsicOrigin::DerivedClass {
                            source: key.clone(),
                        },
                        b,
                    )? {
                        return Ok(None);
                    }
                }
                (LifeNominalActual::Const(a), CheckedActual::Const { value: z, .. }) if a == z => {}
                (LifeNominalActual::Type(recorded), CheckedActual::Type(value))
                | (
                    LifeNominalActual::QualifiedType { ty: recorded, .. },
                    CheckedActual::Type(value),
                ) => {
                    let child_life = match recorded_actual {
                        LifeNominalActual::QualifiedType { life, .. } => life.clone(),
                        LifeNominalActual::Type(Type::BorrowedStorage { life, .. }) => life.clone(),
                        _ => empty_life(),
                    };
                    if recorded != &value.ty || child_life != value.life {
                        return Ok(None);
                    }
                    for (class, (key, origin)) in
                        value.classes.iter().zip(&value.origins).enumerate()
                    {
                        b.spend(life.retained.len())?;
                        let mut rows = life.retained.iter().filter(|row| {
                            row.path == [LifeTypeProjection::TypeArgument(ordinal as u32)]
                                && row.class == class as u32
                        });
                        let row = rows.next().ok_or_else(|| {
                            bad("nominal loses a whole Type actual's intrinsic occurrence")
                        })?;
                        if rows.next().is_some() {
                            return Err(bad("nominal repeats a whole Type actual occurrence"));
                        }
                        if !bind(self, caller, &mut assigned, row.binder, key, origin, b)? {
                            return Ok(None);
                        }
                    }
                }
                _ => return Ok(None),
            }
        }
        if !expected.bind(self, caller, &life, &mut assigned, b)? {
            return Ok(None);
        }
        let mut classes = Vec::with_capacity(assigned.len());
        let mut origins = Vec::with_capacity(assigned.len());
        for assignment in assigned {
            let (key, origin) = assignment
                .ok_or_else(|| bad("nominal class is not grounded in its full actual list"))?;
            for previous in &origins {
                if self.same_origin(caller, previous, &origin, b)? {
                    return Ok(None);
                }
            }
            classes.push(key);
            origins.push(origin);
        }
        // This checks the full actual nominal metadata without treating its
        // outside images as proof that existing child classes are identical.
        let raw = DynTypeValue {
            ty: ty.clone(),
            life: life.clone(),
            classes: (0..life.binder_count).map(StorageBinder::Fixed).collect(),
        };
        dynamic_witness::check_type_value(
            module,
            &raw,
            DynInstantiationScope {
                fixed_binder_count: life.binder_count,
                invocation_binder_count: 0,
            },
            b,
        )?;
        Ok(Some(CheckedType {
            ty: ty.clone(),
            life,
            classes,
            origins,
            statics: vec![],
        }))
    }

    pub(super) fn same_origin(
        &self,
        env: &EvalEnv,
        a: &IntrinsicOrigin,
        z: &IntrinsicOrigin,
        b: &mut Budget,
    ) -> Check<bool> {
        b.spend(1)?;
        match (a, z) {
            (IntrinsicOrigin::Existing { .. }, IntrinsicOrigin::Existing { .. }) => Ok(a == z),
            (
                IntrinsicOrigin::DerivedClass { source: a },
                IntrinsicOrigin::DerivedClass { source: z },
            ) => self.same(env, a, z, b),
            (
                IntrinsicOrigin::DerivedClass { source },
                IntrinsicOrigin::Existing {
                    scope,
                    operand,
                    path,
                    class,
                },
            )
            | (
                IntrinsicOrigin::Existing {
                    scope,
                    operand,
                    path,
                    class,
                },
                IntrinsicOrigin::DerivedClass { source },
            ) => self.same(
                env,
                source,
                &ClassKey::Intrinsic {
                    scope: *scope,
                    operand: operand.clone(),
                    path: path.clone(),
                    class: *class,
                },
                b,
            ),
        }
    }
}

fn bind(
    context: &CheckedContext,
    env: &EvalEnv,
    assigned: &mut [Option<(ClassKey, IntrinsicOrigin)>],
    index: u32,
    key: &ClassKey,
    origin: &IntrinsicOrigin,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(1)?;
    let slot = assigned
        .get_mut(index as usize)
        .ok_or_else(|| bad("nominal actual class escapes its intrinsic domain"))?;
    if let Some((old, old_origin)) = slot {
        if !context.same(env, old, key, b)? || !context.same_origin(env, old_origin, origin, b)? {
            return Ok(false);
        }
        // Derived life equality cannot bridge two distinct existing child
        // origins. Once an actual child is present, retain that identity.
        if matches!(old_origin, IntrinsicOrigin::DerivedClass { .. })
            && matches!(origin, IntrinsicOrigin::Existing { .. })
        {
            *old_origin = origin.clone();
        }
        return Ok(true);
    }
    *slot = Some((key.clone(), origin.clone()));
    Ok(true)
}
fn retain(selected: &mut Option<CheckedType>, candidate: CheckedType, b: &mut Budget) -> Check<()> {
    if let Some(previous) = selected {
        dynamic_metadata::ty(&previous.ty, b)?;
        if previous != &candidate {
            return Err(bad("complete nominal key selects conflicting actual types"));
        }
    } else {
        *selected = Some(candidate);
    }
    Ok(())
}
pub(super) fn same_actuals(
    context: &CheckedContext,
    env: &EvalEnv,
    a: &[CheckedActual],
    z: &[CheckedActual],
    b: &mut Budget,
) -> Check<bool> {
    if a.len() != z.len() {
        return Ok(false);
    }
    for (a, z) in a.iter().zip(z) {
        let equal = match (a, z) {
            (CheckedActual::Type(a), CheckedActual::Type(z)) => context.same_type(env, a, z, b)?,
            (
                CheckedActual::Const {
                    carrier: a,
                    value: av,
                },
                CheckedActual::Const {
                    carrier: z,
                    value: zv,
                },
            ) => av == zv && context.same_type(env, a, z, b)?,
            (CheckedActual::Life(a), CheckedActual::Life(z)) => context.same(env, a, z, b)?,
            _ => false,
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}
enum Expected {
    Product(Vec<(String, CheckedType)>),
    Enum(Vec<(String, Vec<CheckedType>)>),
}
impl Expected {
    fn bind(
        &self,
        context: &CheckedContext,
        env: &EvalEnv,
        life: &StorageLifeShape,
        assigned: &mut [Option<(ClassKey, IntrinsicOrigin)>],
        b: &mut Budget,
    ) -> Check<bool> {
        match self {
            Self::Product(fields) => {
                for (index, (_, value)) in fields.iter().enumerate() {
                    if !bind_child(
                        context,
                        env,
                        life,
                        assigned,
                        PlaceProjection::Field(index as u32),
                        value,
                        b,
                    )? {
                        return Ok(false);
                    }
                }
            }
            Self::Enum(variants) => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, value) in fields.iter().enumerate() {
                        if !bind_child(
                            context,
                            env,
                            life,
                            assigned,
                            PlaceProjection::Variant(VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            }),
                            value,
                            b,
                        )? {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        Ok(true)
    }
    fn physical(&self, ty: &Type, kind: LifeNominalKind, b: &mut Budget) -> Check<bool> {
        let storage = match ty {
            Type::BorrowedStorage { storage, .. } => storage.as_ref(),
            other => other,
        };
        dynamic_metadata::ty(storage, b)?;
        Ok(match (self, storage, kind) {
            (
                Self::Product(expected),
                Type::Record { fields, .. },
                LifeNominalKind::Record | LifeNominalKind::Resource,
            )
            | (Self::Product(expected), Type::Package { fields, .. }, LifeNominalKind::Package) => {
                expected.len() == fields.len()
                    && expected
                        .iter()
                        .zip(fields)
                        .all(|((name, value), (actual, ty))| name == actual && value.ty == *ty)
            }
            (Self::Enum(expected), Type::Enum { variants, .. }, LifeNominalKind::Enum) => {
                expected.len() == variants.len()
                    && expected
                        .iter()
                        .zip(variants)
                        .all(|((name, fields), (actual, types))| {
                            name == actual
                                && fields.len() == types.len()
                                && fields.iter().zip(types).all(|(field, ty)| field.ty == *ty)
                        })
            }
            _ => false,
        })
    }
}

fn bind_child(
    context: &CheckedContext,
    env: &EvalEnv,
    life: &StorageLifeShape,
    assigned: &mut [Option<(ClassKey, IntrinsicOrigin)>],
    field: PlaceProjection,
    value: &CheckedType,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(
        value
            .classes
            .len()
            .saturating_add(value.life.slots.len())
            .saturating_add(value.life.retained.len())
            .saturating_add(value.life.nominal_arguments.len()),
    )?;
    if value.classes.len() != value.origins.len() {
        return Err(bad("nominal field omits intrinsic origins"));
    }
    let mut source = |binder: u32, parent: u32, b: &mut Budget| -> Check<bool> {
        let key = value
            .classes
            .get(binder as usize)
            .ok_or_else(|| bad("nominal field class escapes its domain"))?;
        let origin = value
            .origins
            .get(binder as usize)
            .ok_or_else(|| bad("nominal field origin escapes its domain"))?;
        bind(context, env, assigned, parent, key, origin, b)
    };
    for slot in &value.life.slots {
        let mut path = vec![field];
        path.extend_from_slice(&slot.path);
        b.spend(life.slots.len().saturating_add(path.len()))?;
        let mut rows = life
            .slots
            .iter()
            .filter(|row| row.path == path && row.mode == slot.mode);
        let Some(row) = rows.next() else {
            return Ok(false);
        };
        if rows.next().is_some() {
            return Err(bad("nominal field repeats an actual borrow slot"));
        }
        if !source(slot.binder, row.binder, b)? {
            return Ok(false);
        }
    }
    let prefix = LifeTypeProjection::Place(field);
    let boundary = matches!(
        value.ty,
        Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Task { .. }
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
    );
    let retained = if boundary {
        (0..value.classes.len())
            .map(|class| (vec![prefix], class as u32, class as u32))
            .collect::<Vec<_>>()
    } else {
        value
            .life
            .retained
            .iter()
            .map(|row| {
                let mut path = vec![prefix];
                path.extend_from_slice(&row.path);
                (path, row.class, row.binder)
            })
            .collect()
    };
    for (path, class, binder) in retained {
        b.spend(life.retained.len().saturating_add(path.len()))?;
        let mut rows = life
            .retained
            .iter()
            .filter(|row| row.path == path && row.class == class);
        let Some(row) = rows.next() else {
            return Ok(false);
        };
        if rows.next().is_some() {
            return Err(bad("nominal field repeats a child scope mapping"));
        }
        if !source(binder, row.binder, b)? {
            return Ok(false);
        }
    }
    for nominal in &value.life.nominal_arguments {
        let mut path = vec![prefix];
        path.extend_from_slice(&nominal.path);
        b.spend(life.nominal_arguments.len().saturating_add(path.len()))?;
        let mut rows = life.nominal_arguments.iter().filter(|row| {
            row.path == path
                && row.declaration == nominal.declaration
                && row.argument == nominal.argument
        });
        let Some(row) = rows.next() else {
            return Ok(false);
        };
        if rows.next().is_some() {
            return Err(bad("nominal field repeats an original nominal occurrence"));
        }
        if !source(nominal.binder, row.binder, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}

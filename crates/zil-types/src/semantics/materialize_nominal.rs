//! Exact actual nominal selection and complete intrinsic class transport. Class
//! images are derived from original actual recipes, not physical slot order.
use super::*;

#[derive(Clone, Copy)]
enum NominalUse<'a> {
    Materialize(Option<&'a Type>),
    Check(super::deferred::NominalPosition),
}
impl Eval<'_, '_, '_> {
    fn nominal_arguments_match(
        &mut self,
        expected: &[SemanticActual],
        actual: &[LifeNominalActual],
    ) -> R<bool> {
        self.fee(expected.len().saturating_add(actual.len()))?;
        if expected.len() != actual.len() {
            return Ok(false);
        }
        for (expected, actual) in expected.iter().zip(actual) {
            match (expected, actual) {
                (SemanticActual::Life(_), LifeNominalActual::Lifetime(_)) => {}
                (SemanticActual::Const { value, .. }, LifeNominalActual::Const(actual)) => {
                    for value in [value, actual] {
                        self.fee(match value {
                            Constant::WideString(units) => units.len() + 1,
                            _ => 1,
                        })?;
                    }
                    if value != actual {
                        return Ok(false);
                    }
                }
                (SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, .. }), actual) => {
                    let actual_ty = match actual {
                        LifeNominalActual::Type(ty)
                        | LifeNominalActual::QualifiedType { ty, .. } => ty,
                        _ => return Ok(false),
                    };
                    inference::charge_type_snapshot(
                        ty,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    inference::charge_type_snapshot(
                        actual_ty,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if ty != actual_ty {
                        return Ok(false);
                    }
                    let actual_life = match actual {
                        LifeNominalActual::QualifiedType { life, .. } => Some(life),
                        LifeNominalActual::Type(ty) => actual_life(ty),
                        _ => unreachable!(),
                    };
                    if let Some(actual_life) = actual_life {
                        let _paid = self.copy_shape(actual_life)?;
                        let _paid = self.copy_shape(life)?;
                        if actual_life != life {
                            return Ok(false);
                        }
                    } else if let Type::Task { retained_life, .. }
                    | Type::MutexGuard { retained_life, .. } = actual_ty
                    {
                        // The whole carrier Type already includes its complete
                        // internal Context tables, compared above. Its outer
                        // intrinsic scope has N classes and no flattened rows.
                        let _paid = self.copy_shape(life)?;
                        if life.binder_count != retained_life.binder_count
                            || !life.slots.is_empty()
                            || !life.retained.is_empty()
                            || !life.nominal_arguments.is_empty()
                        {
                            return Ok(false);
                        }
                    } else {
                        // A root borrow's separate occurrence directory may
                        // never be inferred from its erased physical pointer.
                        if matches!(actual_ty, Type::Address { .. } | Type::Desc(_))
                            || life.binder_count != 0
                            || !life.slots.is_empty()
                            || !life.retained.is_empty()
                            || !life.nominal_arguments.is_empty()
                        {
                            return Ok(false);
                        }
                    }
                }
                _ => return Ok(false),
            }
        }
        // Only original lifetime equality participates in fresh nominal life
        // normalization. Two formal Type positions never enter this relation.
        for (i, (left, left_actual)) in expected.iter().zip(actual).enumerate() {
            let (SemanticActual::Life(left), LifeNominalActual::Lifetime(a)) = (left, left_actual)
            else {
                continue;
            };
            for (right, right_actual) in expected[..i].iter().zip(&actual[..i]) {
                self.fee(1)?;
                if let (SemanticActual::Life(right), LifeNominalActual::Lifetime(b)) =
                    (right, right_actual)
                {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        left,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        right,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if (left == right) != (a == b) {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
    fn nominal_image(
        &mut self,
        images: &mut [Option<SemanticClassExpr>],
        binder: u32,
        image: &SemanticClassExpr,
    ) -> R<()> {
        let target = images
            .get_mut(binder as usize)
            .ok_or_else(|| bad("nominal actual projection leaves its complete parent domain"))?;
        crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
        if let Some(prior) = target {
            crate::dyn_metadata::semantic_operand_snapshot(prior, self.limits, self.left, 1)?;
            if prior != image {
                return Err(unknown(
                    "nominal actual images require an original exact lifetime equation",
                ));
            }
        } else {
            *target = Some(image.clone())
        }
        Ok(())
    }
    fn nominal_images(
        &mut self,
        declaration: LifeDeclarationId,
        expected: &[SemanticActual],
        instance: &LifeNominalInstance,
    ) -> R<Vec<SemanticClassExpr>> {
        let empty_shape = empty();
        let life = actual_life(&instance.ty).unwrap_or(&empty_shape);
        self.fee(life.binder_count as usize)?;
        let mut images = vec![None; life.binder_count as usize];
        let mut type_owners = std::collections::BTreeMap::new();
        let task_failure = matches!(declaration,
            zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD | zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE);
        for (ordinal, (expected, actual)) in expected.iter().zip(&instance.arguments).enumerate() {
            match (expected, actual) {
                (SemanticActual::Life(image), LifeNominalActual::Lifetime(binder)) => {
                    self.fee(life.nominal_arguments.len())?;
                    let mut count = 0;
                    for row in &life.nominal_arguments {
                        self.fee(row.path.len() + 1)?;
                        if row.path.is_empty()
                            && row.declaration == declaration
                            && row.argument == ordinal as u32
                        {
                            if row.binder != *binder {
                                return Err(bad(
                                    "nominal lifetime actual changes its exact root occurrence",
                                ));
                            }
                            count += 1;
                        }
                    }
                    if count != 1 {
                        return Err(bad(
                            "nominal lifetime actual lacks its unique complete root occurrence",
                        ));
                    }
                    self.nominal_image(&mut images, *binder, image)?;
                }
                (
                    SemanticActual::Type(SemanticTypeValue::Resolved {
                        life: child,
                        classes,
                        ..
                    }),
                    _,
                ) => {
                    if child.binder_count as usize != classes.len() {
                        return Err(bad("nominal Type actual omits intrinsic classes"));
                    }
                    if task_failure && ordinal == 1 && classes.len() != type_owners.len() {
                        return Err(bad("Task failure input and context class counts differ"));
                    }
                    self.fee(classes.len())?;
                    let mut selected = Vec::new();
                    for (class, image) in classes.iter().enumerate() {
                        let prefix = LifeTypeProjection::TypeArgument(ordinal as u32);
                        let mut binder = None;
                        for row in &life.retained {
                            self.fee(row.path.len() + 1)?;
                            if row.path.as_slice() == [prefix] && row.class == class as u32 {
                                if binder.replace(row.binder).is_some() {
                                    return Err(bad(
                                        "nominal Type class has repeated direct projections",
                                    ));
                                }
                            }
                        }
                        for occurrence in child
                            .nominal_arguments
                            .iter()
                            .filter(|r| r.binder == class as u32)
                        {
                            for row in &life.nominal_arguments {
                                self.fee(
                                    row.path
                                        .len()
                                        .saturating_add(occurrence.path.len())
                                        .saturating_add(1),
                                )?;
                                if row.path.first() == Some(&prefix)
                                    && row.path[1..] == occurrence.path
                                    && row.declaration == occurrence.declaration
                                    && row.argument == occurrence.argument
                                {
                                    if binder.is_some_and(|old| old != row.binder) {
                                        return Err(bad(
                                            "nominal Type intrinsic class has conflicting original projections",
                                        ));
                                    }
                                    binder = Some(row.binder);
                                }
                            }
                        }
                        let binder = binder.ok_or_else(|| {
                            unknown(
                                "nominal Type actual omits its complete intrinsic class projection",
                            )
                        })?;
                        self.fee(selected.len())?;
                        if selected.contains(&binder) {
                            return Err(bad(
                                "nominal Type actual collapses distinct intrinsic classes",
                            ));
                        }
                        selected.push(binder);
                        self.fee(type_owners.len().saturating_add(1).ilog2() as usize + 1)?;
                        let expected = (task_failure && ordinal == 1).then_some((0, class));
                        if type_owners.insert(binder, (ordinal, class)) != expected {
                            return Err(bad(
                                "distinct nominal Type actual slots collapse their intrinsic anchors",
                            ));
                        }
                        // The builtin failure carrier projects the same worker
                        // class into its Inputs and Context. Equality of the
                        // original images remains mandatory in both views.
                        self.nominal_image(&mut images, binder, image)?;
                    }
                }
                (SemanticActual::Const { .. }, _) => {}
                _ => {
                    return Err(bad(
                        "nominal application changes its full mixed actual kinds",
                    ));
                }
            }
        }
        images
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                unknown("nominal domain contains a class with no original full-actual image")
            })
    }
    pub(super) fn nominal_application(
        &mut self,
        scope: SemanticInstanceId,
        definition: SemanticDefinitionId,
        args: &[SemanticActualRecipe],
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        Ok(self.nominal_application_for(
            scope, definition, args, NominalUse::Materialize(hint), depth,
        )?.expect("nominal materialization requested a value"))
    }

    pub(super) fn check_nominal_application(
        &mut self,
        scope: SemanticInstanceId,
        definition: SemanticDefinitionId,
        args: &[SemanticActualRecipe],
        position: super::deferred::NominalPosition,
        depth: usize,
    ) -> R<()> {
        self.nominal_application_for(scope, definition, args, NominalUse::Check(position), depth)?;
        Ok(())
    }

    fn nominal_application_for(
        &mut self,
        scope: SemanticInstanceId,
        definition: SemanticDefinitionId,
        args: &[SemanticActualRecipe],
        use_: NominalUse<'_>,
        depth: usize,
    ) -> R<Option<SemanticTypeValue>> {
        self.node(depth)?;
        let original = self.definition(definition)?;
        let SemanticDefinitionBody::Nominal {
            declaration, kind, ..
        } = &original.body
        else {
            return Err(bad(
                "nominal recipe selects another original declaration kind",
            ));
        };
        if original.parameters.len() != args.len() {
            return Err(bad("nominal recipe omits full mixed actuals"));
        }
        self.fee(args.len())?;
        let mut expected = Vec::new();
        for (recipe, formal) in args.iter().zip(&original.parameters) {
            let value = self.recipe_actual(scope, recipe, depth + 1)?;
            if !matches!(
                (formal, &value),
                (SemanticFormal::Lifetime, SemanticActual::Life(_))
                    | (SemanticFormal::Type { .. }, SemanticActual::Type(_))
                    | (SemanticFormal::Const { .. }, SemanticActual::Const { .. })
            ) {
                return Err(bad("nominal recipe changes a full actual kind"));
            }
            expected.push(value);
        }
        let module = self.module;
        let staged = module.nominal_proposals.borrow();
        let (found, hint) = match use_ {
            NominalUse::Check(position) => {
                use super::deferred::NominalPosition;
                self.fee(1)?;
                let row = match position {
                    NominalPosition::Committed(index) => &module.life_instances[index],
                    NominalPosition::Proposed(index) => &staged[index].1,
                };
                let matches = row.declaration == *declaration
                    && self.nominal_arguments_match(&expected, &row.arguments)?;
                (matches.then_some(row), None)
            }
            NominalUse::Materialize(hint) => {
                let positions = module.nominal_positions(*declaration, self.left)?;
                let indexed = positions
                    .into_iter()
                    .flatten()
                    .map(|offset| &module.life_instances[*offset]);
                let unindexed = module.life_instances.iter().take(if positions.is_some() {
                    0
                } else {
                    module.life_instances.len()
                });
                self.fee(staged.len())?;
                let staged = staged.iter().map(|(before, row)| (row, Some(before)));
                let committed = indexed.chain(unindexed).map(|row| (row, None));
                let mut found = None;
                for (instance, physical_occurrence) in committed.chain(staged) {
                    self.fee(1)?;
                    if instance.declaration != *declaration {
                        continue;
                    }
                    if !self.nominal_arguments_match(&expected, &instance.arguments)? {
                        continue;
                    }
                    inference::charge_type_snapshot(
                        &instance.ty,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if let Some(hint) = hint {
                        inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
                        if &instance.ty != hint {
                            let Some(physical_occurrence) = physical_occurrence else {
                                continue;
                            };
                            inference::charge_type_snapshot(
                                physical_occurrence,
                                Span::default(),
                                self.limits,
                                self.left,
                                1,
                            )?;
                            if physical_occurrence != hint {
                                continue;
                            }
                        }
                    }
                    if found.replace(instance).is_some() {
                        return Err(bad(
                            "nominal full actuals select ambiguous complete actual types",
                        ));
                    }
                }
                (found, hint)
            }
        };
        let instance = found.ok_or_else(|| {
            let hint_kind = match hint {
                None => "none", Some(Type::Managed(_)) => "Managed", Some(Type::Desc(_)) => "Desc",
                Some(Type::Record { .. }) => "Record", Some(Type::Package { .. }) => "Package",
                Some(Type::Enum { .. }) => "Enum", Some(Type::BorrowedStorage { .. }) => "BorrowedStorage",
                Some(Type::Address { .. }) => "Address", Some(_) => "other",
            };
            unknown(&format!("nominal recipe has no actual complete declaration/application: instance {scope:?}, definition {definition:?}, declaration {declaration:?}, original kind {kind:?}, actual hint {hint_kind}, mixed actuals {}", expected.len()))
        })?;
        let actual_kind = match stored::physical(&instance.ty) {
            Type::Record { .. } if *kind == LifeNominalKind::Resource => LifeNominalKind::Resource,
            Type::Record { .. } => LifeNominalKind::Record,
            Type::Package { .. } => LifeNominalKind::Package,
            Type::Enum { .. } => LifeNominalKind::Enum,
            _ => {
                return Err(bad(
                    "nominal application changes its actual physical constructor",
                ));
            }
        };
        if actual_kind != *kind {
            return Err(bad(
                "nominal application changes its original ownership kind",
            ));
        }
        let classes = self.nominal_images(*declaration, &expected, instance)?;
        if matches!(use_, NominalUse::Check(_)) {
            return Ok(None);
        }
        let life = match actual_life(&instance.ty) {
            Some(life) => self.copy_shape(life)?,
            None => empty(),
        };
        Ok(Some(resolved(self.physical(&instance.ty)?, life, classes)))
    }
}
fn actual_life(ty: &Type) -> Option<&StorageLifeShape> {
    if let Type::BorrowedStorage { life, .. } = ty {
        Some(life)
    } else {
        None
    }
}

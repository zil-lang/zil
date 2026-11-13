//! Exact actual nominal selection and complete intrinsic class transport. Class
//! images are derived from original actual recipes, not physical slot order.
use super::*;
impl Eval<'_, '_> {
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
                        if type_owners.insert(binder, ordinal).is_some() {
                            return Err(bad(
                                "distinct nominal Type actual slots collapse their intrinsic anchors",
                            ));
                        }
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
        self.fee(self.module.life_instances.len())?;
        let mut found = None;
        for instance in &self.module.life_instances {
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
                    continue;
                }
            }
            if found.replace(instance).is_some() {
                return Err(bad(
                    "nominal full actuals select ambiguous complete actual types",
                ));
            }
        }
        let instance = found.ok_or_else(|| {
            unknown("nominal recipe has no actual complete declaration/application")
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
        let life = match actual_life(&instance.ty) {
            Some(life) => self.copy_shape(life)?,
            None => empty(),
        };
        Ok(resolved(self.physical(&instance.ty)?, life, classes))
    }
}
fn actual_life(ty: &Type) -> Option<&StorageLifeShape> {
    if let Type::BorrowedStorage { life, .. } = ty {
        Some(life)
    } else {
        None
    }
}

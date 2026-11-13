//! Materialize an original Dyn type against an actual, complete interface.
//! The table is a candidate for an exact type, never authority for a provider.
use super::*;

impl Eval<'_, '_> {
    fn dyn_class(
        &mut self,
        domain: &[SemanticClassExpr],
        image: &SemanticClassExpr,
    ) -> R<StorageBinder> {
        self.fee(domain.len())?;
        let mut found = None;
        for (index, candidate) in domain.iter().enumerate() {
            crate::dyn_metadata::semantic_operand_snapshot(candidate, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
            if candidate == image {
                if found.replace(index).is_some() {
                    return Err(bad("Dyn class image has multiple original positions"));
                }
            }
        }
        let index = found.ok_or_else(|| bad("Dyn class is outside its complete Context domain"))?;
        Ok(StorageBinder::Fixed(
            u32::try_from(index).map_err(|_| bad("Dyn class position overflows"))?,
        ))
    }

    fn dyn_actual(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticActualRecipe,
        expected: &DynGenericActual,
        domain: &[SemanticClassExpr],
        depth: usize,
    ) -> R<()> {
        let actual = self.recipe_actual(scope, recipe, depth + 1)?;
        self.fee(1)?;
        match (actual, expected) {
            (
                SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes }),
                DynGenericActual::Type(expected),
            ) if life.binder_count == 0 && classes.is_empty() && ty == *expected => Ok(()),
            (
                SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes }),
                DynGenericActual::QualifiedType(expected),
            ) if ty == expected.ty
                && life == expected.life
                && classes.len() == life.binder_count as usize =>
            {
                self.fee(classes.len())?;
                for (image, expected) in classes.iter().zip(&expected.classes) {
                    if self.dyn_class(domain, image)? != *expected {
                        return Err(bad("Dyn Type actual changes an intrinsic class position"));
                    }
                }
                Ok(())
            }
            (SemanticActual::Const { ty, value }, DynGenericActual::Const(expected)) => {
                let SemanticTypeValue::Resolved { ty, life, classes } = ty else {
                    return Err(bad("Dyn Const carrier has no complete actual type"));
                };
                if life.binder_count != 0
                    || !classes.is_empty()
                    || ty != expected.ty()
                    || value != *expected
                {
                    return Err(bad(
                        "Dyn Const actual differs from its full carrier or value",
                    ));
                }
                Ok(())
            }
            (SemanticActual::Life(image), DynGenericActual::Life(expected))
                if self.dyn_class(domain, &image)? == *expected =>
            {
                Ok(())
            }
            _ => Err(bad(
                "Dyn actual changes its original kind, value, or class domain",
            )),
        }
    }

    fn dyn_application(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTraitRecipe,
        actual: &DynTraitApplication,
        domain: &[SemanticClassExpr],
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        if recipe.declaration != actual.declaration
            || recipe.arguments.len() != actual.actuals.len()
        {
            return Err(bad(
                "Dyn trait application changes its exact declaration or arity",
            ));
        }
        self.fee(self.module.dyn_declarations.len())?;
        let mut declarations = self
            .module
            .dyn_declarations
            .iter()
            .filter(|row| row.id == recipe.declaration);
        let declaration = declarations
            .next()
            .ok_or_else(|| bad("Dyn trait has no actual declaration"))?;
        if declarations.next().is_some()
            || declaration.kind != DynDeclarationKind::Trait
            || declaration.parameters.len() != recipe.arguments.len()
        {
            return Err(bad(
                "Dyn trait declaration or complete formal order is ambiguous",
            ));
        }
        self.fee(recipe.arguments.len())?;
        for ((recipe, actual), kind) in recipe
            .arguments
            .iter()
            .zip(&actual.actuals)
            .zip(&declaration.parameters)
        {
            let valid = matches!(
                (kind, actual),
                (LifeParameterKind::Type, DynGenericActual::Type(_))
                    | (LifeParameterKind::Type, DynGenericActual::QualifiedType(_))
                    | (LifeParameterKind::Lifetime, DynGenericActual::Life(_))
                    | (LifeParameterKind::Const(_), DynGenericActual::Const(_))
            );
            if !valid {
                return Err(bad("Dyn trait actual differs from its declared mixed kind"));
            }
            if let (LifeParameterKind::Const(carrier), DynGenericActual::Const(value)) =
                (kind, actual)
            {
                if *carrier != value.ty() {
                    return Err(bad("Dyn trait Const actual changes its declared carrier"));
                }
            }
            self.dyn_actual(scope, recipe, actual, domain, depth + 1)?;
        }
        Ok(())
    }

    fn dyn_context(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticDynContextRecipe,
        actual: &DynInterface,
        depth: usize,
    ) -> R<Vec<SemanticClassExpr>> {
        self.node(depth)?;
        let (shape, classes) = match recipe {
            SemanticDynContextRecipe::Closed => {
                if actual.context.is_some() {
                    return Err(bad("closed Dyn Context has an actual nonempty Context"));
                }
                (
                    RetainedLifeShape {
                        binder_count: 0,
                        ports: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                    Vec::new(),
                )
            }
            SemanticDynContextRecipe::Shape(shape) => {
                self.retained_shape(scope, shape, depth + 1)?
            }
            SemanticDynContextRecipe::Type(ty) => {
                let context = actual
                    .context
                    .as_ref()
                    .ok_or_else(|| bad("Dyn Context type has no actual complete Context"))?;
                let SemanticTypeValue::Resolved { ty, life, classes } =
                    self.ty(scope, ty, Some(context), depth + 1)?
                else {
                    return Err(bad("Dyn Context type did not materialize"));
                };
                if ty != *context {
                    return Err(bad("Dyn Context changes its exact actual type"));
                }
                // The physical ports are validated from the complete Context
                // inventory when the actual interface is constructed.
                if life.binder_count != actual.retained_life.binder_count
                    || life.retained != actual.retained_life.retained
                    || life.nominal_arguments != actual.retained_life.nominal_arguments
                {
                    return Err(bad("Dyn Context loses a retained or nominal relation"));
                }
                (actual.retained_life.clone(), classes)
            }
        };
        if shape != actual.retained_life || classes.len() != shape.binder_count as usize {
            return Err(bad("Dyn Context differs from its complete retained shape"));
        }
        Ok(classes)
    }

    fn dyn_associated(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &[SemanticAssociatedBindingRecipe],
        actual: &DynInterface,
        domain: &[SemanticClassExpr],
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        if recipe.len() != actual.associated_types.len() + actual.associated_constants.len() {
            return Err(bad("Dyn associated bindings omit an actual member"));
        }
        self.fee(recipe.len().saturating_mul(recipe.len().saturating_add(1)))?;
        let mut seen_types = vec![false; actual.associated_types.len()];
        let mut seen_consts = vec![false; actual.associated_constants.len()];
        for binding in recipe {
            match &binding.value {
                SemanticAssociatedRecipe::Type(value) => {
                    let mut matched = None;
                    for (index, row) in actual.associated_types.iter().enumerate() {
                        if row.member.ordinal != binding.member
                            || row.member.application.declaration != binding.application.declaration
                        {
                            continue;
                        }
                        match self.dyn_application(
                            scope,
                            &binding.application,
                            &row.member.application,
                            domain,
                            depth + 1,
                        ) {
                            Ok(()) if matched.replace(index).is_some() => {
                                return Err(bad("Dyn associated Type member is ambiguous"))
                            }
                            Ok(()) => {}
                            Err(error)
                                if error.category == zil_base::ResultCategory::ResourceLimit =>
                            {
                                return Err(error)
                            }
                            Err(_) => {}
                        }
                    }
                    let index = matched
                        .ok_or_else(|| bad("Dyn associated Type has no exact actual member"))?;
                    if seen_types[index] {
                        return Err(bad("Dyn associated Type member is ambiguous"));
                    }
                    seen_types[index] = true;
                    let row = &actual.associated_types[index];
                    let SemanticTypeValue::Resolved { ty, life, classes } =
                        self.ty(scope, value, Some(&row.ty), depth + 1)?
                    else {
                        return Err(bad("Dyn associated Type did not materialize"));
                    };
                    if ty != row.ty || life != row.life || classes.len() != row.classes.len() {
                        return Err(bad("Dyn associated Type changes its complete result"));
                    }
                    for (image, expected) in classes.iter().zip(&row.classes) {
                        if self.dyn_class(domain, image)? != *expected {
                            return Err(bad("Dyn associated Type changes a result class"));
                        }
                    }
                }
                SemanticAssociatedRecipe::Const { carrier, value } => {
                    let mut matched = None;
                    for (index, row) in actual.associated_constants.iter().enumerate() {
                        if row.member.ordinal != binding.member
                            || row.member.application.declaration != binding.application.declaration
                        {
                            continue;
                        }
                        match self.dyn_application(
                            scope,
                            &binding.application,
                            &row.member.application,
                            domain,
                            depth + 1,
                        ) {
                            Ok(()) if matched.replace(index).is_some() => {
                                return Err(bad("Dyn associated Const member is ambiguous"))
                            }
                            Ok(()) => {}
                            Err(error)
                                if error.category == zil_base::ResultCategory::ResourceLimit =>
                            {
                                return Err(error)
                            }
                            Err(_) => {}
                        }
                    }
                    let index = matched
                        .ok_or_else(|| bad("Dyn associated Const has no exact actual member"))?;
                    if seen_consts[index] {
                        return Err(bad("Dyn associated Const member is ambiguous"));
                    }
                    seen_consts[index] = true;
                    let row = &actual.associated_constants[index];
                    let SemanticTypeValue::Resolved { ty, life, classes } =
                        self.ty(scope, carrier, Some(&row.ty), depth + 1)?
                    else {
                        return Err(bad("Dyn associated Const carrier did not materialize"));
                    };
                    if ty != row.ty
                        || life.binder_count != 0
                        || !classes.is_empty()
                        || self.constant(scope, value, &row.ty, depth + 1)? != row.value
                    {
                        return Err(bad("Dyn associated Const changes its carrier or value"));
                    }
                }
            }
        }
        if seen_types.iter().any(|seen| !seen) || seen_consts.iter().any(|seen| !seen) {
            return Err(bad("Dyn associated binding set is incomplete"));
        }
        Ok(())
    }

    pub(super) fn dyn_type(
        &mut self,
        scope: SemanticInstanceId,
        owned: bool,
        recipe: &SemanticDynInterfaceRecipe,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.node(depth)?;
        let hinted_id = match (owned, hint) {
            (true, Some(Type::OwnedDyn { interface })) | (false, Some(Type::Dyn { interface })) => {
                Some(*interface)
            }
            (_, Some(_)) => return Err(bad("Dyn recipe differs from its exact actual type")),
            (_, None) => None,
        };
        self.fee(self.module.dyn_interfaces.len())?;
        self.fee(
            self.module
                .dyn_interfaces
                .iter()
                .map(|row| {
                    row.application
                        .actuals
                        .len()
                        .saturating_add(row.inherited.len())
                        .saturating_add(row.associated_types.len())
                        .saturating_add(row.associated_constants.len())
                        .saturating_add(row.slots.len())
                })
                .sum(),
        )?;
        let candidates: Vec<DynInterface> = self
            .module
            .dyn_interfaces
            .iter()
            .filter(|row| {
                hinted_id.is_none_or(|id| row.id == id)
                    && row.application.declaration == recipe.application.declaration
            })
            .cloned()
            .collect();
        if candidates.is_empty() {
            return Err(bad(
                "Dyn has no actual interface with its exact declaration and target",
            ));
        }
        let mut found = None;
        for candidate in &candidates {
            // Only an exact full recipe may select an unhinted interface. Any
            // failed hinted comparison is propagated instead of searching by shape.
            let matched = (|| {
                let domain = self.dyn_context(scope, &recipe.context, candidate, depth + 1)?;
                self.dyn_application(
                    scope,
                    &recipe.application,
                    &candidate.application,
                    &domain,
                    depth + 1,
                )?;
                self.dyn_associated(scope, &recipe.associated, candidate, &domain, depth + 1)?;
                Ok::<_, Diagnostic>(domain)
            })();
            match matched {
                Ok(domain) => {
                    if found.replace((candidate, domain)).is_some() {
                        return Err(bad(
                            "Dyn complete original recipe selects multiple actual interfaces",
                        ));
                    }
                }
                Err(error)
                    if hinted_id.is_some()
                        || error.category == zil_base::ResultCategory::ResourceLimit =>
                {
                    return Err(error)
                }
                Err(_) => {}
            }
        }
        let (candidate, domain) = found.ok_or_else(|| {
            bad("Dyn original application, Context, or associated bindings match no actual interface")
        })?;
        let ty = if owned {
            Type::OwnedDyn {
                interface: candidate.id,
            }
        } else {
            Type::Dyn {
                interface: candidate.id,
            }
        };
        Ok(resolved(
            ty,
            StorageLifeShape {
                binder_count: candidate.retained_life.binder_count,
                slots: Vec::new(),
                retained: candidate.retained_life.retained.clone(),
                nominal_arguments: candidate.retained_life.nominal_arguments.clone(),
            },
            domain,
        ))
    }
}

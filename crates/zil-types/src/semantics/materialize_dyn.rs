//! Materialize an original Dyn type against an actual, complete interface.
//! The table is a candidate for an exact type, never authority for a provider.
use super::*;
#[path = "dyn_registry.rs"]
mod registry;
pub(crate) use registry::relocate;

impl Eval<'_, '_, '_> {
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
                                return Err(bad("Dyn associated Type member is ambiguous"));
                            }
                            Ok(()) => {}
                            Err(error)
                                if error.category == zil_base::ResultCategory::ResourceLimit =>
                            {
                                return Err(error);
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
                                return Err(bad("Dyn associated Const member is ambiguous"));
                            }
                            Ok(()) => {}
                            Err(error)
                                if error.category == zil_base::ResultCategory::ResourceLimit =>
                            {
                                return Err(error);
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
                    return Err(error);
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

fn provider_fee(left: &mut usize, count: usize) -> R<()> {
    *left = left.checked_sub(count).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "Dyn original provider check exceeded", None)
    })?;
    Ok(())
}

fn exact_scope<'a>(
    program: &'a SemanticModule,
    owner: SemanticDynOwner,
    left: &mut usize,
) -> R<&'a SemanticDefinition> {
    provider_fee(
        left,
        program
            .dyn_scopes
            .len()
            .saturating_add(program.definitions.len()),
    )?;
    let mut rows = program.dyn_scopes.iter().filter(|row| row.owner == owner);
    let row = rows
        .next()
        .ok_or_else(|| unknown("Dyn has no original owner scope binding"))?;
    if rows.next().is_some() {
        return Err(bad("Dyn original owner scope binding is ambiguous"));
    }
    let mut definitions = program.definitions.iter().filter(|d| d.id == row.scope);
    let definition = definitions
        .next()
        .ok_or_else(|| bad("Dyn owner scope has no original definition"))?;
    if definitions.next().is_some() {
        return Err(bad("Dyn owner scope has duplicate original definitions"));
    }
    Ok(definition)
}

fn original_signature(definition: &SemanticDefinition) -> R<&SemanticSignatureRecipe> {
    match &definition.body {
        SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. } => Ok(signature),
        _ => Err(bad(
            "Dyn method owner does not name an original function signature",
        )),
    }
}

fn exact_signature_recipe(
    recipe: &DynSignatureRecipe,
    definition: &SemanticDefinition,
    left: &mut usize,
) -> R<()> {
    let original = original_signature(definition)?;
    provider_fee(
        left,
        recipe
            .parameters
            .len()
            .saturating_add(original.parameters.len()),
    )?;
    if recipe.parameters.len() != original.parameters.len()
        || original.bound_lifetimes != 0
        || recipe.invocation_binder_count != original.invocation_lifetimes
        || recipe.result_mode != original.result_mode
        || recipe.call_kind != original.call_kind
    {
        return Err(bad(
            "Dyn method signature changes original quantifiers or modes",
        ));
    }
    for (pattern, parameter) in recipe.parameters.iter().zip(&original.parameters) {
        if pattern.mode != parameter.mode
            || !matches!(&pattern.ty, DynTypePattern::Semantic { scope, ty }
                if *scope == definition.id && ty.as_ref() == &parameter.ty)
        {
            return Err(bad(
                "Dyn method parameter loses its exact original type recipe",
            ));
        }
    }
    if !matches!(&recipe.result, DynTypePattern::Semantic { scope, ty }
        if *scope == definition.id && ty.as_ref() == &original.result)
    {
        return Err(bad(
            "Dyn method result loses its exact original type recipe",
        ));
    }
    Ok(())
}

fn exact_effects(recipe: &[DynEffectRecipe], actual: &[DynEffect]) -> R<()> {
    if recipe.len() != actual.len() {
        return Err(bad("Dyn method effect recipe omits an actual effect"));
    }
    for (recipe, actual) in recipe.iter().zip(actual) {
        let matched = matches!(
            (recipe, actual),
            (DynEffectRecipe::Alloc, DynEffect::Alloc)
                | (DynEffectRecipe::Io, DynEffect::Io)
                | (DynEffectRecipe::Block, DynEffect::Block)
                | (DynEffectRecipe::Suspend, DynEffect::Suspend)
                | (DynEffectRecipe::Spawn, DynEffect::Spawn)
                | (DynEffectRecipe::Ffi, DynEffect::Ffi)
                | (DynEffectRecipe::Reenter, DynEffect::Reenter)
                | (DynEffectRecipe::Panic, DynEffect::Panic)
        );
        if !matched {
            return Err(unknown(
                "Dyn method effect needs its exact callable or cleanup recipe",
            ));
        }
    }
    Ok(())
}

fn exact_contract_recipe(recipe: &DynContractRecipe, actual: &DynContract) -> R<()> {
    match (&recipe.requires, &actual.requires) {
        (Some(recipes), Some(actuals)) if recipes.len() == actuals.len() => {
            for (recipe, actual) in recipes.iter().zip(actuals) {
                if !matches!((recipe, actual),
                    (DynRequirementRecipe::Read(a), DynRequirement::Read(b)) if a == b)
                    && !matches!((recipe, actual),
                    (DynRequirementRecipe::Write(a), DynRequirement::Write(b)) if a == b)
                    && !matches!((recipe, actual),
                    (DynRequirementRecipe::Manage(a), DynRequirement::Manage(b)) if a == b)
                    && !matches!((recipe, actual),
                    (DynRequirementRecipe::Apart(a, b), DynRequirement::Apart(c, d)) if a == c && b == d)
                {
                    return Err(bad("Dyn method requirement changes its original contract"));
                }
            }
        }
        (None, None) => {}
        _ => return Err(bad("Dyn method requirement directory is incomplete")),
    }
    match (&recipe.guarantees, &actual.guarantees) {
        (Some(recipes), Some(actuals)) if recipes.is_empty() && actuals.is_empty() => {}
        (None, None) => {}
        _ => {
            return Err(unknown(
                "Dyn method guarantee needs its complete source recipe",
            ));
        }
    }
    if recipe.hold != actual.hold
        || recipe.call != actual.call
        || recipe.resume != actual.resume
        || recipe.threads != actual.threads
    {
        return Err(bad(
            "Dyn method contract changes hold, access, resume, or thread bound",
        ));
    }
    match (&recipe.result, &actual.result) {
        (Some(DynResultSourceRecipe::Value), Some(DynResultSource::Value))
        | (Some(DynResultSourceRecipe::Create), Some(DynResultSource::Create))
        | (Some(DynResultSourceRecipe::Give), Some(DynResultSource::Give))
        | (None, None) => {}
        _ => {
            return Err(unknown(
                "Dyn method result needs its complete source recipe",
            ));
        }
    }
    match (&recipe.cleanup, &actual.cleanup) {
        (Some(recipe), Some(actual))
            if recipe.environment == actual.environment
                && recipe.positions == actual.positions
                && recipe.access == actual.access =>
        {
            exact_effects(&recipe.effects, &actual.effects)?;
        }
        (None, None) => {}
        _ => return Err(bad("Dyn method cleanup differs from its original contract")),
    }
    match (&recipe.effects, &actual.effects) {
        (Some(recipe), Some(actual)) => exact_effects(recipe, actual)?,
        (None, None) => {}
        _ => return Err(bad("Dyn method effect directory is incomplete")),
    }
    Ok(())
}

/// A separate, bounded check of the actual owned provider and its original
/// method identities. The caller proves the original trait predicate and
/// actual MakeDynOwned instruction in its own world before invoking this.
#[allow(clippy::too_many_arguments)]
pub(crate) fn owned_provider(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    interface_id: DynInterfaceId,
    vtable_id: DynVtableId,
    source: &Type,
    route: DynOwnedStorage,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    provider_fee(
        left,
        module
            .dyn_vtables
            .len()
            .saturating_add(module.dyn_interfaces.len())
            .saturating_add(module.dyn_declarations.len()),
    )?;
    let mut tables = module.dyn_vtables.iter().filter(|row| row.id == vtable_id);
    let table = tables
        .next()
        .ok_or_else(|| bad("owned Dyn has no exact actual provider table"))?;
    if tables.next().is_some()
        || table.interface != interface_id
        || &table.self_type != source
        || table.owned_storage != Some(route)
    {
        return Err(bad(
            "owned Dyn provider changes its exact table, Self, or route",
        ));
    }
    let mut interfaces = module
        .dyn_interfaces
        .iter()
        .filter(|row| row.id == interface_id);
    let interface = interfaces
        .next()
        .ok_or_else(|| bad("owned Dyn provider has no exact actual interface"))?;
    if interfaces.next().is_some() {
        return Err(bad("owned Dyn actual interface identity is ambiguous"));
    }
    let mut traits = module
        .dyn_declarations
        .iter()
        .filter(|row| row.id == interface.application.declaration);
    let trait_declaration = traits
        .next()
        .ok_or_else(|| bad("owned Dyn has no original trait declaration"))?;
    if traits.next().is_some() || trait_declaration.kind != DynDeclarationKind::Trait {
        return Err(bad("owned Dyn trait identity is ambiguous or wrong kind"));
    }
    let mut implementations = module
        .dyn_declarations
        .iter()
        .filter(|row| row.id == table.implementation);
    let implementation = implementations
        .next()
        .ok_or_else(|| bad("owned Dyn has no original impl declaration"))?;
    if implementations.next().is_some() || implementation.kind != DynDeclarationKind::Impl {
        return Err(bad("owned Dyn impl identity is ambiguous or wrong kind"));
    }
    let trait_scope = exact_scope(
        program,
        SemanticDynOwner::Declaration(trait_declaration.id),
        left,
    )?;
    if !matches!(&trait_scope.body, SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration })
        if *declaration == trait_declaration.id)
    {
        return Err(bad("Dyn trait scope differs from its original declaration"));
    }
    let impl_scope = exact_scope(
        program,
        SemanticDynOwner::Declaration(implementation.id),
        left,
    )?;
    if !matches!(&impl_scope.body, SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl { declaration, trait_application, .. })
        if *declaration == implementation.id && trait_application.declaration == trait_declaration.id)
    {
        return Err(bad(
            "Dyn impl scope differs from its original trait implementation",
        ));
    }
    let signatures = trait_declaration
        .method_signatures
        .as_ref()
        .ok_or_else(|| unknown("Dyn trait has no complete original method signature directory"))?;
    let definitions = implementation
        .method_definitions
        .as_ref()
        .ok_or_else(|| unknown("Dyn impl has no complete original method definition directory"))?;
    if signatures.len() != interface.slots.len()
        || definitions.len() != table.entries.len()
        || interface.slots.len() != table.entries.len()
    {
        return Err(bad("Dyn provider method directory omits an actual slot"));
    }
    if interface.slots.len() > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "Dyn provider method directory exceeds node limit",
            None,
        ));
    }
    provider_fee(
        left,
        signatures
            .len()
            .saturating_mul(4)
            .saturating_add(definitions.len()),
    )?;
    for (index, ((slot, entry), signature)) in interface
        .slots
        .iter()
        .zip(&table.entries)
        .zip(signatures)
        .enumerate()
    {
        let definition = definitions
            .get(index)
            .ok_or_else(|| bad("Dyn provider method definition is absent"))?;
        if slot.member.ordinal as usize != index
            || entry.slot as usize != index
            || signature.member as usize != index
            || definition.member as usize != index
            || definition.scope != DynDefinitionScope::ImplOverride
        {
            return Err(bad(
                "Dyn provider method ordinals or definition scope differ",
            ));
        }
        let original_trait_method = exact_scope(
            program,
            SemanticDynOwner::MethodSignature {
                declaration: trait_declaration.id,
                member: index as u32,
            },
            left,
        )?;
        if original_trait_method.parent != Some(trait_scope.id) {
            return Err(bad(
                "Dyn trait method is outside its original declaration scope",
            ));
        }
        exact_signature_recipe(&signature.signature, original_trait_method, left)?;
        exact_contract_recipe(&signature.signature.contract, &slot.contract)?;
        let original_provider = exact_scope(
            program,
            SemanticDynOwner::MethodDefinition(definition.id),
            left,
        )?;
        if original_provider.parent != Some(impl_scope.id) {
            return Err(bad(
                "Dyn provider method is outside its original impl scope",
            ));
        }
        exact_signature_recipe(&definition.signature, original_provider, left)?;
        exact_contract_recipe(&definition.signature.contract, &slot.contract)?;
        provider_fee(
            left,
            definition
                .instances
                .len()
                .saturating_add(program.instances.len()),
        )?;
        let frame = ledger
            .frames
            .get(&entry.function)
            .ok_or_else(|| bad("Dyn method has no actual emitter frame"))?;
        let mut specializations = definition.instances.iter().filter(|row| {
            matches!(row.function, DynFunctionTarget::Typed(function) if function == entry.function)
        });
        let specialization = specializations
            .next()
            .ok_or_else(|| bad("Dyn method has no exact original specialization"))?;
        if specializations.next().is_some()
            || specialization.receiver.ty != *source
            || specialization.declaration_actuals != table.actuals
            || !specialization.method_actuals.is_empty()
        {
            let mut diagnostic = bad("Dyn method specialization changes Self or complete actuals");
            inference::charge_type_snapshot(source, Span::default(), limits, left, 1)?;
            inference::charge_type_snapshot(
                &specialization.receiver.ty,
                Span::default(),
                limits,
                left,
                1,
            )?;
            diagnostic.notes.push(format!(
                "original provider {:?}, scope {:?}, actual function {:?}, Core origin {:?}; expected Self {:?}, specialization Self {:?}",
                original_provider.id, impl_scope.id, entry.function, frame.origin,
                source, specialization.receiver.ty
            ));
            return Err(diagnostic);
        }
        let mut instances = program.instances.iter().filter(|row| {
            row.definition == original_provider.id
                && matches!(row.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == entry.function)
        });
        let instance = instances
            .next()
            .ok_or_else(|| bad("Dyn provider method has no exact original emitted instance"))?;
        if instances.next().is_some() || instance.parent.is_none() {
            return Err(bad("Dyn provider method instance is ambiguous or unscoped"));
        }
        provider_fee(left, program.instances.len())?;
        let mut parents = program
            .instances
            .iter()
            .filter(|row| Some(row.id) == instance.parent);
        let parent = parents
            .next()
            .ok_or_else(|| bad("Dyn provider has no actual lexical impl scope"))?;
        if parents.next().is_some()
            || parent.definition != impl_scope.id
            || !matches!(parent.target, SemanticInstanceTarget::Scope)
        {
            return Err(bad(
                "Dyn provider changes its exact original lexical impl scope",
            ));
        }
        // Self is derived in the real provider's lexical domain, not from a
        // same-shaped table or a specialization's proposed receiver type.
        let expected_self = super::type_at(
            program,
            module,
            ledger,
            instance.id,
            &SemanticTypeRecipe::SelfType {
                scope: SemanticScopeRef::Ancestor(1),
            },
            Some(source),
            limits,
            left,
        )?;
        crate::dyn_metadata::semantic_operand_snapshot(&expected_self, limits, left, 1)?;
        let SemanticTypeValue::Resolved {
            ty: expected_type,
            life: expected_life,
            classes: expected_classes,
        } = &expected_self
        else {
            return Err(bad(
                "Dyn provider Self remains unresolved in its original domain",
            ));
        };
        if expected_type != source
            || expected_life != &specialization.receiver.life
            || expected_classes.len() != expected_life.binder_count as usize
        {
            return Err(bad(
                "Dyn provider specialization differs from its independently derived complete lexical Self",
            ));
        }
        let claimed_self = instance
            .self_type
            .as_ref()
            .ok_or_else(|| bad("Dyn provider instance omits its complete lexical Self"))?;
        crate::dyn_metadata::semantic_operand_snapshot(claimed_self, limits, left, 1)?;
        if claimed_self != &expected_self {
            return Err(bad(
                "Dyn provider instance changes its original complete Self domain",
            ));
        }
        let function = module
            .functions
            .iter()
            .find(|row| row.id == entry.function)
            .ok_or_else(|| bad("Dyn method entry has no actual Typed function"))?;
        if function.foreign.is_some()
            || function.parameters.len() != slot.parameters.len() + 1
            || function.return_type != slot.result
            || function.result_mode != slot.result_mode
            || function.call_kind != slot.call_kind
        {
            return Err(bad(
                "Dyn provider method changes its actual callable signature",
            ));
        }
        for (parameter, expected) in function.parameters.iter().skip(1).zip(&slot.parameters) {
            let local = function
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| bad("Dyn method parameter local is absent"))?;
            if local.ty != *expected {
                return Err(bad(
                    "Dyn provider method parameter changes actual slot type",
                ));
            }
        }
        if source.is_b0_scalar() && !table.self_classes.is_empty() {
            return Err(bad("Dyn scalar Self invents retained classes"));
        }
        if let Some(self_life) = &table.self_life {
            if self_life.binder_count as usize != table.self_classes.len()
                || specialization.receiver.life != *self_life
                || specialization.receiver.classes != table.self_classes
            {
                return Err(bad(
                    "Dyn method specialization changes complete Self domain",
                ));
            }
        } else if specialization.receiver.life.binder_count != 0
            || !specialization.receiver.classes.is_empty()
        {
            return Err(unknown(
                "Dyn method lacks its complete Self class inventory",
            ));
        }
        // Original body, type substitution and class flow are checked by the
        // normal semantic proof before this table can be published.
    }
    Ok(())
}

/// Authenticate the selected slot against its complete original declaration
/// directory. Caller arguments and all class images are proved by World.
#[allow(clippy::too_many_arguments)]
pub(crate) fn call_slot(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    interface_id: DynInterfaceId,
    slot_ordinal: u32,
    parent: SemanticDefinitionId,
    method: SemanticDefinitionId,
    member: u32,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    provider_fee(
        left,
        module
            .dyn_interfaces
            .len()
            .saturating_add(module.dyn_declarations.len()),
    )?;
    let mut interfaces = module
        .dyn_interfaces
        .iter()
        .filter(|row| row.id == interface_id);
    let interface = interfaces
        .next()
        .ok_or_else(|| bad("Dyn call has no actual interface directory"))?;
    if interfaces.next().is_some() {
        return Err(bad("Dyn call actual interface repeats"));
    }
    crate::dyn_metadata::actual_interface_snapshot(interface, limits, left, 1)?;
    let slot = interface
        .slots
        .get(slot_ordinal as usize)
        .ok_or_else(|| bad("Dyn call has no actual selected slot"))?;
    let mut declarations = module
        .dyn_declarations
        .iter()
        .filter(|row| row.id == slot.member.application.declaration);
    let declaration = declarations
        .next()
        .ok_or_else(|| bad("Dyn call has no original trait declaration"))?;
    if declarations.next().is_some() || declaration.kind != DynDeclarationKind::Trait {
        return Err(bad("Dyn call trait declaration repeats or changes kind"));
    }
    crate::dyn_metadata::actual_declaration_snapshot(declaration, limits, left, 1)?;
    let parent_scope = exact_scope(program, SemanticDynOwner::Declaration(declaration.id), left)?;
    if parent_scope.id != parent
        || parent_scope.identity != declaration.identity
        || !matches!(parent_scope.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration:id}) if id==declaration.id)
    {
        return Err(bad("Dyn slot changes its complete original trait scope"));
    }
    if slot.member.application != interface.application {
        return Err(bad(
            "Dyn selected root member changes the complete trait application",
        ));
    }
    if declaration.direct_supertraits.is_none()
        || declaration.requirements.is_none()
        || declaration.associated_definitions.is_none()
    {
        return Err(unknown(
            "Dyn call requires its complete original supertrait, where and associated directory",
        ));
    }
    if declaration.parameters.len() != parent_scope.parameters.len() {
        return Err(bad("Dyn original trait loses complete mixed formal kinds"));
    }
    provider_fee(left, declaration.parameters.len())?;
    for (kind, formal) in declaration.parameters.iter().zip(&parent_scope.parameters) {
        let matched = match (kind, formal) {
            (LifeParameterKind::Type, SemanticFormal::Type { .. })
            | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime) => true,
            (
                LifeParameterKind::Const(actual),
                SemanticFormal::Const {
                    carrier: SemanticTypeRecipe::Atom(expected),
                },
            ) => actual == expected,
            _ => false,
        };
        if !matched {
            return Err(bad(
                "Dyn original trait changes a mixed kind or Const carrier",
            ));
        }
    }
    if slot.member.ordinal != member {
        return Err(bad("Dyn slot selects another original member"));
    }
    let original = exact_scope(
        program,
        SemanticDynOwner::MethodSignature {
            declaration: declaration.id,
            member,
        },
        left,
    )?;
    if original.id != method
        || original.parent != Some(parent)
        || original.kind != SemanticDefinitionKind::Method
    {
        return Err(bad("Dyn slot method is outside its original trait scope"));
    }
    let Some(directory_member) = declaration.members.get(member as usize) else {
        return Err(bad("Dyn call original member ordinal is absent"));
    };
    let DynMemberKind::Method(kinds) = &directory_member.kind else {
        return Err(bad("Dyn call selects a nonmethod original member"));
    };
    provider_fee(
        left,
        kinds
            .len()
            .saturating_add(original.parameters.len())
            .saturating_add(original.identity.declaration_path.len())
            .saturating_add(parent_scope.identity.declaration_path.len()),
    )?;
    let prefix = &parent_scope.identity.declaration_path;
    let path = &original.identity.declaration_path;
    if !path.starts_with(prefix)
        || path.len() != prefix.len() + 3
        || path[prefix.len()] != "member"
        || path[prefix.len() + 1].parse::<u32>().ok() != Some(member)
        || path.last() != Some(&directory_member.name)
        || original.identity.package != parent_scope.identity.package
        || original.identity.module_path != parent_scope.identity.module_path
        || original.identity.generation_path != parent_scope.identity.generation_path
        || kinds.len() != original.parameters.len()
        || slot.actuals.len() != kinds.len()
    {
        return Err(bad(
            "Dyn slot loses its complete original member identity or mixed scheme",
        ));
    }
    for (kind, formal) in kinds.iter().zip(&original.parameters) {
        let matched = match (kind, formal) {
            (LifeParameterKind::Type, SemanticFormal::Type { .. })
            | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime) => true,
            (
                LifeParameterKind::Const(actual),
                SemanticFormal::Const {
                    carrier: SemanticTypeRecipe::Atom(expected),
                },
            ) => actual == expected,
            _ => false,
        };
        if !matched {
            return Err(bad(
                "Dyn slot changes an original generic kind or Const carrier",
            ));
        }
    }
    let signatures = declaration
        .method_signatures
        .as_ref()
        .ok_or_else(|| unknown("Dyn call omits its original method signature directory"))?;
    provider_fee(left, signatures.len())?;
    let mut signatures = signatures
        .iter()
        .filter(|signature| signature.member == member);
    let signature = signatures
        .next()
        .ok_or_else(|| bad("Dyn call original method signature is absent"))?;
    if signatures.next().is_some() {
        return Err(bad("Dyn call original method signature repeats"));
    }
    exact_signature_recipe(&signature.signature, original, left)?;
    exact_contract_recipe(&signature.signature.contract, &slot.contract)?;
    let original_signature = original_signature(original)?;
    if slot.parameter_modes.len() != slot.parameters.len()
        || original_signature.parameters.len() != slot.parameters.len().saturating_add(1)
        || slot.result_mode != original_signature.result_mode
        || slot.call_kind != original_signature.call_kind
        || slot.receiver.mode != original_signature.parameters[0].mode
    {
        return Err(bad(
            "Dyn slot changes its complete original signature modes or effect",
        ));
    }
    for (parameter, mode) in original_signature
        .parameters
        .iter()
        .skip(1)
        .zip(&slot.parameter_modes)
    {
        if parameter.mode != *mode {
            return Err(bad("Dyn slot changes an original parameter mode"));
        }
    }
    let proof = ledger.original.borrow();
    let proof = proof
        .as_ref()
        .ok_or_else(|| bad("Dyn slot has no checked original Gamma"))?;
    if !proof.complete.contains(&method) {
        return Err(bad(
            "Dyn slot method has no complete original body or declaration proof",
        ));
    }
    Ok(())
}

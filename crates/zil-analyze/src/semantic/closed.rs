//! Interpret original function instances and their actual local captured heads.
//! Empty metadata never selects a proof: the complete type, original signature,
//! creator, occurrence and operation inventories must independently agree.
use super::*;
mod capture_heads;
mod operations;

fn unknown(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::unsupported(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
fn empty(life: &StorageLifeShape) -> bool {
    life.binder_count == 0
        && life.slots.is_empty()
        && life.retained.is_empty()
        && life.nominal_arguments.is_empty()
}
struct Check<'a, 'i> {
    module: &'a TypedModule,
    program: &'a SemanticModule,
    inventory: &'i Inventory<'a>,
}
impl Check<'_, '_> {
    fn ordinary_type(&self, ty: &Type, b: &mut Budget) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if ty.is_b0_scalar() || *ty == Type::Unit {
            return Ok(());
        }
        let Type::Closure(id) = ty else {
            return Err(unknown(
                "semantic persistent or compound type needs complete original class substitution",
            ));
        };
        b.spend(self.module.closures.len().saturating_add(1), WORK)?;
        let definition = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|definition| definition.id == *id)
            .ok_or_else(|| fail("actual closure inventory identity differs"))?;
        // The complete actual directory also binds every active implicit
        // captured head to its original name occurrence and creator operand.
        self.capture_directory(*id, b)?;
        b.spend(
            definition
                .captures
                .len()
                .saturating_add(definition.parameters.len()),
            WORK,
        )?;
        if !definition
            .parameters
            .iter()
            .chain(std::iter::once(&definition.result))
            .all(|ty| ty.is_b0_scalar() || *ty == Type::Unit)
        {
            return Err(unknown(
                "semantic closure has a non-scalar actual capture or signature occurrence",
            ));
        }
        let entry = self
            .module
            .functions
            .get(definition.function.0 as usize)
            .filter(|entry| entry.id == definition.function && entry.environment == Some(*id))
            .ok_or_else(|| fail("actual closure type has another definition's entry"))?;
        if entry.return_type != definition.result
            || entry.parameters.len() != definition.parameters.len()
        {
            return Err(fail("actual closure entry loses its complete signature"));
        }
        for (local, ty) in entry.parameters.iter().zip(&definition.parameters) {
            if entry.locals.get(local.0 as usize).map(|local| &local.ty) != Some(ty) {
                return Err(fail("actual closure parameter differs from its directory"));
            }
        }
        Ok(())
    }
    fn value_type(&self, value: &SemanticTypeValue, b: &mut Budget) -> CompileResult<Type> {
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(fail("Core type survived into executable semantic context"));
        };
        if !empty(life) || !classes.is_empty() {
            return Err(unknown(
                "semantic type retains an original lifetime domain requiring class flow",
            ));
        }
        self.ordinary_type(ty, b)?;
        if let Type::Closure(id) = ty
            && !self.capture_directory(*id, b)?.is_empty()
        {
            return Err(fail(
                "classful actual closure cannot claim a known-empty type domain",
            ));
        }
        Ok(ty.clone())
    }
    fn actual_type(&self, ty: &Type, b: &mut Budget) -> CompileResult<()> {
        if matches!(ty, Type::Address { mutable: false, inner } if inner.is_b0_scalar()) {
            // This is physical classification only. Every occurrence still
            // obtains its actual head/loan/epoch in the ordinary kernels.
            b.spend(1, WORK)?;
            Ok(())
        } else {
            self.ordinary_type(ty, b)
        }
    }
    fn ancestor<'b>(
        &'b self,
        mut instance: &'b SemanticInstance,
        scope: SemanticScopeRef,
        b: &mut Budget,
    ) -> CompileResult<&'b SemanticInstance> {
        let SemanticScopeRef::Ancestor(depth) = scope;
        if depth as usize > b.partial_depth {
            return Err(fail("semantic lexical ancestor exceeds its finite bound"));
        }
        for _ in 0..depth {
            instance = self.inventory.instance(
                instance
                    .parent
                    .ok_or_else(|| fail("semantic lexical ancestor is absent"))?,
                b,
            )?;
        }
        Ok(instance)
    }
    fn ty(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Type> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(fail("original semantic type exceeds its finite bound"));
        }
        match recipe {
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() || *ty == Type::Unit => {
                Ok(ty.clone())
            }
            SemanticTypeRecipe::Exact(value) if empty(&value.life) && value.classes.is_empty() => {
                self.ordinary_type(&value.ty, b)?;
                if let Type::Closure(id) = &value.ty
                    && !self.capture_directory(*id, b)?.is_empty()
                {
                    return Err(fail(
                        "exact closure recipe erases its actual intrinsic head domain",
                    ));
                }
                Ok(value.ty.clone())
            }
            SemanticTypeRecipe::Formal(reference) => {
                let instance = self.ancestor(instance, reference.scope, b)?;
                let definition = self.inventory.definition(instance.definition, b)?;
                if !matches!(
                    definition.parameters.get(reference.ordinal as usize),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail("type recipe changes an original mixed formal's kind"));
                }
                match instance.actuals.get(reference.ordinal as usize) {
                    Some(SemanticActual::Type(value)) => self.value_type(value, b),
                    _ => Err(fail(
                        "type recipe has no complete actual at its original ordinal",
                    )),
                }
            }
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                let instance = self.ancestor(instance, *scope, b)?;
                let definition = self.inventory.definition(instance.definition, b)?;
                if !matches!(
                    definition
                        .environment
                        .get(*ordinal as usize)
                        .map(|formal| &formal.kind),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail(
                        "type environment selector changes its checked slot kind",
                    ));
                }
                match instance
                    .environment
                    .as_ref()
                    .and_then(|environment| environment.actuals.get(*ordinal as usize))
                {
                    Some(SemanticActual::Type(value)) => self.value_type(value, b),
                    _ => Err(fail("type environment has no sealed actual type")),
                }
            }
            SemanticTypeRecipe::Closure {
                definition,
                arguments,
                environment,
            } => {
                let mut found = None;
                for candidate in &self.program.instances {
                    b.spend(1, WORK)?;
                    if candidate.definition != *definition || candidate.parent != Some(instance.id)
                    {
                        continue;
                    }
                    let Some(actual_environment) = &candidate.environment else {
                        continue;
                    };
                    if !self.actual_vectors(
                        instance,
                        arguments,
                        &candidate.actuals,
                        b,
                        depth + 1,
                    )? || !self.actual_vectors(
                        instance,
                        environment,
                        &actual_environment.actuals,
                        b,
                        depth + 1,
                    )? {
                        continue;
                    }
                    let SemanticClosureTarget::Typed(id) = actual_environment.closure else {
                        return Err(fail("closure type retains an unresolved original target"));
                    };
                    if found.is_some_and(|previous| previous != id) {
                        return Err(fail(
                            "original closure application has ambiguous concrete identities",
                        ));
                    }
                    found = Some(id);
                }
                let ty =
                    Type::Closure(found.ok_or_else(|| {
                        fail("original closure type has no exact actual instance")
                    })?);
                self.ordinary_type(&ty, b)?;
                Ok(ty)
            }
            _ => Err(unknown(
                "original semantic type constructor requires complete class and nominal evaluation",
            )),
        }
    }
    fn actual_vectors(
        &self,
        context: &SemanticInstance,
        recipes: &[SemanticActualRecipe],
        actuals: &[SemanticActual],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        if depth > b.partial_depth {
            return Err(fail("original actual recipe exceeds its finite bound"));
        }
        b.spend(recipes.len().saturating_add(actuals.len()), WORK)?;
        if recipes.len() != actuals.len() {
            return Ok(false);
        }
        for (recipe, actual) in recipes.iter().zip(actuals) {
            match (recipe, actual) {
                (SemanticActualRecipe::Type(recipe), SemanticActual::Type(value)) => {
                    if self.ty(context, recipe, b, depth + 1)? != self.value_type(value, b)? {
                        return Ok(false);
                    }
                }
                _ => {
                    return Err(unknown(
                        "original const and lifetime actuals require their complete scoped interpretation",
                    ));
                }
            }
        }
        Ok(true)
    }
    fn requirements(
        &self,
        context: &SemanticInstance,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(requirements.len(), WORK)?;
        for requirement in requirements {
            match requirement {
                SemanticRequirementRecipe::Structural { ty, bound } => {
                    let ty = self.ty(context, ty, b, 0)?;
                    // These are actual scalar predicates, never a user trait
                    // with the same spelling or a claimed closure capability.
                    if !(ty.is_b0_scalar() || ty == Type::Unit) {
                        return Err(unknown(
                            "structural semantic bound requires its complete actual type proof",
                        ));
                    }
                    match bound {
                        DynStructuralBound::Copy
                        | DynStructuralBound::Value
                        | DynStructuralBound::SyncDrop => {}
                        DynStructuralBound::StoreElement if ty != Type::Unit => {}
                        DynStructuralBound::StoreElement => {
                            return Err(fail(
                                "zero-sized unit does not satisfy the actual Store element bound",
                            ));
                        }
                    }
                }
                SemanticRequirementRecipe::TypeEqual { left, right } => {
                    if self.ty(context, left, b, 0)? != self.ty(context, right, b, 0)? {
                        return Err(fail(
                            "actual type specialization does not satisfy its original equality",
                        ));
                    }
                }
                _ => {
                    return Err(unknown(
                        "original semantic premise needs independent caller and rigid-definition proof",
                    ));
                }
            }
        }
        Ok(())
    }
    fn function_instance(
        &self,
        target: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<&SemanticInstance> {
        b.spend(self.program.instances.len(), WORK)?;
        let mut matches = self.program.instances.iter().filter(|instance| matches!(instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == target));
        let instance = matches
            .next()
            .ok_or_else(|| fail("actual function has no original semantic instance"))?;
        if matches.next().is_some() {
            return Err(fail("actual function has ambiguous original instances"));
        }
        Ok(instance)
    }
    fn instances(&self, b: &mut Budget) -> CompileResult<()> {
        for instance in &self.program.instances {
            let definition = self.inventory.definition(instance.definition, b)?;
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                return Err(unknown(
                    "nominal and impl semantic contexts need their complete original evaluation",
                ));
            };
            let function = self.inventory.function(target, b)?;
            match (definition.kind, function.environment, &instance.environment) {
                (SemanticDefinitionKind::Function, None, None) => {}
                (SemanticDefinitionKind::Closure, Some(actual), Some(environment))
                    if environment.closure == SemanticClosureTarget::Typed(actual) => {}
                _ => {
                    return Err(unknown(
                        "original function kind requires its exact entry and creator environment proof",
                    ));
                }
            }
            if !matches!(
                instance.lifetime_instantiation,
                SemanticLifetimeInstantiation::Rigid
            ) || (function.environment.is_none() && !instance.header_embedding.is_empty())
                || instance.self_type.is_some()
            {
                return Err(unknown(
                    "semantic nonempty header or partition needs its complete rigid class proof",
                ));
            }
            let SemanticDefinitionBody::Function { signature, .. } = &definition.body else {
                return Err(fail(
                    "actual function is relabelled as another original kind",
                ));
            };
            if (function.environment.is_none() && !definition.implicit_lifetimes.is_empty())
                || signature.bound_lifetimes != 0
                || signature.invocation_lifetimes != 0
            {
                return Err(unknown(
                    "original function binder frames require scoped class interpretation",
                ));
            }
            if instance.actuals.len() != definition.parameters.len() {
                return Err(fail("original formal vector is incomplete"));
            }
            for (formal, actual) in definition.parameters.iter().zip(&instance.actuals) {
                match (formal, actual) {
                    (SemanticFormal::Type { requirements }, SemanticActual::Type(value)) => {
                        self.value_type(value, b)?;
                        self.requirements(instance, requirements, b)?;
                    }
                    _ => {
                        return Err(unknown(
                            "original mixed const or lifetime formals require complete rigid context",
                        ));
                    }
                }
            }
            if !definition.environment.is_empty() {
                let parent = self.inventory.instance(
                    instance.parent.ok_or_else(|| {
                        fail("semantic environment has no actual creator context")
                    })?,
                    b,
                )?;
                let actual = instance
                    .environment
                    .as_ref()
                    .ok_or_else(|| fail("semantic environment is omitted"))?;
                if definition.environment.len() != actual.actuals.len() {
                    return Err(fail("semantic environment omits an original slot"));
                }
                for (formal, value) in definition.environment.iter().zip(&actual.actuals) {
                    // Both the kind and source are interpreted in the real
                    // parent, before the child may read this checked slot.
                    match (&formal.kind, value) {
                        (SemanticFormal::Type { requirements }, SemanticActual::Type(value)) => {
                            self.value_type(value, b)?;
                            self.requirements(parent, requirements, b)?;
                            let SemanticActualRecipe::Type(source) = &formal.source else {
                                return Err(fail(
                                    "type environment source changes its original kind",
                                ));
                            };
                            if self.ty(parent, source, b, 0)? != self.value_type(value, b)? {
                                return Err(fail(
                                    "actual environment differs from its original creator source",
                                ));
                            }
                        }
                        _ => {
                            return Err(unknown(
                                "semantic life/const environment needs complete sealed creator proof",
                            ));
                        }
                    }
                }
            } else if instance
                .environment
                .as_ref()
                .is_some_and(|environment| !environment.actuals.is_empty())
            {
                return Err(fail(
                    "actual environment invents an undeclared semantic slot",
                ));
            }
            self.requirements(instance, &definition.requirements, b)?;
            b.spend(
                function
                    .parameters
                    .len()
                    .saturating_add(function.values.len())
                    .saturating_add(function.locals.len()),
                WORK,
            )?;
            if function.parameters.len() != signature.parameters.len()
                || function.call_kind != signature.call_kind
            {
                return Err(fail(
                    "actual function header changes its original signature",
                ));
            }
            for (parameter, parameter_recipe) in
                function.parameters.iter().zip(&signature.parameters)
            {
                let actual = function
                    .locals
                    .get(parameter.0 as usize)
                    .ok_or_else(|| fail("actual parameter local is absent"))?;
                let expected = self.ty(instance, &parameter_recipe.ty, b, 0)?;
                if !matches!(
                    parameter_recipe.mode,
                    PassingMode::Infer | PassingMode::Copy
                ) {
                    return Err(unknown(
                        "semantic parameter mode needs an independently checked resource relation",
                    ));
                }
                if actual.ty != expected || !(expected.is_b0_scalar() || expected == Type::Unit) {
                    return Err(unknown(
                        "actual parameter needs complete persistent signature substitution",
                    ));
                }
            }
            if function.return_type != self.ty(instance, &signature.result, b, 0)?
                || !(function.return_type.is_b0_scalar() || function.return_type == Type::Unit)
                || !(function.result_mode == signature.result_mode
                    || signature.result_mode == PassingMode::Infer)
            {
                return Err(fail(
                    "actual result differs from the original instantiated signature",
                ));
            }
            if function.environment.is_none()
                && function.storage_life.as_ref().is_some_and(|life| {
                    life.fixed_binder_count != 0
                        || life.invocation_binder_count != 0
                        || !life.bindings.is_empty()
                        || !life.static_bindings.is_empty()
                        || !life.classes.is_empty()
                })
            {
                return Err(unknown(
                    "actual header has a nonempty lifetime occurrence requiring class flow",
                ));
            }
            for ty in function
                .locals
                .iter()
                .map(|local| &local.ty)
                .chain(&function.values)
            {
                self.actual_type(ty, b)?;
            }
            if let Some(closure) = function.environment {
                self.capture_directory(closure, b)?;
            }
        }
        for function in &self.module.functions {
            self.function_instance(function.id, b)?;
        }
        Ok(())
    }
}

pub(super) fn check(
    module: &TypedModule,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    let checker = Check {
        module,
        program,
        inventory,
    };
    if !module.dyn_declarations.is_empty()
        || !module.dyn_interfaces.is_empty()
        || !module.dyn_vtables.is_empty()
        || !program.dyn_scopes.is_empty()
    {
        return Err(unknown(
            "semantic Dyn owners need their complete independently interpreted scope bridge",
        ));
    }
    checker.instances(b)?;
    checker.sites(b)?;
    // Actual operation/SSA/cleanup checks still run in the ordinary kernels.
    // No semantic context manufactures a target, loan, epoch or effect.
    Ok(())
}

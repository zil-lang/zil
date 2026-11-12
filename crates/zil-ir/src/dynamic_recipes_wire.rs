//! Bounded transport for declaration-scoped dynamic recipes.
use super::*;
impl Writer<'_> {
    pub(super) fn dyn_recipe_node(&mut self, depth: usize) -> WireResult<()> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(quota("dynamic recipe depth exceeded"));
        }
        self.charge(1)
    }
    pub(super) fn dyn_type_value(&mut self, value: &DynTypeValue, depth: usize) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.ty(&value.ty, depth + 1)?;
        self.storage_shape(&value.life)?;
        self.count((&value.classes).len(), self.limits.max_nodes)?;
        for item in &value.classes {
            self.storage_binder(item)?;
        }
        Ok(())
    }
    pub(super) fn dyn_life_pattern(
        &mut self,
        value: &DynLifePattern,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynLifePattern::DeclarationParameter(value_0) => {
                self.u8(0)?;
                self.u32(*value_0)?;
            }
            DynLifePattern::MethodParameter(value_0) => {
                self.u8(1)?;
                self.u32(*value_0)?;
            }
            DynLifePattern::Bound { depth, index } => {
                self.u8(2)?;
                self.u32(*depth)?;
                self.u32(*index)?;
            }
            DynLifePattern::Invocation(value_0) => {
                self.u8(3)?;
                self.u32(*value_0)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_obligation(&mut self, value: &DynObligation, depth: usize) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynObligation::Trait {
                receiver,
                application,
            } => {
                self.u8(0)?;
                self.dyn_type_pattern(receiver, depth + 1)?;
                self.dyn_self_pattern(application, depth + 1)?;
            }
            DynObligation::Callable {
                receiver,
                kind,
                quantified_lifetimes,
                signature,
            } => {
                self.u8(1)?;
                self.dyn_type_pattern(receiver, depth + 1)?;
                self.u8(match kind {
                    CallableKind::FunctionPointer => 0,
                    CallableKind::Fn => 1,
                    CallableKind::FnMut => 2,
                    CallableKind::FnOnce => 3,
                })?;
                self.u32(*quantified_lifetimes)?;
                self.dyn_signature_recipe(signature, depth + 1)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_parameter_recipe(
        &mut self,
        value: &DynParameterRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.dyn_type_pattern(&value.ty, depth + 1)?;
        self.dyn_passing_mode(&value.mode)?;
        Ok(())
    }
    pub(super) fn dyn_signature_recipe(
        &mut self,
        value: &DynSignatureRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.count((&value.parameters).len(), self.limits.max_nodes)?;
        for item in &value.parameters {
            self.dyn_parameter_recipe(item, depth + 1)?;
        }
        self.dyn_type_pattern(&value.result, depth + 1)?;
        self.dyn_passing_mode(&value.result_mode)?;
        self.dyn_call_kind(&value.call_kind)?;
        self.u32(*&value.invocation_binder_count)?;
        self.dyn_contract_recipe(&value.contract, depth + 1)?;
        Ok(())
    }
    pub(super) fn dyn_requirement_recipe(
        &mut self,
        value: &DynRequirementRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynRequirementRecipe::Read(value_0) => {
                self.u8(0)?;
                self.storage_position(value_0)?;
            }
            DynRequirementRecipe::Write(value_0) => {
                self.u8(1)?;
                self.storage_position(value_0)?;
            }
            DynRequirementRecipe::Manage(value_0) => {
                self.u8(2)?;
                self.storage_position(value_0)?;
            }
            DynRequirementRecipe::Apart(value_0, value_1) => {
                self.u8(3)?;
                self.storage_position(value_0)?;
                self.storage_position(value_1)?;
            }
            DynRequirementRecipe::State {
                position,
                ty,
                variant,
            } => {
                self.u8(4)?;
                self.storage_position(position)?;
                self.dyn_type_pattern(ty, depth + 1)?;
                self.u32(*variant)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_guarantee_recipe(
        &mut self,
        value: &DynGuaranteeRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynGuaranteeRecipe::StorageSources(value_0) => {
                self.u8(0)?;
                self.storage_source_binding(value_0, depth + 1)?;
            }
            DynGuaranteeRecipe::ManageResult => {
                self.u8(1)?;
            }
            DynGuaranteeRecipe::State {
                position,
                ty,
                variant,
            } => {
                self.u8(2)?;
                self.storage_position(position)?;
                self.dyn_type_pattern(ty, depth + 1)?;
                self.u32(*variant)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_effect_recipe(
        &mut self,
        value: &DynEffectRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynEffectRecipe::Alloc => {
                self.u8(0)?;
            }
            DynEffectRecipe::Io => {
                self.u8(1)?;
            }
            DynEffectRecipe::Block => {
                self.u8(2)?;
            }
            DynEffectRecipe::Suspend => {
                self.u8(3)?;
            }
            DynEffectRecipe::Spawn => {
                self.u8(4)?;
            }
            DynEffectRecipe::Ffi => {
                self.u8(5)?;
            }
            DynEffectRecipe::Reenter => {
                self.u8(6)?;
            }
            DynEffectRecipe::Panic => {
                self.u8(7)?;
            }
            DynEffectRecipe::Call(value_0) => {
                self.u8(8)?;
                self.storage_position(value_0)?;
            }
            DynEffectRecipe::Cleanup(value_0) => {
                self.u8(9)?;
                self.dyn_type_pattern(value_0, depth + 1)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_cleanup_recipe(
        &mut self,
        value: &DynCleanupRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.boolean(*&value.environment)?;
        self.count((&value.positions).len(), self.limits.max_nodes)?;
        for item in &value.positions {
            self.storage_position(item)?;
        }
        self.dyn_call_access(&value.access)?;
        self.count((&value.effects).len(), self.limits.max_nodes)?;
        for item in &value.effects {
            self.dyn_effect_recipe(item, depth + 1)?;
        }
        Ok(())
    }
    pub(super) fn dyn_contract_recipe(
        &mut self,
        value: &DynContractRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.boolean((&value.requires).is_some())?;
        if let Some(item) = &value.requires {
            self.count((item).len(), self.limits.max_nodes)?;
            for item in item {
                self.dyn_requirement_recipe(item, depth + 1)?;
            }
        }
        self.boolean((&value.guarantees).is_some())?;
        if let Some(item) = &value.guarantees {
            self.count((item).len(), self.limits.max_nodes)?;
            for item in item {
                self.dyn_guarantee_recipe(item, depth + 1)?;
            }
        }
        self.boolean((&value.hold).is_some())?;
        if let Some(item) = &value.hold {
            self.count((item).len(), self.limits.max_nodes)?;
            for item in item {
                self.storage_position(item)?;
            }
        }
        self.boolean((&value.call).is_some())?;
        if let Some(item) = &value.call {
            self.dyn_call_access(item)?;
        }
        self.boolean((&value.result).is_some())?;
        if let Some(item) = &value.result {
            self.dyn_result_source(item)?;
        }
        self.boolean((&value.cleanup).is_some())?;
        if let Some(item) = &value.cleanup {
            self.dyn_cleanup_recipe(item, depth + 1)?;
        }
        self.boolean((&value.effects).is_some())?;
        if let Some(item) = &value.effects {
            self.count((item).len(), self.limits.max_nodes)?;
            for item in item {
                self.dyn_effect_recipe(item, depth + 1)?;
            }
        }
        self.boolean((&value.resume).is_some())?;
        if let Some(item) = &value.resume {
            self.dyn_resume_bound(item)?;
        }
        self.boolean((&value.threads).is_some())?;
        if let Some(item) = &value.threads {
            self.dyn_thread_requirements(item)?;
        }
        Ok(())
    }
    pub(super) fn dyn_definition_id(
        &mut self,
        value: &DynDefinitionId,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.u32((&value.module).0)?;
        self.u32((&value.source).0)?;
        Ok(())
    }
    pub(super) fn dyn_definition_scope(
        &mut self,
        value: &DynDefinitionScope,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynDefinitionScope::TraitDefault => {
                self.u8(0)?;
            }
            DynDefinitionScope::ImplOverride => {
                self.u8(1)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_definition_declaration(
        &mut self,
        value: &DynDefinitionDeclaration,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.count((&value.requirements).len(), self.limits.max_nodes)?;
        for item in &value.requirements {
            self.dyn_obligation(item, depth + 1)?;
        }
        self.count((&value.instances).len(), self.limits.max_nodes)?;
        for item in &value.instances {
            self.dyn_definition_specialization(item, depth + 1)?;
        }
        self.dyn_definition_id(&value.id, depth + 1)?;
        self.declaration_identity(&value.identity)?;
        self.u32(*&value.member)?;
        self.dyn_definition_scope(&value.scope, depth + 1)?;
        self.dyn_signature_recipe(&value.signature, depth + 1)?;
        Ok(())
    }
    pub(super) fn dyn_method_recipe(
        &mut self,
        value: &DynMethodRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.count((&value.requirements).len(), self.limits.max_nodes)?;
        for item in &value.requirements {
            self.dyn_obligation(item, depth + 1)?;
        }
        self.u32(*&value.member)?;
        self.dyn_signature_recipe(&value.signature, depth + 1)?;
        self.boolean((&value.default_definition).is_some())?;
        if let Some(item) = &value.default_definition {
            self.dyn_definition_id(item, depth + 1)?;
        }
        Ok(())
    }
    pub(super) fn dyn_function_instance(
        &mut self,
        value: &DynFunctionInstance,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.dyn_definition_id(&value.definition, depth + 1)?;
        self.dyn_type_value(&value.receiver, depth + 1)?;
        self.count((&value.declaration_actuals).len(), self.limits.max_nodes)?;
        for item in &value.declaration_actuals {
            self.dyn_generic_actual(item)?;
        }
        self.count((&value.method_actuals).len(), self.limits.max_nodes)?;
        for item in &value.method_actuals {
            self.dyn_generic_actual(item)?;
        }
        Ok(())
    }
    pub(super) fn dyn_associated_value_recipe(
        &mut self,
        value: &DynAssociatedValueRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynAssociatedValueRecipe::Type(value_0) => {
                self.u8(0)?;
                self.dyn_type_pattern(value_0, depth + 1)?;
            }
            DynAssociatedValueRecipe::Constant { ty, value } => {
                self.u8(1)?;
                self.dyn_type_pattern(ty, depth + 1)?;
                self.dyn_const_pattern(value, depth + 1)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_associated_definition(
        &mut self,
        value: &DynAssociatedDefinition,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.boolean((&value.carrier).is_some())?;
        if let Some(item) = &value.carrier {
            self.dyn_type_pattern(item, depth + 1)?;
        }
        self.u32(*&value.member)?;
        self.boolean((&value.value).is_some())?;
        if let Some(item) = &value.value {
            self.dyn_associated_value_recipe(item, depth + 1)?;
        }
        self.count((&value.requirements).len(), self.limits.max_nodes)?;
        for item in &value.requirements {
            self.dyn_obligation(item, depth + 1)?;
        }
        Ok(())
    }
    pub(super) fn dyn_associated_source(
        &mut self,
        value: &DynAssociatedSource,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynAssociatedSource::TraitDefault {
                declaration,
                member,
            } => {
                self.u8(0)?;
                self.declaration_id(declaration)?;
                self.u32(*member)?;
            }
            DynAssociatedSource::ImplOverride {
                declaration,
                member,
            } => {
                self.u8(1)?;
                self.declaration_id(declaration)?;
                self.u32(*member)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_associated_value(
        &mut self,
        value: &DynAssociatedValue,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynAssociatedValue::Type(value_0) => {
                self.u8(0)?;
                self.dyn_type_value(value_0, depth + 1)?;
            }
            DynAssociatedValue::Constant { ty, value } => {
                self.u8(1)?;
                self.dyn_type_value(ty, depth + 1)?;
                self.constant(value)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_associated_resolution(
        &mut self,
        value: &DynAssociatedResolution,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.dyn_member(&value.member)?;
        self.dyn_associated_source(&value.source, depth + 1)?;
        self.dyn_associated_value(&value.value, depth + 1)?;
        Ok(())
    }
    pub(super) fn dyn_implementation_witness(
        &mut self,
        value: &DynImplementationWitness,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.dyn_type_value(&value.receiver, depth + 1)?;
        self.dyn_trait_application(&value.application)?;
        self.declaration_id(&value.implementation)?;
        self.count((&value.actuals).len(), self.limits.max_nodes)?;
        for item in &value.actuals {
            self.dyn_generic_actual(item)?;
        }
        self.count((&value.associated).len(), self.limits.max_nodes)?;
        for item in &value.associated {
            self.dyn_associated_resolution(item, depth + 1)?;
        }
        Ok(())
    }
    pub(super) fn dyn_function_target(
        &mut self,
        value: &DynFunctionTarget,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynFunctionTarget::Core { module, source } => {
                self.u8(0)?;
                self.u32((module).0)?;
                self.u32((source).0)?;
            }
            DynFunctionTarget::Typed(value_0) => {
                self.u8(1)?;
                self.u32((value_0).0)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_definition_specialization(
        &mut self,
        value: &DynDefinitionSpecialization,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.dyn_function_target(&value.function, depth + 1)?;
        self.dyn_type_value(&value.receiver, depth + 1)?;
        self.count((&value.declaration_actuals).len(), self.limits.max_nodes)?;
        for item in &value.declaration_actuals {
            self.dyn_generic_actual(item)?;
        }
        self.count((&value.method_actuals).len(), self.limits.max_nodes)?;
        for item in &value.method_actuals {
            self.dyn_generic_actual(item)?;
        }
        Ok(())
    }
    pub(super) fn dyn_self_pattern(
        &mut self,
        value: &DynSelfPattern,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.declaration_id(&value.declaration)?;
        self.count((&value.arguments).len(), self.limits.max_nodes)?;
        for item in &value.arguments {
            self.dyn_pattern_actual(item, depth + 1)?;
        }
        Ok(())
    }
    pub(super) fn dyn_pattern_actual(
        &mut self,
        value: &DynPatternActual,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynPatternActual::Type(value_0) => {
                self.u8(0)?;
                self.dyn_type_pattern(value_0, depth + 1)?;
            }
            DynPatternActual::Const(value_0) => {
                self.u8(1)?;
                self.dyn_const_pattern(value_0, depth + 1)?;
            }
            DynPatternActual::Life(value_0) => {
                self.u8(2)?;
                self.dyn_life_pattern(value_0, depth + 1)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_const_pattern(
        &mut self,
        value: &DynConstPattern,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynConstPattern::Value(value_0) => {
                self.u8(0)?;
                self.constant(value_0)?;
            }
            DynConstPattern::Parameter(value_0) => {
                self.u8(1)?;
                self.u32(*value_0)?;
            }
            DynConstPattern::MethodParameter(value_0) => {
                self.u8(2)?;
                self.u32(*value_0)?;
            }
            DynConstPattern::Unary { op, value, ty } => {
                self.u8(3)?;
                self.unary(*op)?;
                self.dyn_const_pattern(value, depth + 1)?;
                self.dyn_type_pattern(ty, depth + 1)?;
            }
            DynConstPattern::Binary {
                op,
                left,
                right,
                ty,
            } => {
                self.u8(4)?;
                self.binary(*op)?;
                self.dyn_const_pattern(left, depth + 1)?;
                self.dyn_const_pattern(right, depth + 1)?;
                self.dyn_type_pattern(ty, depth + 1)?;
            }
            DynConstPattern::Convert { value, ty } => {
                self.u8(5)?;
                self.dyn_const_pattern(value, depth + 1)?;
                self.dyn_type_pattern(ty, depth + 1)?;
            }
            DynConstPattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                self.u8(6)?;
                self.dyn_type_pattern(receiver, depth + 1)?;
                self.dyn_self_pattern(application, depth + 1)?;
                self.u32(*member)?;
            }
            DynConstPattern::IntegerLiteral(value_0) => {
                self.u8(7)?;
                self.text(value_0)?;
            }
            DynConstPattern::FloatLiteral(value_0) => {
                self.u8(8)?;
                self.text(value_0)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_type_pattern(
        &mut self,
        value: &DynTypePattern,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        match value {
            DynTypePattern::Concrete(value_0) => {
                self.u8(0)?;
                self.ty(value_0, depth + 1)?;
            }
            DynTypePattern::Parameter(value_0) => {
                self.u8(1)?;
                self.u32(*value_0)?;
            }
            DynTypePattern::Nominal(value_0) => {
                self.u8(2)?;
                self.dyn_self_pattern(value_0, depth + 1)?;
            }
            DynTypePattern::Address {
                mutable,
                lifetime,
                inner,
            } => {
                self.u8(3)?;
                self.boolean(*mutable)?;
                self.boolean((lifetime).is_some())?;
                if let Some(item) = lifetime {
                    self.dyn_life_pattern(item, depth + 1)?;
                }
                self.dyn_type_pattern(inner, depth + 1)?;
            }
            DynTypePattern::Tuple(value_0) => {
                self.u8(4)?;
                self.count((value_0).len(), self.limits.max_nodes)?;
                for item in value_0 {
                    self.dyn_type_pattern(item, depth + 1)?;
                }
            }
            DynTypePattern::Array { element, length } => {
                self.u8(5)?;
                self.dyn_type_pattern(element, depth + 1)?;
                self.dyn_const_pattern(length, depth + 1)?;
            }
            DynTypePattern::Slice(value_0) => {
                self.u8(6)?;
                self.dyn_type_pattern(value_0, depth + 1)?;
            }
            DynTypePattern::Managed(value_0) => {
                self.u8(7)?;
                self.dyn_type_pattern(value_0, depth + 1)?;
            }
            DynTypePattern::Desc(value_0) => {
                self.u8(8)?;
                self.dyn_type_pattern(value_0, depth + 1)?;
            }
            DynTypePattern::OwnedStore(value_0) => {
                self.u8(9)?;
                self.dyn_type_pattern(value_0, depth + 1)?;
            }
            DynTypePattern::ErrorUnion { error, result } => {
                self.u8(10)?;
                self.dyn_type_pattern(error, depth + 1)?;
                self.dyn_type_pattern(result, depth + 1)?;
            }
            DynTypePattern::SelfType => {
                self.u8(11)?;
            }
            DynTypePattern::MethodParameter(value_0) => {
                self.u8(12)?;
                self.u32(*value_0)?;
            }
            DynTypePattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                self.u8(13)?;
                self.dyn_type_pattern(receiver, depth + 1)?;
                self.dyn_self_pattern(application, depth + 1)?;
                self.u32(*member)?;
            }
        }
        Ok(())
    }
    pub(super) fn storage_source_binding(
        &mut self,
        value: &StorageSourceBinding,
        depth: usize,
    ) -> WireResult<()> {
        self.dyn_recipe_node(depth)?;
        self.storage_position(&value.position)?;
        self.count((&value.sources).len(), self.limits.max_nodes)?;
        for item in &value.sources {
            self.storage_position(item)?;
        }
        Ok(())
    }
}
impl Reader<'_> {
    pub(super) fn dyn_recipe_node(&mut self, depth: usize) -> WireResult<()> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(quota("dynamic recipe depth exceeded"));
        }
        self.charge(1)
    }
    pub(super) fn dyn_type_value(&mut self, depth: usize) -> WireResult<DynTypeValue> {
        self.dyn_recipe_node(depth)?;
        Ok(DynTypeValue {
            ty: self.ty(depth + 1)?,
            life: self.storage_shape()?,
            classes: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.storage_binder()?);
                }
                rows
            },
        })
    }
    pub(super) fn dyn_life_pattern(&mut self, depth: usize) -> WireResult<DynLifePattern> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynLifePattern::DeclarationParameter(self.u32()?),
            1 => DynLifePattern::MethodParameter(self.u32()?),
            2 => DynLifePattern::Bound {
                depth: self.u32()?,
                index: self.u32()?,
            },
            3 => DynLifePattern::Invocation(self.u32()?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_obligation(&mut self, depth: usize) -> WireResult<DynObligation> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynObligation::Trait {
                receiver: self.dyn_type_pattern(depth + 1)?,
                application: self.dyn_self_pattern(depth + 1)?,
            },
            1 => DynObligation::Callable {
                receiver: self.dyn_type_pattern(depth + 1)?,
                kind: self.callable_kind()?,
                quantified_lifetimes: self.u32()?,
                signature: Box::new(self.dyn_signature_recipe(depth + 1)?),
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_parameter_recipe(&mut self, depth: usize) -> WireResult<DynParameterRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(DynParameterRecipe {
            ty: self.dyn_type_pattern(depth + 1)?,
            mode: self.dyn_passing_mode()?,
        })
    }
    pub(super) fn dyn_signature_recipe(&mut self, depth: usize) -> WireResult<DynSignatureRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(DynSignatureRecipe {
            parameters: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_parameter_recipe(depth + 1)?);
                }
                rows
            },
            result: self.dyn_type_pattern(depth + 1)?,
            result_mode: self.dyn_passing_mode()?,
            call_kind: self.dyn_call_kind()?,
            invocation_binder_count: self.u32()?,
            contract: self.dyn_contract_recipe(depth + 1)?,
        })
    }
    pub(super) fn dyn_requirement_recipe(
        &mut self,
        depth: usize,
    ) -> WireResult<DynRequirementRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynRequirementRecipe::Read(self.storage_position()?),
            1 => DynRequirementRecipe::Write(self.storage_position()?),
            2 => DynRequirementRecipe::Manage(self.storage_position()?),
            3 => DynRequirementRecipe::Apart(self.storage_position()?, self.storage_position()?),
            4 => DynRequirementRecipe::State {
                position: self.storage_position()?,
                ty: self.dyn_type_pattern(depth + 1)?,
                variant: self.u32()?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_guarantee_recipe(&mut self, depth: usize) -> WireResult<DynGuaranteeRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynGuaranteeRecipe::StorageSources(self.storage_source_binding(depth + 1)?),
            1 => DynGuaranteeRecipe::ManageResult,
            2 => DynGuaranteeRecipe::State {
                position: self.storage_position()?,
                ty: self.dyn_type_pattern(depth + 1)?,
                variant: self.u32()?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_effect_recipe(&mut self, depth: usize) -> WireResult<DynEffectRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynEffectRecipe::Alloc,
            1 => DynEffectRecipe::Io,
            2 => DynEffectRecipe::Block,
            3 => DynEffectRecipe::Suspend,
            4 => DynEffectRecipe::Spawn,
            5 => DynEffectRecipe::Ffi,
            6 => DynEffectRecipe::Reenter,
            7 => DynEffectRecipe::Panic,
            8 => DynEffectRecipe::Call(self.storage_position()?),
            9 => DynEffectRecipe::Cleanup(self.dyn_type_pattern(depth + 1)?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_cleanup_recipe(&mut self, depth: usize) -> WireResult<DynCleanupRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(DynCleanupRecipe {
            environment: self.boolean()?,
            positions: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.storage_position()?);
                }
                rows
            },
            access: self.dyn_call_access()?,
            effects: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_effect_recipe(depth + 1)?);
                }
                rows
            },
        })
    }
    pub(super) fn dyn_contract_recipe(&mut self, depth: usize) -> WireResult<DynContractRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(DynContractRecipe {
            requires: if self.boolean()? {
                Some({
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut rows = Vec::with_capacity(count);
                    for _ in 0..count {
                        rows.push(self.dyn_requirement_recipe(depth + 1)?);
                    }
                    rows
                })
            } else {
                None
            },
            guarantees: if self.boolean()? {
                Some({
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut rows = Vec::with_capacity(count);
                    for _ in 0..count {
                        rows.push(self.dyn_guarantee_recipe(depth + 1)?);
                    }
                    rows
                })
            } else {
                None
            },
            hold: if self.boolean()? {
                Some({
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut rows = Vec::with_capacity(count);
                    for _ in 0..count {
                        rows.push(self.storage_position()?);
                    }
                    rows
                })
            } else {
                None
            },
            call: if self.boolean()? {
                Some(self.dyn_call_access()?)
            } else {
                None
            },
            result: if self.boolean()? {
                Some(self.dyn_result_source()?)
            } else {
                None
            },
            cleanup: if self.boolean()? {
                Some(self.dyn_cleanup_recipe(depth + 1)?)
            } else {
                None
            },
            effects: if self.boolean()? {
                Some({
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut rows = Vec::with_capacity(count);
                    for _ in 0..count {
                        rows.push(self.dyn_effect_recipe(depth + 1)?);
                    }
                    rows
                })
            } else {
                None
            },
            resume: if self.boolean()? {
                Some(self.dyn_resume_bound()?)
            } else {
                None
            },
            threads: if self.boolean()? {
                Some(self.dyn_thread_requirements()?)
            } else {
                None
            },
        })
    }
    pub(super) fn dyn_definition_id(&mut self, depth: usize) -> WireResult<DynDefinitionId> {
        self.dyn_recipe_node(depth)?;
        Ok(DynDefinitionId {
            module: ModuleId(self.u32()?),
            source: NodeId(self.u32()?),
        })
    }
    pub(super) fn dyn_definition_scope(&mut self, depth: usize) -> WireResult<DynDefinitionScope> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynDefinitionScope::TraitDefault,
            1 => DynDefinitionScope::ImplOverride,
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_definition_declaration(
        &mut self,
        depth: usize,
    ) -> WireResult<DynDefinitionDeclaration> {
        self.dyn_recipe_node(depth)?;
        Ok(DynDefinitionDeclaration {
            requirements: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_obligation(depth + 1)?);
                }
                rows
            },
            instances: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_definition_specialization(depth + 1)?);
                }
                rows
            },
            id: self.dyn_definition_id(depth + 1)?,
            identity: self.declaration_identity()?,
            member: self.u32()?,
            scope: self.dyn_definition_scope(depth + 1)?,
            signature: self.dyn_signature_recipe(depth + 1)?,
        })
    }
    pub(super) fn dyn_method_recipe(&mut self, depth: usize) -> WireResult<DynMethodRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(DynMethodRecipe {
            requirements: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_obligation(depth + 1)?);
                }
                rows
            },
            member: self.u32()?,
            signature: self.dyn_signature_recipe(depth + 1)?,
            default_definition: if self.boolean()? {
                Some(self.dyn_definition_id(depth + 1)?)
            } else {
                None
            },
        })
    }
    pub(super) fn dyn_function_instance(
        &mut self,
        depth: usize,
    ) -> WireResult<DynFunctionInstance> {
        self.dyn_recipe_node(depth)?;
        Ok(DynFunctionInstance {
            definition: self.dyn_definition_id(depth + 1)?,
            receiver: self.dyn_type_value(depth + 1)?,
            declaration_actuals: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_generic_actual()?);
                }
                rows
            },
            method_actuals: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_generic_actual()?);
                }
                rows
            },
        })
    }
    pub(super) fn dyn_associated_value_recipe(
        &mut self,
        depth: usize,
    ) -> WireResult<DynAssociatedValueRecipe> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynAssociatedValueRecipe::Type(self.dyn_type_pattern(depth + 1)?),
            1 => DynAssociatedValueRecipe::Constant {
                ty: self.dyn_type_pattern(depth + 1)?,
                value: self.dyn_const_pattern(depth + 1)?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_associated_definition(
        &mut self,
        depth: usize,
    ) -> WireResult<DynAssociatedDefinition> {
        self.dyn_recipe_node(depth)?;
        Ok(DynAssociatedDefinition {
            carrier: if self.boolean()? {
                Some(self.dyn_type_pattern(depth + 1)?)
            } else {
                None
            },
            member: self.u32()?,
            value: if self.boolean()? {
                Some(self.dyn_associated_value_recipe(depth + 1)?)
            } else {
                None
            },
            requirements: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_obligation(depth + 1)?);
                }
                rows
            },
        })
    }
    pub(super) fn dyn_associated_source(
        &mut self,
        depth: usize,
    ) -> WireResult<DynAssociatedSource> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynAssociatedSource::TraitDefault {
                declaration: self.declaration_id()?,
                member: self.u32()?,
            },
            1 => DynAssociatedSource::ImplOverride {
                declaration: self.declaration_id()?,
                member: self.u32()?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_associated_value(&mut self, depth: usize) -> WireResult<DynAssociatedValue> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynAssociatedValue::Type(self.dyn_type_value(depth + 1)?),
            1 => DynAssociatedValue::Constant {
                ty: self.dyn_type_value(depth + 1)?,
                value: self.constant()?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_associated_resolution(
        &mut self,
        depth: usize,
    ) -> WireResult<DynAssociatedResolution> {
        self.dyn_recipe_node(depth)?;
        Ok(DynAssociatedResolution {
            member: self.dyn_member()?,
            source: self.dyn_associated_source(depth + 1)?,
            value: self.dyn_associated_value(depth + 1)?,
        })
    }
    pub(super) fn dyn_implementation_witness(
        &mut self,
        depth: usize,
    ) -> WireResult<DynImplementationWitness> {
        self.dyn_recipe_node(depth)?;
        Ok(DynImplementationWitness {
            receiver: self.dyn_type_value(depth + 1)?,
            application: self.dyn_trait_application()?,
            implementation: self.declaration_id()?,
            actuals: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_generic_actual()?);
                }
                rows
            },
            associated: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_associated_resolution(depth + 1)?);
                }
                rows
            },
        })
    }
    pub(super) fn dyn_function_target(&mut self, depth: usize) -> WireResult<DynFunctionTarget> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynFunctionTarget::Core {
                module: ModuleId(self.u32()?),
                source: NodeId(self.u32()?),
            },
            1 => DynFunctionTarget::Typed(FunctionId(self.u32()?)),
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_definition_specialization(
        &mut self,
        depth: usize,
    ) -> WireResult<DynDefinitionSpecialization> {
        self.dyn_recipe_node(depth)?;
        Ok(DynDefinitionSpecialization {
            function: self.dyn_function_target(depth + 1)?,
            receiver: self.dyn_type_value(depth + 1)?,
            declaration_actuals: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_generic_actual()?);
                }
                rows
            },
            method_actuals: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_generic_actual()?);
                }
                rows
            },
        })
    }
    pub(super) fn dyn_self_pattern(&mut self, depth: usize) -> WireResult<DynSelfPattern> {
        self.dyn_recipe_node(depth)?;
        Ok(DynSelfPattern {
            declaration: self.declaration_id()?,
            arguments: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_pattern_actual(depth + 1)?);
                }
                rows
            },
        })
    }
    pub(super) fn dyn_pattern_actual(&mut self, depth: usize) -> WireResult<DynPatternActual> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynPatternActual::Type(self.dyn_type_pattern(depth + 1)?),
            1 => DynPatternActual::Const(self.dyn_const_pattern(depth + 1)?),
            2 => DynPatternActual::Life(self.dyn_life_pattern(depth + 1)?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_const_pattern(&mut self, depth: usize) -> WireResult<DynConstPattern> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynConstPattern::Value(self.constant()?),
            1 => DynConstPattern::Parameter(self.u32()?),
            2 => DynConstPattern::MethodParameter(self.u32()?),
            3 => DynConstPattern::Unary {
                op: self.unary()?,
                value: Box::new(self.dyn_const_pattern(depth + 1)?),
                ty: Box::new(self.dyn_type_pattern(depth + 1)?),
            },
            4 => DynConstPattern::Binary {
                op: self.binary()?,
                left: Box::new(self.dyn_const_pattern(depth + 1)?),
                right: Box::new(self.dyn_const_pattern(depth + 1)?),
                ty: Box::new(self.dyn_type_pattern(depth + 1)?),
            },
            5 => DynConstPattern::Convert {
                value: Box::new(self.dyn_const_pattern(depth + 1)?),
                ty: Box::new(self.dyn_type_pattern(depth + 1)?),
            },
            6 => DynConstPattern::AssociatedProjection {
                receiver: Box::new(self.dyn_type_pattern(depth + 1)?),
                application: Box::new(self.dyn_self_pattern(depth + 1)?),
                member: self.u32()?,
            },
            7 => DynConstPattern::IntegerLiteral(self.text()?),
            8 => DynConstPattern::FloatLiteral(self.text()?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn dyn_type_pattern(&mut self, depth: usize) -> WireResult<DynTypePattern> {
        self.dyn_recipe_node(depth)?;
        Ok(match self.u8()? {
            0 => DynTypePattern::Concrete(self.ty(depth + 1)?),
            1 => DynTypePattern::Parameter(self.u32()?),
            2 => DynTypePattern::Nominal(Box::new(self.dyn_self_pattern(depth + 1)?)),
            3 => DynTypePattern::Address {
                mutable: self.boolean()?,
                lifetime: if self.boolean()? {
                    Some(self.dyn_life_pattern(depth + 1)?)
                } else {
                    None
                },
                inner: Box::new(self.dyn_type_pattern(depth + 1)?),
            },
            4 => DynTypePattern::Tuple({
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.dyn_type_pattern(depth + 1)?);
                }
                rows
            }),
            5 => DynTypePattern::Array {
                element: Box::new(self.dyn_type_pattern(depth + 1)?),
                length: self.dyn_const_pattern(depth + 1)?,
            },
            6 => DynTypePattern::Slice(Box::new(self.dyn_type_pattern(depth + 1)?)),
            7 => DynTypePattern::Managed(Box::new(self.dyn_type_pattern(depth + 1)?)),
            8 => DynTypePattern::Desc(Box::new(self.dyn_type_pattern(depth + 1)?)),
            9 => DynTypePattern::OwnedStore(Box::new(self.dyn_type_pattern(depth + 1)?)),
            10 => DynTypePattern::ErrorUnion {
                error: Box::new(self.dyn_type_pattern(depth + 1)?),
                result: Box::new(self.dyn_type_pattern(depth + 1)?),
            },
            11 => DynTypePattern::SelfType,
            12 => DynTypePattern::MethodParameter(self.u32()?),
            13 => DynTypePattern::AssociatedProjection {
                receiver: Box::new(self.dyn_type_pattern(depth + 1)?),
                application: Box::new(self.dyn_self_pattern(depth + 1)?),
                member: self.u32()?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
        })
    }
    pub(super) fn storage_source_binding(
        &mut self,
        depth: usize,
    ) -> WireResult<StorageSourceBinding> {
        self.dyn_recipe_node(depth)?;
        Ok(StorageSourceBinding {
            position: self.storage_position()?,
            sources: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(self.storage_position()?);
                }
                rows
            },
        })
    }
}

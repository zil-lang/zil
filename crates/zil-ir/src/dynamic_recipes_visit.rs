//! Shared structural visitor for raw recipe accounting, never semantic proof.
//! Consumers supply their own bounded callbacks before cloning or hashing.
use crate::*;
pub trait DynRecipeVisitor {
    type Error;
    fn node(&mut self, depth: usize) -> Result<(), Self::Error>;
    fn rows(&mut self, length: usize, depth: usize) -> Result<(), Self::Error>;
    fn ty(&mut self, ty: &Type, depth: usize) -> Result<(), Self::Error>;
    fn life(&mut self, life: &StorageLifeShape, depth: usize) -> Result<(), Self::Error>;
    fn identity(
        &mut self,
        identity: &LifeDeclarationIdentity,
        depth: usize,
    ) -> Result<(), Self::Error>;
    fn position(&mut self, position: &StoragePosition, depth: usize) -> Result<(), Self::Error>;
    fn constant(&mut self, constant: &Constant, depth: usize) -> Result<(), Self::Error>;
    fn text(&mut self, text: &str, depth: usize) -> Result<(), Self::Error>;
}
pub trait DynRecipeMetadata {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error>;
}
impl DynRecipeMetadata for DynTypeValue {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        visitor.ty(&self.ty, nesting + 1)?;
        visitor.life(&self.life, nesting + 1)?;
        visitor.rows((&self.classes).len(), nesting + 1)?;
        for item in &self.classes {
            let _ = item;
            visitor.node(nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynLifePattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynLifePattern::DeclarationParameter(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
            DynLifePattern::MethodParameter(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
            DynLifePattern::Bound { depth, index } => {
                let _ = depth;
                visitor.node(nesting + 1)?;
                let _ = index;
                visitor.node(nesting + 1)?;
            }
            DynLifePattern::Invocation(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynObligation {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynObligation::Trait {
                receiver,
                application,
            } => {
                (receiver).visit_dyn_metadata(visitor, nesting + 1)?;
                (application).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynObligation::Callable {
                receiver,
                kind,
                quantified_lifetimes,
                signature,
            } => {
                (receiver).visit_dyn_metadata(visitor, nesting + 1)?;
                let _ = kind;
                visitor.node(nesting + 1)?;
                let _ = quantified_lifetimes;
                visitor.node(nesting + 1)?;
                (signature).visit_dyn_metadata(visitor, nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynParameterRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        (&self.ty).visit_dyn_metadata(visitor, nesting + 1)?;
        let _ = &self.mode;
        visitor.node(nesting + 1)?;
        Ok(())
    }
}
impl DynRecipeMetadata for DynSignatureRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        visitor.rows((&self.parameters).len(), nesting + 1)?;
        for item in &self.parameters {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        (&self.result).visit_dyn_metadata(visitor, nesting + 1)?;
        let _ = &self.result_mode;
        visitor.node(nesting + 1)?;
        let _ = &self.call_kind;
        visitor.node(nesting + 1)?;
        let _ = &self.invocation_binder_count;
        visitor.node(nesting + 1)?;
        (&self.contract).visit_dyn_metadata(visitor, nesting + 1)?;
        Ok(())
    }
}
impl DynRecipeMetadata for DynRequirementRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynRequirementRecipe::Read(value_0) => {
                visitor.position(value_0, nesting + 1)?;
            }
            DynRequirementRecipe::Write(value_0) => {
                visitor.position(value_0, nesting + 1)?;
            }
            DynRequirementRecipe::Manage(value_0) => {
                visitor.position(value_0, nesting + 1)?;
            }
            DynRequirementRecipe::Apart(value_0, value_1) => {
                visitor.position(value_0, nesting + 1)?;
                visitor.position(value_1, nesting + 1)?;
            }
            DynRequirementRecipe::State {
                position,
                ty,
                variant,
            } => {
                visitor.position(position, nesting + 1)?;
                (ty).visit_dyn_metadata(visitor, nesting + 1)?;
                let _ = variant;
                visitor.node(nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynGuaranteeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynGuaranteeRecipe::StorageSources(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynGuaranteeRecipe::ManageResult => {}
            DynGuaranteeRecipe::State {
                position,
                ty,
                variant,
            } => {
                visitor.position(position, nesting + 1)?;
                (ty).visit_dyn_metadata(visitor, nesting + 1)?;
                let _ = variant;
                visitor.node(nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynEffectRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynEffectRecipe::Alloc => {}
            DynEffectRecipe::Io => {}
            DynEffectRecipe::Block => {}
            DynEffectRecipe::Suspend => {}
            DynEffectRecipe::Spawn => {}
            DynEffectRecipe::Ffi => {}
            DynEffectRecipe::Reenter => {}
            DynEffectRecipe::Panic => {}
            DynEffectRecipe::Call(value_0) => {
                visitor.position(value_0, nesting + 1)?;
            }
            DynEffectRecipe::Cleanup(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynCleanupRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        let _ = &self.environment;
        visitor.node(nesting + 1)?;
        visitor.rows((&self.positions).len(), nesting + 1)?;
        for item in &self.positions {
            visitor.position(item, nesting + 1)?;
        }
        (&self.access).visit_dyn_metadata(visitor, nesting + 1)?;
        visitor.rows((&self.effects).len(), nesting + 1)?;
        for item in &self.effects {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynContractRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.requires {
            visitor.rows((item).len(), nesting + 1)?;
            for item in item {
                (item).visit_dyn_metadata(visitor, nesting + 1)?;
            }
        }
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.guarantees {
            visitor.rows((item).len(), nesting + 1)?;
            for item in item {
                (item).visit_dyn_metadata(visitor, nesting + 1)?;
            }
        }
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.hold {
            visitor.rows((item).len(), nesting + 1)?;
            for item in item {
                visitor.position(item, nesting + 1)?;
            }
        }
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.call {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.result {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.cleanup {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.effects {
            visitor.rows((item).len(), nesting + 1)?;
            for item in item {
                (item).visit_dyn_metadata(visitor, nesting + 1)?;
            }
        }
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.resume {
            let _ = item;
            visitor.node(nesting + 1)?;
        }
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.threads {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynDefinitionId {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        let _ = &self.module;
        visitor.node(nesting + 1)?;
        let _ = &self.source;
        visitor.node(nesting + 1)?;
        Ok(())
    }
}
impl DynRecipeMetadata for DynDefinitionScope {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynDefinitionScope::TraitDefault => {}
            DynDefinitionScope::ImplOverride => {}
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynDefinitionDeclaration {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        visitor.rows((&self.requirements).len(), nesting + 1)?;
        for item in &self.requirements {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        visitor.rows((&self.instances).len(), nesting + 1)?;
        for item in &self.instances {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        (&self.id).visit_dyn_metadata(visitor, nesting + 1)?;
        visitor.identity(&self.identity, nesting + 1)?;
        let _ = &self.member;
        visitor.node(nesting + 1)?;
        (&self.scope).visit_dyn_metadata(visitor, nesting + 1)?;
        (&self.signature).visit_dyn_metadata(visitor, nesting + 1)?;
        Ok(())
    }
}
impl DynRecipeMetadata for DynMethodRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        visitor.rows((&self.requirements).len(), nesting + 1)?;
        for item in &self.requirements {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        let _ = &self.member;
        visitor.node(nesting + 1)?;
        (&self.signature).visit_dyn_metadata(visitor, nesting + 1)?;
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.default_definition {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynFunctionInstance {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        (&self.definition).visit_dyn_metadata(visitor, nesting + 1)?;
        (&self.receiver).visit_dyn_metadata(visitor, nesting + 1)?;
        visitor.rows((&self.declaration_actuals).len(), nesting + 1)?;
        for item in &self.declaration_actuals {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        visitor.rows((&self.method_actuals).len(), nesting + 1)?;
        for item in &self.method_actuals {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynAssociatedValueRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynAssociatedValueRecipe::Type(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynAssociatedValueRecipe::Constant { ty, value } => {
                (ty).visit_dyn_metadata(visitor, nesting + 1)?;
                (value).visit_dyn_metadata(visitor, nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynAssociatedDefinition {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.carrier {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        let _ = &self.member;
        visitor.node(nesting + 1)?;
        visitor.node(nesting + 1)?;
        if let Some(item) = &self.value {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        visitor.rows((&self.requirements).len(), nesting + 1)?;
        for item in &self.requirements {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynAssociatedSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynAssociatedSource::TraitDefault {
                declaration,
                member,
            } => {
                let _ = declaration;
                visitor.node(nesting + 1)?;
                let _ = member;
                visitor.node(nesting + 1)?;
            }
            DynAssociatedSource::ImplOverride {
                declaration,
                member,
            } => {
                let _ = declaration;
                visitor.node(nesting + 1)?;
                let _ = member;
                visitor.node(nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynAssociatedValue {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynAssociatedValue::Type(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynAssociatedValue::Constant { ty, value } => {
                (ty).visit_dyn_metadata(visitor, nesting + 1)?;
                visitor.constant(value, nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynAssociatedResolution {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        (&self.member).visit_dyn_metadata(visitor, nesting + 1)?;
        (&self.source).visit_dyn_metadata(visitor, nesting + 1)?;
        (&self.value).visit_dyn_metadata(visitor, nesting + 1)?;
        Ok(())
    }
}
impl DynRecipeMetadata for DynImplementationWitness {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        (&self.receiver).visit_dyn_metadata(visitor, nesting + 1)?;
        (&self.application).visit_dyn_metadata(visitor, nesting + 1)?;
        let _ = &self.implementation;
        visitor.node(nesting + 1)?;
        visitor.rows((&self.actuals).len(), nesting + 1)?;
        for item in &self.actuals {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        visitor.rows((&self.associated).len(), nesting + 1)?;
        for item in &self.associated {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynFunctionTarget {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynFunctionTarget::Core { module, source } => {
                let _ = module;
                visitor.node(nesting + 1)?;
                let _ = source;
                visitor.node(nesting + 1)?;
            }
            DynFunctionTarget::Typed(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynDefinitionSpecialization {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        (&self.function).visit_dyn_metadata(visitor, nesting + 1)?;
        (&self.receiver).visit_dyn_metadata(visitor, nesting + 1)?;
        visitor.rows((&self.declaration_actuals).len(), nesting + 1)?;
        for item in &self.declaration_actuals {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        visitor.rows((&self.method_actuals).len(), nesting + 1)?;
        for item in &self.method_actuals {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynSelfPattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        let _ = &self.declaration;
        visitor.node(nesting + 1)?;
        visitor.rows((&self.arguments).len(), nesting + 1)?;
        for item in &self.arguments {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynPatternActual {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynPatternActual::Type(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynPatternActual::Const(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynPatternActual::Life(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynConstPattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynConstPattern::Value(value_0) => {
                visitor.constant(value_0, nesting + 1)?;
            }
            DynConstPattern::Parameter(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
            DynConstPattern::MethodParameter(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
            DynConstPattern::Unary { op, value, ty } => {
                let _ = op;
                visitor.node(nesting + 1)?;
                (value).visit_dyn_metadata(visitor, nesting + 1)?;
                (ty).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynConstPattern::Binary {
                op,
                left,
                right,
                ty,
            } => {
                let _ = op;
                visitor.node(nesting + 1)?;
                (left).visit_dyn_metadata(visitor, nesting + 1)?;
                (right).visit_dyn_metadata(visitor, nesting + 1)?;
                (ty).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynConstPattern::Convert { value, ty } => {
                (value).visit_dyn_metadata(visitor, nesting + 1)?;
                (ty).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynConstPattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                (receiver).visit_dyn_metadata(visitor, nesting + 1)?;
                (application).visit_dyn_metadata(visitor, nesting + 1)?;
                let _ = member;
                visitor.node(nesting + 1)?;
            }
            DynConstPattern::IntegerLiteral(value_0) => {
                visitor.text(value_0, nesting + 1)?;
            }
            DynConstPattern::FloatLiteral(value_0) => {
                visitor.text(value_0, nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynTypePattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynTypePattern::Concrete(value_0) => {
                visitor.ty(value_0, nesting + 1)?;
            }
            DynTypePattern::Parameter(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
            DynTypePattern::Nominal(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynTypePattern::Address {
                mutable,
                lifetime,
                inner,
            } => {
                let _ = mutable;
                visitor.node(nesting + 1)?;
                visitor.node(nesting + 1)?;
                if let Some(item) = lifetime {
                    (item).visit_dyn_metadata(visitor, nesting + 1)?;
                }
                (inner).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynTypePattern::Tuple(value_0) => {
                visitor.rows((value_0).len(), nesting + 1)?;
                for item in value_0 {
                    (item).visit_dyn_metadata(visitor, nesting + 1)?;
                }
            }
            DynTypePattern::Array { element, length } => {
                (element).visit_dyn_metadata(visitor, nesting + 1)?;
                (length).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynTypePattern::Slice(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynTypePattern::Managed(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynTypePattern::Desc(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynTypePattern::OwnedStore(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynTypePattern::ErrorUnion { error, result } => {
                (error).visit_dyn_metadata(visitor, nesting + 1)?;
                (result).visit_dyn_metadata(visitor, nesting + 1)?;
            }
            DynTypePattern::SelfType => {}
            DynTypePattern::MethodParameter(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
            DynTypePattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                (receiver).visit_dyn_metadata(visitor, nesting + 1)?;
                (application).visit_dyn_metadata(visitor, nesting + 1)?;
                let _ = member;
                visitor.node(nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for StorageSourceBinding {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        visitor.position(&self.position, nesting + 1)?;
        visitor.rows((&self.sources).len(), nesting + 1)?;
        for item in &self.sources {
            visitor.position(item, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynGenericActual {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynGenericActual::Type(value_0) => {
                visitor.ty(value_0, nesting + 1)?;
            }
            DynGenericActual::Const(value_0) => {
                visitor.constant(value_0, nesting + 1)?;
            }
            DynGenericActual::Life(value_0) => {
                let _ = value_0;
                visitor.node(nesting + 1)?;
            }
            DynGenericActual::QualifiedType(value_0) => {
                (value_0).visit_dyn_metadata(visitor, nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynTraitApplication {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        let _ = &self.declaration;
        visitor.node(nesting + 1)?;
        visitor.rows((&self.actuals).len(), nesting + 1)?;
        for item in &self.actuals {
            (item).visit_dyn_metadata(visitor, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynMember {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        (&self.application).visit_dyn_metadata(visitor, nesting + 1)?;
        let _ = &self.ordinal;
        visitor.node(nesting + 1)?;
        Ok(())
    }
}
impl DynRecipeMetadata for DynCallAccess {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        visitor.rows((&self.reads).len(), nesting + 1)?;
        for item in &self.reads {
            visitor.position(item, nesting + 1)?;
        }
        visitor.rows((&self.writes).len(), nesting + 1)?;
        for item in &self.writes {
            visitor.position(item, nesting + 1)?;
        }
        Ok(())
    }
}
impl DynRecipeMetadata for DynResultSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        match self {
            DynResultSource::Value => {}
            DynResultSource::Create => {}
            DynResultSource::Give => {}
            DynResultSource::Borrow { sources, mutable } => {
                visitor.rows((sources).len(), nesting + 1)?;
                for item in sources {
                    visitor.position(item, nesting + 1)?;
                }
                let _ = mutable;
                visitor.node(nesting + 1)?;
            }
        }

        Ok(())
    }
}
impl DynRecipeMetadata for DynThreadRequirements {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visitor.node(nesting)?;
        let _ = &self.transfer;
        visitor.node(nesting + 1)?;
        let _ = &self.share;
        visitor.node(nesting + 1)?;
        Ok(())
    }
}

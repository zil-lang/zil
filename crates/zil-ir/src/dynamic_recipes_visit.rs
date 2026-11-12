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
enum Work<'a> {
    W0(&'a DynTypeValue, usize),
    W1(&'a Type, usize),
    W2(&'a StorageLifeShape, usize),
    W3(&'a Vec<StorageBinder>, usize),
    W4(&'a StorageBinder, usize),
    W5(&'a DynInstantiationScope, usize),
    W6(&'a u32, usize),
    W7(&'a DynLifeEmbedding, usize),
    W8(&'a DynImplicitLifeKind, usize),
    W9(&'a DynImplicitLifeRoot, usize),
    W10(&'a DynImplicitLifeOccurrence, usize),
    W11(&'a Vec<u32>, usize),
    W12(&'a DynImplicitLifeDeclaration, usize),
    W13(&'a Vec<DynImplicitLifeOccurrence>, usize),
    W14(&'a DynLifePattern, usize),
    W15(&'a DynObligation, usize),
    W16(&'a DynTypePattern, usize),
    W17(&'a DynStructuralBound, usize),
    W18(&'a DynSelfPattern, usize),
    W19(&'a CallableKind, usize),
    W20(&'a DynSignatureRecipe, usize),
    W21(&'a DynConstPattern, usize),
    W22(&'a DynParameterRecipe, usize),
    W23(&'a PassingMode, usize),
    W24(&'a Vec<DynParameterRecipe>, usize),
    W25(&'a CallKind, usize),
    W26(&'a DynContractRecipe, usize),
    W27(&'a DynRequirementRecipe, usize),
    W28(&'a StoragePosition, usize),
    W29(&'a DynGuaranteeRecipe, usize),
    W30(&'a StorageSourceBinding, usize),
    W31(&'a DynEffectRecipe, usize),
    W32(&'a DynCleanupRecipe, usize),
    W33(&'a bool, usize),
    W34(&'a Vec<StoragePosition>, usize),
    W35(&'a DynCallAccess, usize),
    W36(&'a Vec<DynEffectRecipe>, usize),
    W37(&'a Option<Vec<DynRequirementRecipe>>, usize),
    W38(&'a Vec<DynRequirementRecipe>, usize),
    W39(&'a Option<Vec<DynGuaranteeRecipe>>, usize),
    W40(&'a Vec<DynGuaranteeRecipe>, usize),
    W41(&'a Option<Vec<StoragePosition>>, usize),
    W42(&'a Option<DynCallAccess>, usize),
    W43(&'a Option<DynResultSource>, usize),
    W44(&'a DynResultSource, usize),
    W45(&'a Option<DynCleanupRecipe>, usize),
    W46(&'a Option<Vec<DynEffectRecipe>>, usize),
    W47(&'a Option<DynResumeBound>, usize),
    W48(&'a DynResumeBound, usize),
    W49(&'a Option<DynThreadRequirements>, usize),
    W50(&'a DynThreadRequirements, usize),
    W51(&'a DynDefinitionId, usize),
    W52(&'a ModuleId, usize),
    W53(&'a NodeId, usize),
    W54(&'a DynDefinitionScope, usize),
    W55(&'a DynDefinitionDeclaration, usize),
    W56(&'a Vec<DynObligation>, usize),
    W57(&'a Vec<DynDefinitionSpecialization>, usize),
    W58(&'a DynDefinitionSpecialization, usize),
    W59(&'a LifeDeclarationIdentity, usize),
    W60(&'a DynMethodRecipe, usize),
    W61(&'a Option<DynDefinitionId>, usize),
    W62(&'a DynFunctionInstance, usize),
    W63(&'a Vec<DynLifeEmbedding>, usize),
    W64(&'a Vec<DynGenericActual>, usize),
    W65(&'a DynGenericActual, usize),
    W66(&'a DynAssociatedValueRecipe, usize),
    W67(&'a DynAssociatedDefinition, usize),
    W68(&'a Option<DynTypePattern>, usize),
    W69(&'a Option<DynAssociatedValueRecipe>, usize),
    W70(&'a DynAssociatedSource, usize),
    W71(&'a LifeDeclarationId, usize),
    W72(&'a DynAssociatedValue, usize),
    W73(&'a Constant, usize),
    W74(&'a DynAssociatedResolution, usize),
    W75(&'a DynMember, usize),
    W76(&'a DynImplementationWitness, usize),
    W77(&'a DynTraitApplication, usize),
    W78(&'a Vec<DynAssociatedResolution>, usize),
    W79(&'a DynFunctionTarget, usize),
    W80(&'a FunctionId, usize),
    W81(&'a Vec<DynPatternActual>, usize),
    W82(&'a DynPatternActual, usize),
    W83(&'a UnaryOp, usize),
    W84(&'a BinaryOp, usize),
    W85(&'a String, usize),
    W86(&'a Option<DynLifePattern>, usize),
    W87(&'a Vec<DynTypePattern>, usize),
    W88(&'a Vec<StorageSource>, usize),
    W89(&'a StorageSource, usize),
    W90(&'a StoredBorrowMode, usize),
}
fn visit<V: DynRecipeVisitor>(first: Work<'_>, visitor: &mut V) -> Result<(), V::Error> {
    let mut pending = vec![first];
    while let Some(next) = pending.pop() {
        match next {
            Work::W0(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W3(&value.classes, nesting + 1));
                pending.push(Work::W2(&value.life, nesting + 1));
                pending.push(Work::W1(&value.ty, nesting + 1));
            }
            Work::W1(value, nesting) => {
                visitor.ty(value, nesting)?;
            }
            Work::W2(value, nesting) => {
                visitor.life(value, nesting)?;
            }
            Work::W3(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W4(item, nesting));
                }
            }
            Work::W4(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W5(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W6(&value.invocation_binder_count, nesting + 1));
                pending.push(Work::W6(&value.fixed_binder_count, nesting + 1));
            }
            Work::W6(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W7(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W4(&value.to, nesting + 1));
                pending.push(Work::W4(&value.from, nesting + 1));
            }
            Work::W8(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynImplicitLifeKind::Address => {}
                    DynImplicitLifeKind::Description => {}
                    DynImplicitLifeKind::LifetimeArgument => {}
                }
            }
            Work::W9(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynImplicitLifeRoot::SelfPattern => {}
                    DynImplicitLifeRoot::ImplementedTrait => {}
                    DynImplicitLifeRoot::Supertrait(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynImplicitLifeRoot::Requirement(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynImplicitLifeRoot::AssociatedDefinition(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynImplicitLifeRoot::MethodSignature(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynImplicitLifeRoot::MethodDefinition(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W10(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W8(&value.kind, nesting + 1));
                pending.push(Work::W11(&value.path, nesting + 1));
                pending.push(Work::W9(&value.root, nesting + 1));
            }
            Work::W11(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W6(item, nesting));
                }
            }
            Work::W12(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W13(&value.occurrences, nesting + 1));
            }
            Work::W13(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W10(item, nesting));
                }
            }
            Work::W14(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynLifePattern::DeclarationParameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynLifePattern::MethodParameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynLifePattern::Bound {
                        depth: field_depth,
                        index: field_index,
                    } => {
                        pending.push(Work::W6(field_index, nesting + 1));
                        pending.push(Work::W6(field_depth, nesting + 1));
                    }
                    DynLifePattern::Invocation(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynLifePattern::ImplicitDeclaration(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W15(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynObligation::Structural {
                        receiver: field_receiver,
                        bound: field_bound,
                    } => {
                        pending.push(Work::W17(field_bound, nesting + 1));
                        pending.push(Work::W16(field_receiver, nesting + 1));
                    }
                    DynObligation::Trait {
                        receiver: field_receiver,
                        application: field_application,
                    } => {
                        pending.push(Work::W18(field_application, nesting + 1));
                        pending.push(Work::W16(field_receiver, nesting + 1));
                    }
                    DynObligation::Callable {
                        receiver: field_receiver,
                        kind: field_kind,
                        quantified_lifetimes: field_quantified_lifetimes,
                        signature: field_signature,
                    } => {
                        pending.push(Work::W20(field_signature, nesting + 1));
                        pending.push(Work::W6(field_quantified_lifetimes, nesting + 1));
                        pending.push(Work::W19(field_kind, nesting + 1));
                        pending.push(Work::W16(field_receiver, nesting + 1));
                    }
                    DynObligation::TypeEqual {
                        left: field_left,
                        right: field_right,
                    } => {
                        pending.push(Work::W16(field_right, nesting + 1));
                        pending.push(Work::W16(field_left, nesting + 1));
                    }
                    DynObligation::ConstEqual {
                        ty: field_ty,
                        left: field_left,
                        right: field_right,
                    } => {
                        pending.push(Work::W21(field_right, nesting + 1));
                        pending.push(Work::W21(field_left, nesting + 1));
                        pending.push(Work::W16(field_ty, nesting + 1));
                    }
                }
            }
            Work::W16(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynTypePattern::Concrete(field_value_0) => {
                        pending.push(Work::W1(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Parameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Nominal(field_value_0) => {
                        pending.push(Work::W18(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Address {
                        mutable: field_mutable,
                        lifetime: field_lifetime,
                        inner: field_inner,
                    } => {
                        pending.push(Work::W16(field_inner, nesting + 1));
                        pending.push(Work::W86(field_lifetime, nesting + 1));
                        pending.push(Work::W33(field_mutable, nesting + 1));
                    }
                    DynTypePattern::Tuple(field_value_0) => {
                        pending.push(Work::W87(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Array {
                        element: field_element,
                        length: field_length,
                    } => {
                        pending.push(Work::W21(field_length, nesting + 1));
                        pending.push(Work::W16(field_element, nesting + 1));
                    }
                    DynTypePattern::Slice(field_value_0) => {
                        pending.push(Work::W16(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Managed(field_value_0) => {
                        pending.push(Work::W16(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Desc {
                        lifetime: field_lifetime,
                        inner: field_inner,
                    } => {
                        pending.push(Work::W16(field_inner, nesting + 1));
                        pending.push(Work::W86(field_lifetime, nesting + 1));
                    }
                    DynTypePattern::OwnedStore(field_value_0) => {
                        pending.push(Work::W16(field_value_0, nesting + 1));
                    }
                    DynTypePattern::ErrorUnion {
                        error: field_error,
                        result: field_result,
                    } => {
                        pending.push(Work::W16(field_result, nesting + 1));
                        pending.push(Work::W16(field_error, nesting + 1));
                    }
                    DynTypePattern::SelfType => {}
                    DynTypePattern::MethodParameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynTypePattern::AssociatedProjection {
                        receiver: field_receiver,
                        application: field_application,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W18(field_application, nesting + 1));
                        pending.push(Work::W16(field_receiver, nesting + 1));
                    }
                }
            }
            Work::W17(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynStructuralBound::Copy => {}
                    DynStructuralBound::Value => {}
                    DynStructuralBound::SyncDrop => {}
                    DynStructuralBound::StoreElement => {}
                }
            }
            Work::W18(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W81(&value.arguments, nesting + 1));
                pending.push(Work::W71(&value.declaration, nesting + 1));
            }
            Work::W19(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W20(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W26(&value.contract, nesting + 1));
                pending.push(Work::W6(&value.invocation_binder_count, nesting + 1));
                pending.push(Work::W25(&value.call_kind, nesting + 1));
                pending.push(Work::W23(&value.result_mode, nesting + 1));
                pending.push(Work::W16(&value.result, nesting + 1));
                pending.push(Work::W24(&value.parameters, nesting + 1));
            }
            Work::W21(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynConstPattern::Value(field_value_0) => {
                        pending.push(Work::W73(field_value_0, nesting + 1));
                    }
                    DynConstPattern::Parameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynConstPattern::MethodParameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynConstPattern::Unary {
                        op: field_op,
                        value: field_value,
                        ty: field_ty,
                    } => {
                        pending.push(Work::W16(field_ty, nesting + 1));
                        pending.push(Work::W21(field_value, nesting + 1));
                        pending.push(Work::W83(field_op, nesting + 1));
                    }
                    DynConstPattern::Binary {
                        op: field_op,
                        left: field_left,
                        right: field_right,
                        ty: field_ty,
                    } => {
                        pending.push(Work::W16(field_ty, nesting + 1));
                        pending.push(Work::W21(field_right, nesting + 1));
                        pending.push(Work::W21(field_left, nesting + 1));
                        pending.push(Work::W84(field_op, nesting + 1));
                    }
                    DynConstPattern::Convert {
                        value: field_value,
                        ty: field_ty,
                    } => {
                        pending.push(Work::W16(field_ty, nesting + 1));
                        pending.push(Work::W21(field_value, nesting + 1));
                    }
                    DynConstPattern::AssociatedProjection {
                        receiver: field_receiver,
                        application: field_application,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W18(field_application, nesting + 1));
                        pending.push(Work::W16(field_receiver, nesting + 1));
                    }
                    DynConstPattern::IntegerLiteral(field_value_0) => {
                        pending.push(Work::W85(field_value_0, nesting + 1));
                    }
                    DynConstPattern::FloatLiteral(field_value_0) => {
                        pending.push(Work::W85(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W22(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W23(&value.mode, nesting + 1));
                pending.push(Work::W16(&value.ty, nesting + 1));
            }
            Work::W23(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W24(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W22(item, nesting));
                }
            }
            Work::W25(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W26(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W49(&value.threads, nesting + 1));
                pending.push(Work::W47(&value.resume, nesting + 1));
                pending.push(Work::W46(&value.effects, nesting + 1));
                pending.push(Work::W45(&value.cleanup, nesting + 1));
                pending.push(Work::W43(&value.result, nesting + 1));
                pending.push(Work::W42(&value.call, nesting + 1));
                pending.push(Work::W41(&value.hold, nesting + 1));
                pending.push(Work::W39(&value.guarantees, nesting + 1));
                pending.push(Work::W37(&value.requires, nesting + 1));
            }
            Work::W27(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynRequirementRecipe::Read(field_value_0) => {
                        pending.push(Work::W28(field_value_0, nesting + 1));
                    }
                    DynRequirementRecipe::Write(field_value_0) => {
                        pending.push(Work::W28(field_value_0, nesting + 1));
                    }
                    DynRequirementRecipe::Manage(field_value_0) => {
                        pending.push(Work::W28(field_value_0, nesting + 1));
                    }
                    DynRequirementRecipe::Apart(field_value_0, field_value_1) => {
                        pending.push(Work::W28(field_value_1, nesting + 1));
                        pending.push(Work::W28(field_value_0, nesting + 1));
                    }
                    DynRequirementRecipe::State {
                        position: field_position,
                        ty: field_ty,
                        variant: field_variant,
                    } => {
                        pending.push(Work::W6(field_variant, nesting + 1));
                        pending.push(Work::W16(field_ty, nesting + 1));
                        pending.push(Work::W28(field_position, nesting + 1));
                    }
                }
            }
            Work::W28(value, nesting) => {
                visitor.position(value, nesting)?;
            }
            Work::W29(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynGuaranteeRecipe::StorageSources(field_value_0) => {
                        pending.push(Work::W30(field_value_0, nesting + 1));
                    }
                    DynGuaranteeRecipe::ManageResult => {}
                    DynGuaranteeRecipe::State {
                        position: field_position,
                        ty: field_ty,
                        variant: field_variant,
                    } => {
                        pending.push(Work::W6(field_variant, nesting + 1));
                        pending.push(Work::W16(field_ty, nesting + 1));
                        pending.push(Work::W28(field_position, nesting + 1));
                    }
                }
            }
            Work::W30(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W88(&value.sources, nesting + 1));
                pending.push(Work::W28(&value.position, nesting + 1));
            }
            Work::W31(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynEffectRecipe::Alloc => {}
                    DynEffectRecipe::Io => {}
                    DynEffectRecipe::Block => {}
                    DynEffectRecipe::Suspend => {}
                    DynEffectRecipe::Spawn => {}
                    DynEffectRecipe::Ffi => {}
                    DynEffectRecipe::Reenter => {}
                    DynEffectRecipe::Panic => {}
                    DynEffectRecipe::Call(field_value_0) => {
                        pending.push(Work::W28(field_value_0, nesting + 1));
                    }
                    DynEffectRecipe::Cleanup(field_value_0) => {
                        pending.push(Work::W16(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W32(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W36(&value.effects, nesting + 1));
                pending.push(Work::W35(&value.access, nesting + 1));
                pending.push(Work::W34(&value.positions, nesting + 1));
                pending.push(Work::W33(&value.environment, nesting + 1));
            }
            Work::W33(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W34(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W28(item, nesting));
                }
            }
            Work::W35(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W34(&value.writes, nesting + 1));
                pending.push(Work::W34(&value.reads, nesting + 1));
            }
            Work::W36(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W31(item, nesting));
                }
            }
            Work::W37(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W38(item, nesting));
                }
            }
            Work::W38(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W27(item, nesting));
                }
            }
            Work::W39(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W40(item, nesting));
                }
            }
            Work::W40(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W29(item, nesting));
                }
            }
            Work::W41(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W34(item, nesting));
                }
            }
            Work::W42(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W35(item, nesting));
                }
            }
            Work::W43(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W44(item, nesting));
                }
            }
            Work::W44(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynResultSource::Value => {}
                    DynResultSource::Create => {}
                    DynResultSource::Give => {}
                    DynResultSource::Borrow {
                        sources: field_sources,
                        mutable: field_mutable,
                    } => {
                        pending.push(Work::W33(field_mutable, nesting + 1));
                        pending.push(Work::W88(field_sources, nesting + 1));
                    }
                }
            }
            Work::W45(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W32(item, nesting));
                }
            }
            Work::W46(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W36(item, nesting));
                }
            }
            Work::W47(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W48(item, nesting));
                }
            }
            Work::W48(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W49(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W50(item, nesting));
                }
            }
            Work::W50(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W33(&value.share, nesting + 1));
                pending.push(Work::W33(&value.transfer, nesting + 1));
            }
            Work::W51(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W53(&value.source, nesting + 1));
                pending.push(Work::W52(&value.module, nesting + 1));
            }
            Work::W52(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W53(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W54(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynDefinitionScope::TraitDefault => {}
                    DynDefinitionScope::ImplOverride => {}
                }
            }
            Work::W55(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W20(&value.signature, nesting + 1));
                pending.push(Work::W54(&value.scope, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W59(&value.identity, nesting + 1));
                pending.push(Work::W51(&value.id, nesting + 1));
                pending.push(Work::W57(&value.instances, nesting + 1));
                pending.push(Work::W56(&value.requirements, nesting + 1));
            }
            Work::W56(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W15(item, nesting));
                }
            }
            Work::W57(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W58(item, nesting));
                }
            }
            Work::W58(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W64(&value.method_actuals, nesting + 1));
                pending.push(Work::W64(&value.declaration_actuals, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
                pending.push(Work::W79(&value.function, nesting + 1));
            }
            Work::W59(value, nesting) => {
                visitor.identity(value, nesting)?;
            }
            Work::W60(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W61(&value.default_definition, nesting + 1));
                pending.push(Work::W20(&value.signature, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W56(&value.requirements, nesting + 1));
            }
            Work::W61(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W51(item, nesting));
                }
            }
            Work::W62(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W64(&value.method_actuals, nesting + 1));
                pending.push(Work::W64(&value.declaration_actuals, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
                pending.push(Work::W51(&value.definition, nesting + 1));
                pending.push(Work::W63(&value.header_embedding, nesting + 1));
                pending.push(Work::W5(&value.scope, nesting + 1));
            }
            Work::W63(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W7(item, nesting));
                }
            }
            Work::W64(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W65(item, nesting));
                }
            }
            Work::W65(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynGenericActual::Type(field_value_0) => {
                        pending.push(Work::W1(field_value_0, nesting + 1));
                    }
                    DynGenericActual::Const(field_value_0) => {
                        pending.push(Work::W73(field_value_0, nesting + 1));
                    }
                    DynGenericActual::Life(field_value_0) => {
                        pending.push(Work::W4(field_value_0, nesting + 1));
                    }
                    DynGenericActual::QualifiedType(field_value_0) => {
                        pending.push(Work::W0(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W66(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynAssociatedValueRecipe::Type(field_value_0) => {
                        pending.push(Work::W16(field_value_0, nesting + 1));
                    }
                    DynAssociatedValueRecipe::Constant {
                        ty: field_ty,
                        value: field_value,
                    } => {
                        pending.push(Work::W21(field_value, nesting + 1));
                        pending.push(Work::W16(field_ty, nesting + 1));
                    }
                }
            }
            Work::W67(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W56(&value.requirements, nesting + 1));
                pending.push(Work::W69(&value.value, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W68(&value.carrier, nesting + 1));
            }
            Work::W68(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W16(item, nesting));
                }
            }
            Work::W69(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W66(item, nesting));
                }
            }
            Work::W70(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynAssociatedSource::TraitDefault {
                        declaration: field_declaration,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W71(field_declaration, nesting + 1));
                    }
                    DynAssociatedSource::ImplOverride {
                        declaration: field_declaration,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W71(field_declaration, nesting + 1));
                    }
                }
            }
            Work::W71(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W72(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynAssociatedValue::Type(field_value_0) => {
                        pending.push(Work::W0(field_value_0, nesting + 1));
                    }
                    DynAssociatedValue::Constant {
                        ty: field_ty,
                        value: field_value,
                    } => {
                        pending.push(Work::W73(field_value, nesting + 1));
                        pending.push(Work::W0(field_ty, nesting + 1));
                    }
                }
            }
            Work::W73(value, nesting) => {
                visitor.constant(value, nesting)?;
            }
            Work::W74(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W72(&value.value, nesting + 1));
                pending.push(Work::W70(&value.source, nesting + 1));
                pending.push(Work::W75(&value.member, nesting + 1));
            }
            Work::W75(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W6(&value.ordinal, nesting + 1));
                pending.push(Work::W77(&value.application, nesting + 1));
            }
            Work::W76(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W78(&value.associated, nesting + 1));
                pending.push(Work::W64(&value.actuals, nesting + 1));
                pending.push(Work::W71(&value.implementation, nesting + 1));
                pending.push(Work::W77(&value.application, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
                pending.push(Work::W63(&value.to_context, nesting + 1));
                pending.push(Work::W5(&value.scope, nesting + 1));
            }
            Work::W77(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W64(&value.actuals, nesting + 1));
                pending.push(Work::W71(&value.declaration, nesting + 1));
            }
            Work::W78(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W74(item, nesting));
                }
            }
            Work::W79(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynFunctionTarget::Core {
                        module: field_module,
                        source: field_source,
                    } => {
                        pending.push(Work::W53(field_source, nesting + 1));
                        pending.push(Work::W52(field_module, nesting + 1));
                    }
                    DynFunctionTarget::Typed(field_value_0) => {
                        pending.push(Work::W80(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W80(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W81(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W82(item, nesting));
                }
            }
            Work::W82(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynPatternActual::Type(field_value_0) => {
                        pending.push(Work::W16(field_value_0, nesting + 1));
                    }
                    DynPatternActual::Const(field_value_0) => {
                        pending.push(Work::W21(field_value_0, nesting + 1));
                    }
                    DynPatternActual::Life(field_value_0) => {
                        pending.push(Work::W14(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W83(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W84(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W85(value, nesting) => {
                visitor.text(value, nesting)?;
            }
            Work::W86(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W14(item, nesting));
                }
            }
            Work::W87(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W16(item, nesting));
                }
            }
            Work::W88(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W89(item, nesting));
                }
            }
            Work::W89(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    StorageSource::Exact(field_value_0) => {
                        pending.push(Work::W28(field_value_0, nesting + 1));
                    }
                    StorageSource::PlaceAddress {
                        place: field_place,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W90(field_mode, nesting + 1));
                        pending.push(Work::W28(field_place, nesting + 1));
                    }
                    StorageSource::OwnedView {
                        head: field_head,
                        leaf: field_leaf,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W90(field_mode, nesting + 1));
                        pending.push(Work::W1(field_leaf, nesting + 1));
                        pending.push(Work::W28(field_head, nesting + 1));
                    }
                    StorageSource::ReborrowedLeaf {
                        source: field_source,
                        head: field_head,
                    } => {
                        pending.push(Work::W28(field_head, nesting + 1));
                        pending.push(Work::W28(field_source, nesting + 1));
                    }
                    StorageSource::Static {
                        leaf: field_leaf,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W90(field_mode, nesting + 1));
                        pending.push(Work::W1(field_leaf, nesting + 1));
                    }
                }
            }
            Work::W90(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
        }
    }
    Ok(())
}
impl DynRecipeMetadata for DynTypeValue {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W0(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynInstantiationScope {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W5(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynLifeEmbedding {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W7(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynImplicitLifeKind {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W8(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynImplicitLifeRoot {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W9(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynImplicitLifeOccurrence {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W10(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynImplicitLifeDeclaration {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W12(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynLifePattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W14(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynObligation {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W15(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynParameterRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W22(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynSignatureRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W20(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynRequirementRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W27(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynGuaranteeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W29(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynEffectRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W31(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynCleanupRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W32(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynContractRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W26(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionId {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W51(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionScope {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W54(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionDeclaration {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W55(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynMethodRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W60(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynFunctionInstance {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W62(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedValueRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W66(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedDefinition {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W67(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W70(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedValue {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W72(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedResolution {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W74(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynImplementationWitness {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W76(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynFunctionTarget {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W79(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionSpecialization {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W58(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynStructuralBound {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W17(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynSelfPattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W18(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynPatternActual {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W82(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynConstPattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W21(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynTypePattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W16(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for StorageSourceBinding {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W30(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for StorageSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W89(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynGenericActual {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W65(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynTraitApplication {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W77(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynMember {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W75(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynCallAccess {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W35(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynResultSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W44(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynThreadRequirements {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W50(self, nesting), visitor)
    }
}

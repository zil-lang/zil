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
    W5(&'a DynLifePattern, usize),
    W6(&'a u32, usize),
    W7(&'a DynObligation, usize),
    W8(&'a DynTypePattern, usize),
    W9(&'a DynStructuralBound, usize),
    W10(&'a DynSelfPattern, usize),
    W11(&'a CallableKind, usize),
    W12(&'a DynSignatureRecipe, usize),
    W13(&'a DynConstPattern, usize),
    W14(&'a DynParameterRecipe, usize),
    W15(&'a PassingMode, usize),
    W16(&'a Vec<DynParameterRecipe>, usize),
    W17(&'a CallKind, usize),
    W18(&'a DynContractRecipe, usize),
    W19(&'a DynRequirementRecipe, usize),
    W20(&'a StoragePosition, usize),
    W21(&'a DynGuaranteeRecipe, usize),
    W22(&'a StorageSourceBinding, usize),
    W23(&'a DynEffectRecipe, usize),
    W24(&'a DynCleanupRecipe, usize),
    W25(&'a bool, usize),
    W26(&'a Vec<StoragePosition>, usize),
    W27(&'a DynCallAccess, usize),
    W28(&'a Vec<DynEffectRecipe>, usize),
    W29(&'a Option<Vec<DynRequirementRecipe>>, usize),
    W30(&'a Vec<DynRequirementRecipe>, usize),
    W31(&'a Option<Vec<DynGuaranteeRecipe>>, usize),
    W32(&'a Vec<DynGuaranteeRecipe>, usize),
    W33(&'a Option<Vec<StoragePosition>>, usize),
    W34(&'a Option<DynCallAccess>, usize),
    W35(&'a Option<DynResultSource>, usize),
    W36(&'a DynResultSource, usize),
    W37(&'a Option<DynCleanupRecipe>, usize),
    W38(&'a Option<Vec<DynEffectRecipe>>, usize),
    W39(&'a Option<DynResumeBound>, usize),
    W40(&'a DynResumeBound, usize),
    W41(&'a Option<DynThreadRequirements>, usize),
    W42(&'a DynThreadRequirements, usize),
    W43(&'a DynDefinitionId, usize),
    W44(&'a ModuleId, usize),
    W45(&'a NodeId, usize),
    W46(&'a DynDefinitionScope, usize),
    W47(&'a DynDefinitionDeclaration, usize),
    W48(&'a Vec<DynObligation>, usize),
    W49(&'a Vec<DynDefinitionSpecialization>, usize),
    W50(&'a DynDefinitionSpecialization, usize),
    W51(&'a LifeDeclarationIdentity, usize),
    W52(&'a DynMethodRecipe, usize),
    W53(&'a Option<DynDefinitionId>, usize),
    W54(&'a DynFunctionInstance, usize),
    W55(&'a Vec<DynGenericActual>, usize),
    W56(&'a DynGenericActual, usize),
    W57(&'a DynAssociatedValueRecipe, usize),
    W58(&'a DynAssociatedDefinition, usize),
    W59(&'a Option<DynTypePattern>, usize),
    W60(&'a Option<DynAssociatedValueRecipe>, usize),
    W61(&'a DynAssociatedSource, usize),
    W62(&'a LifeDeclarationId, usize),
    W63(&'a DynAssociatedValue, usize),
    W64(&'a Constant, usize),
    W65(&'a DynAssociatedResolution, usize),
    W66(&'a DynMember, usize),
    W67(&'a DynImplementationWitness, usize),
    W68(&'a DynTraitApplication, usize),
    W69(&'a Vec<DynAssociatedResolution>, usize),
    W70(&'a DynFunctionTarget, usize),
    W71(&'a FunctionId, usize),
    W72(&'a Vec<DynPatternActual>, usize),
    W73(&'a DynPatternActual, usize),
    W74(&'a UnaryOp, usize),
    W75(&'a BinaryOp, usize),
    W76(&'a String, usize),
    W77(&'a Option<DynLifePattern>, usize),
    W78(&'a Vec<DynTypePattern>, usize),
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
                }
            }
            Work::W6(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W7(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynObligation::Structural {
                        receiver: field_receiver,
                        bound: field_bound,
                    } => {
                        pending.push(Work::W9(field_bound, nesting + 1));
                        pending.push(Work::W8(field_receiver, nesting + 1));
                    }
                    DynObligation::Trait {
                        receiver: field_receiver,
                        application: field_application,
                    } => {
                        pending.push(Work::W10(field_application, nesting + 1));
                        pending.push(Work::W8(field_receiver, nesting + 1));
                    }
                    DynObligation::Callable {
                        receiver: field_receiver,
                        kind: field_kind,
                        quantified_lifetimes: field_quantified_lifetimes,
                        signature: field_signature,
                    } => {
                        pending.push(Work::W12(field_signature, nesting + 1));
                        pending.push(Work::W6(field_quantified_lifetimes, nesting + 1));
                        pending.push(Work::W11(field_kind, nesting + 1));
                        pending.push(Work::W8(field_receiver, nesting + 1));
                    }
                    DynObligation::TypeEqual {
                        left: field_left,
                        right: field_right,
                    } => {
                        pending.push(Work::W8(field_right, nesting + 1));
                        pending.push(Work::W8(field_left, nesting + 1));
                    }
                    DynObligation::ConstEqual {
                        ty: field_ty,
                        left: field_left,
                        right: field_right,
                    } => {
                        pending.push(Work::W13(field_right, nesting + 1));
                        pending.push(Work::W13(field_left, nesting + 1));
                        pending.push(Work::W8(field_ty, nesting + 1));
                    }
                }
            }
            Work::W8(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynTypePattern::Concrete(field_value_0) => {
                        pending.push(Work::W1(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Parameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Nominal(field_value_0) => {
                        pending.push(Work::W10(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Address {
                        mutable: field_mutable,
                        lifetime: field_lifetime,
                        inner: field_inner,
                    } => {
                        pending.push(Work::W8(field_inner, nesting + 1));
                        pending.push(Work::W77(field_lifetime, nesting + 1));
                        pending.push(Work::W25(field_mutable, nesting + 1));
                    }
                    DynTypePattern::Tuple(field_value_0) => {
                        pending.push(Work::W78(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Array {
                        element: field_element,
                        length: field_length,
                    } => {
                        pending.push(Work::W13(field_length, nesting + 1));
                        pending.push(Work::W8(field_element, nesting + 1));
                    }
                    DynTypePattern::Slice(field_value_0) => {
                        pending.push(Work::W8(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Managed(field_value_0) => {
                        pending.push(Work::W8(field_value_0, nesting + 1));
                    }
                    DynTypePattern::Desc(field_value_0) => {
                        pending.push(Work::W8(field_value_0, nesting + 1));
                    }
                    DynTypePattern::OwnedStore(field_value_0) => {
                        pending.push(Work::W8(field_value_0, nesting + 1));
                    }
                    DynTypePattern::ErrorUnion {
                        error: field_error,
                        result: field_result,
                    } => {
                        pending.push(Work::W8(field_result, nesting + 1));
                        pending.push(Work::W8(field_error, nesting + 1));
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
                        pending.push(Work::W10(field_application, nesting + 1));
                        pending.push(Work::W8(field_receiver, nesting + 1));
                    }
                }
            }
            Work::W9(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynStructuralBound::Copy => {}
                    DynStructuralBound::Value => {}
                    DynStructuralBound::SyncDrop => {}
                    DynStructuralBound::StoreElement => {}
                }
            }
            Work::W10(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W72(&value.arguments, nesting + 1));
                pending.push(Work::W62(&value.declaration, nesting + 1));
            }
            Work::W11(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W12(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W18(&value.contract, nesting + 1));
                pending.push(Work::W6(&value.invocation_binder_count, nesting + 1));
                pending.push(Work::W17(&value.call_kind, nesting + 1));
                pending.push(Work::W15(&value.result_mode, nesting + 1));
                pending.push(Work::W8(&value.result, nesting + 1));
                pending.push(Work::W16(&value.parameters, nesting + 1));
            }
            Work::W13(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynConstPattern::Value(field_value_0) => {
                        pending.push(Work::W64(field_value_0, nesting + 1));
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
                        pending.push(Work::W8(field_ty, nesting + 1));
                        pending.push(Work::W13(field_value, nesting + 1));
                        pending.push(Work::W74(field_op, nesting + 1));
                    }
                    DynConstPattern::Binary {
                        op: field_op,
                        left: field_left,
                        right: field_right,
                        ty: field_ty,
                    } => {
                        pending.push(Work::W8(field_ty, nesting + 1));
                        pending.push(Work::W13(field_right, nesting + 1));
                        pending.push(Work::W13(field_left, nesting + 1));
                        pending.push(Work::W75(field_op, nesting + 1));
                    }
                    DynConstPattern::Convert {
                        value: field_value,
                        ty: field_ty,
                    } => {
                        pending.push(Work::W8(field_ty, nesting + 1));
                        pending.push(Work::W13(field_value, nesting + 1));
                    }
                    DynConstPattern::AssociatedProjection {
                        receiver: field_receiver,
                        application: field_application,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W10(field_application, nesting + 1));
                        pending.push(Work::W8(field_receiver, nesting + 1));
                    }
                    DynConstPattern::IntegerLiteral(field_value_0) => {
                        pending.push(Work::W76(field_value_0, nesting + 1));
                    }
                    DynConstPattern::FloatLiteral(field_value_0) => {
                        pending.push(Work::W76(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W14(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W15(&value.mode, nesting + 1));
                pending.push(Work::W8(&value.ty, nesting + 1));
            }
            Work::W15(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W16(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W14(item, nesting));
                }
            }
            Work::W17(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W18(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W41(&value.threads, nesting + 1));
                pending.push(Work::W39(&value.resume, nesting + 1));
                pending.push(Work::W38(&value.effects, nesting + 1));
                pending.push(Work::W37(&value.cleanup, nesting + 1));
                pending.push(Work::W35(&value.result, nesting + 1));
                pending.push(Work::W34(&value.call, nesting + 1));
                pending.push(Work::W33(&value.hold, nesting + 1));
                pending.push(Work::W31(&value.guarantees, nesting + 1));
                pending.push(Work::W29(&value.requires, nesting + 1));
            }
            Work::W19(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynRequirementRecipe::Read(field_value_0) => {
                        pending.push(Work::W20(field_value_0, nesting + 1));
                    }
                    DynRequirementRecipe::Write(field_value_0) => {
                        pending.push(Work::W20(field_value_0, nesting + 1));
                    }
                    DynRequirementRecipe::Manage(field_value_0) => {
                        pending.push(Work::W20(field_value_0, nesting + 1));
                    }
                    DynRequirementRecipe::Apart(field_value_0, field_value_1) => {
                        pending.push(Work::W20(field_value_1, nesting + 1));
                        pending.push(Work::W20(field_value_0, nesting + 1));
                    }
                    DynRequirementRecipe::State {
                        position: field_position,
                        ty: field_ty,
                        variant: field_variant,
                    } => {
                        pending.push(Work::W6(field_variant, nesting + 1));
                        pending.push(Work::W8(field_ty, nesting + 1));
                        pending.push(Work::W20(field_position, nesting + 1));
                    }
                }
            }
            Work::W20(value, nesting) => {
                visitor.position(value, nesting)?;
            }
            Work::W21(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynGuaranteeRecipe::StorageSources(field_value_0) => {
                        pending.push(Work::W22(field_value_0, nesting + 1));
                    }
                    DynGuaranteeRecipe::ManageResult => {}
                    DynGuaranteeRecipe::State {
                        position: field_position,
                        ty: field_ty,
                        variant: field_variant,
                    } => {
                        pending.push(Work::W6(field_variant, nesting + 1));
                        pending.push(Work::W8(field_ty, nesting + 1));
                        pending.push(Work::W20(field_position, nesting + 1));
                    }
                }
            }
            Work::W22(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W26(&value.sources, nesting + 1));
                pending.push(Work::W20(&value.position, nesting + 1));
            }
            Work::W23(value, nesting) => {
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
                        pending.push(Work::W20(field_value_0, nesting + 1));
                    }
                    DynEffectRecipe::Cleanup(field_value_0) => {
                        pending.push(Work::W8(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W24(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W28(&value.effects, nesting + 1));
                pending.push(Work::W27(&value.access, nesting + 1));
                pending.push(Work::W26(&value.positions, nesting + 1));
                pending.push(Work::W25(&value.environment, nesting + 1));
            }
            Work::W25(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W26(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W20(item, nesting));
                }
            }
            Work::W27(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W26(&value.writes, nesting + 1));
                pending.push(Work::W26(&value.reads, nesting + 1));
            }
            Work::W28(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W23(item, nesting));
                }
            }
            Work::W29(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W30(item, nesting));
                }
            }
            Work::W30(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W19(item, nesting));
                }
            }
            Work::W31(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W32(item, nesting));
                }
            }
            Work::W32(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W21(item, nesting));
                }
            }
            Work::W33(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W26(item, nesting));
                }
            }
            Work::W34(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W27(item, nesting));
                }
            }
            Work::W35(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W36(item, nesting));
                }
            }
            Work::W36(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynResultSource::Value => {}
                    DynResultSource::Create => {}
                    DynResultSource::Give => {}
                    DynResultSource::Borrow {
                        sources: field_sources,
                        mutable: field_mutable,
                    } => {
                        pending.push(Work::W25(field_mutable, nesting + 1));
                        pending.push(Work::W26(field_sources, nesting + 1));
                    }
                }
            }
            Work::W37(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W24(item, nesting));
                }
            }
            Work::W38(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W28(item, nesting));
                }
            }
            Work::W39(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W40(item, nesting));
                }
            }
            Work::W40(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W41(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W42(item, nesting));
                }
            }
            Work::W42(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W25(&value.share, nesting + 1));
                pending.push(Work::W25(&value.transfer, nesting + 1));
            }
            Work::W43(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W45(&value.source, nesting + 1));
                pending.push(Work::W44(&value.module, nesting + 1));
            }
            Work::W44(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W45(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W46(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynDefinitionScope::TraitDefault => {}
                    DynDefinitionScope::ImplOverride => {}
                }
            }
            Work::W47(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W12(&value.signature, nesting + 1));
                pending.push(Work::W46(&value.scope, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W51(&value.identity, nesting + 1));
                pending.push(Work::W43(&value.id, nesting + 1));
                pending.push(Work::W49(&value.instances, nesting + 1));
                pending.push(Work::W48(&value.requirements, nesting + 1));
            }
            Work::W48(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W7(item, nesting));
                }
            }
            Work::W49(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W50(item, nesting));
                }
            }
            Work::W50(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W55(&value.method_actuals, nesting + 1));
                pending.push(Work::W55(&value.declaration_actuals, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
                pending.push(Work::W70(&value.function, nesting + 1));
            }
            Work::W51(value, nesting) => {
                visitor.identity(value, nesting)?;
            }
            Work::W52(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W53(&value.default_definition, nesting + 1));
                pending.push(Work::W12(&value.signature, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W48(&value.requirements, nesting + 1));
            }
            Work::W53(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W43(item, nesting));
                }
            }
            Work::W54(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W55(&value.method_actuals, nesting + 1));
                pending.push(Work::W55(&value.declaration_actuals, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
                pending.push(Work::W43(&value.definition, nesting + 1));
            }
            Work::W55(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W56(item, nesting));
                }
            }
            Work::W56(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynGenericActual::Type(field_value_0) => {
                        pending.push(Work::W1(field_value_0, nesting + 1));
                    }
                    DynGenericActual::Const(field_value_0) => {
                        pending.push(Work::W64(field_value_0, nesting + 1));
                    }
                    DynGenericActual::Life(field_value_0) => {
                        pending.push(Work::W4(field_value_0, nesting + 1));
                    }
                    DynGenericActual::QualifiedType(field_value_0) => {
                        pending.push(Work::W0(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W57(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynAssociatedValueRecipe::Type(field_value_0) => {
                        pending.push(Work::W8(field_value_0, nesting + 1));
                    }
                    DynAssociatedValueRecipe::Constant {
                        ty: field_ty,
                        value: field_value,
                    } => {
                        pending.push(Work::W13(field_value, nesting + 1));
                        pending.push(Work::W8(field_ty, nesting + 1));
                    }
                }
            }
            Work::W58(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W48(&value.requirements, nesting + 1));
                pending.push(Work::W60(&value.value, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W59(&value.carrier, nesting + 1));
            }
            Work::W59(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W8(item, nesting));
                }
            }
            Work::W60(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W57(item, nesting));
                }
            }
            Work::W61(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynAssociatedSource::TraitDefault {
                        declaration: field_declaration,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W62(field_declaration, nesting + 1));
                    }
                    DynAssociatedSource::ImplOverride {
                        declaration: field_declaration,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W62(field_declaration, nesting + 1));
                    }
                }
            }
            Work::W62(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W63(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynAssociatedValue::Type(field_value_0) => {
                        pending.push(Work::W0(field_value_0, nesting + 1));
                    }
                    DynAssociatedValue::Constant {
                        ty: field_ty,
                        value: field_value,
                    } => {
                        pending.push(Work::W64(field_value, nesting + 1));
                        pending.push(Work::W0(field_ty, nesting + 1));
                    }
                }
            }
            Work::W64(value, nesting) => {
                visitor.constant(value, nesting)?;
            }
            Work::W65(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W63(&value.value, nesting + 1));
                pending.push(Work::W61(&value.source, nesting + 1));
                pending.push(Work::W66(&value.member, nesting + 1));
            }
            Work::W66(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W6(&value.ordinal, nesting + 1));
                pending.push(Work::W68(&value.application, nesting + 1));
            }
            Work::W67(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W69(&value.associated, nesting + 1));
                pending.push(Work::W55(&value.actuals, nesting + 1));
                pending.push(Work::W62(&value.implementation, nesting + 1));
                pending.push(Work::W68(&value.application, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
            }
            Work::W68(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W55(&value.actuals, nesting + 1));
                pending.push(Work::W62(&value.declaration, nesting + 1));
            }
            Work::W69(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W65(item, nesting));
                }
            }
            Work::W70(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynFunctionTarget::Core {
                        module: field_module,
                        source: field_source,
                    } => {
                        pending.push(Work::W45(field_source, nesting + 1));
                        pending.push(Work::W44(field_module, nesting + 1));
                    }
                    DynFunctionTarget::Typed(field_value_0) => {
                        pending.push(Work::W71(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W71(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W72(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W73(item, nesting));
                }
            }
            Work::W73(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynPatternActual::Type(field_value_0) => {
                        pending.push(Work::W8(field_value_0, nesting + 1));
                    }
                    DynPatternActual::Const(field_value_0) => {
                        pending.push(Work::W13(field_value_0, nesting + 1));
                    }
                    DynPatternActual::Life(field_value_0) => {
                        pending.push(Work::W5(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W74(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W75(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W76(value, nesting) => {
                visitor.text(value, nesting)?;
            }
            Work::W77(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W5(item, nesting));
                }
            }
            Work::W78(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W8(item, nesting));
                }
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
impl DynRecipeMetadata for DynLifePattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W5(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynObligation {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W7(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynParameterRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W14(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynSignatureRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W12(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynRequirementRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W19(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynGuaranteeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W21(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynEffectRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W23(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynCleanupRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W24(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynContractRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W18(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionId {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W43(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionScope {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W46(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionDeclaration {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W47(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynMethodRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W52(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynFunctionInstance {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W54(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedValueRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W57(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedDefinition {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W58(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W61(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedValue {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W63(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedResolution {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W65(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynImplementationWitness {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W67(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynFunctionTarget {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W70(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionSpecialization {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W50(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynStructuralBound {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W9(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynSelfPattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W10(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynPatternActual {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W73(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynConstPattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W13(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynTypePattern {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W8(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for StorageSourceBinding {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W22(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynGenericActual {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W56(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynTraitApplication {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W68(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynMember {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W66(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynCallAccess {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W27(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynResultSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W36(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynThreadRequirements {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W42(self, nesting), visitor)
    }
}

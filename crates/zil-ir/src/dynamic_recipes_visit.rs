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
    W30(&'a DynStorageSourceBindingRecipe, usize),
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
    W43(&'a Option<DynResultSourceRecipe>, usize),
    W44(&'a DynResultSourceRecipe, usize),
    W45(&'a Option<DynCleanupRecipe>, usize),
    W46(&'a Option<Vec<DynEffectRecipe>>, usize),
    W47(&'a Option<DynResumeBound>, usize),
    W48(&'a DynResumeBound, usize),
    W49(&'a Option<DynThreadRequirements>, usize),
    W50(&'a DynThreadRequirements, usize),
    W51(&'a DynStorageSourceRecipe, usize),
    W52(&'a StoredBorrowMode, usize),
    W53(&'a Vec<DynStorageSourceRecipe>, usize),
    W54(&'a DynDefinitionId, usize),
    W55(&'a ModuleId, usize),
    W56(&'a NodeId, usize),
    W57(&'a DynDefinitionScope, usize),
    W58(&'a DynDefinitionDeclaration, usize),
    W59(&'a Vec<DynObligation>, usize),
    W60(&'a Vec<DynDefinitionSpecialization>, usize),
    W61(&'a DynDefinitionSpecialization, usize),
    W62(&'a LifeDeclarationIdentity, usize),
    W63(&'a DynMethodRecipe, usize),
    W64(&'a Option<DynDefinitionId>, usize),
    W65(&'a DynFunctionInstance, usize),
    W66(&'a Vec<DynLifeEmbedding>, usize),
    W67(&'a Vec<DynGenericActual>, usize),
    W68(&'a DynGenericActual, usize),
    W69(&'a DynAssociatedValueRecipe, usize),
    W70(&'a DynAssociatedDefinition, usize),
    W71(&'a Option<DynTypePattern>, usize),
    W72(&'a Option<DynAssociatedValueRecipe>, usize),
    W73(&'a DynAssociatedSource, usize),
    W74(&'a LifeDeclarationId, usize),
    W75(&'a DynAssociatedValue, usize),
    W76(&'a Constant, usize),
    W77(&'a DynAssociatedResolution, usize),
    W78(&'a DynMember, usize),
    W79(&'a DynImplementationWitness, usize),
    W80(&'a DynTraitApplication, usize),
    W81(&'a Vec<DynAssociatedResolution>, usize),
    W82(&'a DynFunctionTarget, usize),
    W83(&'a FunctionId, usize),
    W84(&'a SemanticDefinitionId, usize),
    W85(&'a SemanticInstanceId, usize),
    W86(&'a SemanticModule, usize),
    W87(&'a Vec<SemanticDefinition>, usize),
    W88(&'a SemanticDefinition, usize),
    W89(&'a Vec<SemanticInstance>, usize),
    W90(&'a SemanticInstance, usize),
    W91(&'a Vec<SemanticSite>, usize),
    W92(&'a SemanticSite, usize),
    W93(&'a Vec<SemanticDynScopeBinding>, usize),
    W94(&'a SemanticDynScopeBinding, usize),
    W95(&'a SemanticDynOwner, usize),
    W96(&'a Option<SemanticDefinitionId>, usize),
    W97(&'a SemanticDefinitionKind, usize),
    W98(&'a Vec<SemanticFormal>, usize),
    W99(&'a SemanticFormal, usize),
    W100(&'a Vec<SemanticImplicitLifetime>, usize),
    W101(&'a SemanticImplicitLifetime, usize),
    W102(&'a Vec<SemanticRequirementRecipe>, usize),
    W103(&'a SemanticRequirementRecipe, usize),
    W104(&'a Vec<SemanticEnvironmentFormal>, usize),
    W105(&'a SemanticEnvironmentFormal, usize),
    W106(&'a SemanticDefinitionBody, usize),
    W107(&'a SemanticSignatureRecipe, usize),
    W108(&'a Vec<SemanticSiteRecipe>, usize),
    W109(&'a SemanticSiteRecipe, usize),
    W110(&'a LifeNominalKind, usize),
    W111(&'a SemanticNominalBody, usize),
    W112(&'a SemanticScopeRecipe, usize),
    W113(&'a SemanticTypeRecipe, usize),
    W114(&'a SemanticTraitRecipe, usize),
    W115(&'a SemanticBuiltinTrait, usize),
    W116(&'a Vec<SemanticFieldRecipe>, usize),
    W117(&'a SemanticFieldRecipe, usize),
    W118(&'a Vec<SemanticVariantRecipe>, usize),
    W119(&'a SemanticVariantRecipe, usize),
    W120(&'a String, usize),
    W121(&'a Vec<SemanticTypeRecipe>, usize),
    W122(&'a SemanticActualRecipe, usize),
    W123(&'a SemanticInstanceTarget, usize),
    W124(&'a SemanticLifetimeInstantiation, usize),
    W125(&'a Option<SemanticInstanceId>, usize),
    W126(&'a Vec<SemanticActual>, usize),
    W127(&'a SemanticActual, usize),
    W128(&'a Vec<SemanticHeaderBinding>, usize),
    W129(&'a SemanticHeaderBinding, usize),
    W130(&'a Option<SemanticEnvironmentInstance>, usize),
    W131(&'a SemanticEnvironmentInstance, usize),
    W132(&'a Option<SemanticTypeValue>, usize),
    W133(&'a SemanticTypeValue, usize),
    W134(&'a SemanticFunctionTarget, usize),
    W135(&'a SemanticClosureTarget, usize),
    W136(&'a ClosureId, usize),
    W137(&'a SemanticClassExpr, usize),
    W138(&'a SemanticCoreTypeSource, usize),
    W139(&'a Vec<SemanticClassExpr>, usize),
    W140(&'a SemanticScopeRef, usize),
    W141(&'a SemanticOperand, usize),
    W142(&'a Vec<LifeTypeProjection>, usize),
    W143(&'a LifeTypeProjection, usize),
    W144(&'a ValueId, usize),
    W145(&'a LocalId, usize),
    W146(&'a SemanticSiteId, usize),
    W147(&'a SemanticSiteRecipeKind, usize),
    W148(&'a Vec<SemanticActualRecipe>, usize),
    W149(&'a SemanticSiteAddress, usize),
    W150(&'a SemanticSiteAction, usize),
    W151(&'a SemanticEmissionRole, usize),
    W152(&'a BlockId, usize),
    W153(&'a SemanticSiteEdge, usize),
    W154(&'a SemanticLocalTarget, usize),
    W155(&'a SemanticRecipeOperand, usize),
    W156(&'a SemanticFormalRef, usize),
    W157(&'a SemanticLifeRecipe, usize),
    W158(&'a SemanticOuterLifeRecipe, usize),
    W159(&'a SemanticExactTypeRecipe, usize),
    W160(&'a SemanticConstRecipe, usize),
    W161(&'a SemanticRetainedShapeRecipe, usize),
    W162(&'a SemanticStorageShapeRecipe, usize),
    W163(&'a SemanticCodeKind, usize),
    W164(&'a SemanticDynInterfaceRecipe, usize),
    W165(&'a Vec<SemanticLifeRecipe>, usize),
    W166(&'a SemanticClassSequenceRecipe, usize),
    W167(&'a Vec<SemanticClassSequencePart>, usize),
    W168(&'a SemanticClassSequencePart, usize),
    W169(&'a Vec<SemanticSlotRecipe>, usize),
    W170(&'a SemanticSlotRecipe, usize),
    W171(&'a Vec<SemanticRetainedPart>, usize),
    W172(&'a SemanticRetainedPart, usize),
    W173(&'a Vec<SemanticNominalPart>, usize),
    W174(&'a SemanticNominalPart, usize),
    W175(&'a Vec<PlaceProjection>, usize),
    W176(&'a PlaceProjection, usize),
    W177(&'a Vec<SemanticPortPart>, usize),
    W178(&'a SemanticPortPart, usize),
    W179(&'a UnaryOp, usize),
    W180(&'a BinaryOp, usize),
    W181(&'a Vec<SemanticParameterRecipe>, usize),
    W182(&'a SemanticParameterRecipe, usize),
    W183(&'a SemanticDynContextRecipe, usize),
    W184(&'a Vec<SemanticAssociatedBindingRecipe>, usize),
    W185(&'a SemanticAssociatedBindingRecipe, usize),
    W186(&'a SemanticAssociatedRecipe, usize),
    W187(&'a Vec<DynPatternActual>, usize),
    W188(&'a DynPatternActual, usize),
    W189(&'a Option<DynLifePattern>, usize),
    W190(&'a Vec<DynTypePattern>, usize),
    W191(&'a VariantProjection, usize),
    W192(&'a StorageSourceBinding, usize),
    W193(&'a Vec<StorageSource>, usize),
    W194(&'a StorageSource, usize),
    W195(&'a DynResultSource, usize),
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
                        pending.push(Work::W189(field_lifetime, nesting + 1));
                        pending.push(Work::W33(field_mutable, nesting + 1));
                    }
                    DynTypePattern::Tuple(field_value_0) => {
                        pending.push(Work::W190(field_value_0, nesting + 1));
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
                        pending.push(Work::W189(field_lifetime, nesting + 1));
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
                    DynTypePattern::Semantic {
                        scope: field_scope,
                        ty: field_ty,
                    } => {
                        pending.push(Work::W113(field_ty, nesting + 1));
                        pending.push(Work::W84(field_scope, nesting + 1));
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
                pending.push(Work::W187(&value.arguments, nesting + 1));
                pending.push(Work::W74(&value.declaration, nesting + 1));
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
                        pending.push(Work::W76(field_value_0, nesting + 1));
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
                        pending.push(Work::W179(field_op, nesting + 1));
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
                        pending.push(Work::W180(field_op, nesting + 1));
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
                        pending.push(Work::W120(field_value_0, nesting + 1));
                    }
                    DynConstPattern::FloatLiteral(field_value_0) => {
                        pending.push(Work::W120(field_value_0, nesting + 1));
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
                pending.push(Work::W53(&value.sources, nesting + 1));
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
                    DynResultSourceRecipe::Value => {}
                    DynResultSourceRecipe::Create => {}
                    DynResultSourceRecipe::Give => {}
                    DynResultSourceRecipe::Borrow {
                        sources: field_sources,
                        mutable: field_mutable,
                    } => {
                        pending.push(Work::W33(field_mutable, nesting + 1));
                        pending.push(Work::W53(field_sources, nesting + 1));
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
                match value {
                    DynStorageSourceRecipe::Exact(field_value_0) => {
                        pending.push(Work::W28(field_value_0, nesting + 1));
                    }
                    DynStorageSourceRecipe::PlaceAddress {
                        place: field_place,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W52(field_mode, nesting + 1));
                        pending.push(Work::W28(field_place, nesting + 1));
                    }
                    DynStorageSourceRecipe::OwnedView {
                        head: field_head,
                        leaf: field_leaf,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W52(field_mode, nesting + 1));
                        pending.push(Work::W16(field_leaf, nesting + 1));
                        pending.push(Work::W28(field_head, nesting + 1));
                    }
                    DynStorageSourceRecipe::ReborrowedLeaf {
                        source: field_source,
                        head: field_head,
                    } => {
                        pending.push(Work::W28(field_head, nesting + 1));
                        pending.push(Work::W28(field_source, nesting + 1));
                    }
                    DynStorageSourceRecipe::Static {
                        leaf: field_leaf,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W52(field_mode, nesting + 1));
                        pending.push(Work::W16(field_leaf, nesting + 1));
                    }
                }
            }
            Work::W52(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    StoredBorrowMode::SharedAddress => {}
                    StoredBorrowMode::ExclusiveAddress => {}
                    StoredBorrowMode::ReadOnlyDescription => {}
                    StoredBorrowMode::WritableDescription => {}
                }
            }
            Work::W53(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W51(item, nesting));
                }
            }
            Work::W54(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W56(&value.source, nesting + 1));
                pending.push(Work::W55(&value.module, nesting + 1));
            }
            Work::W55(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W56(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W57(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynDefinitionScope::TraitDefault => {}
                    DynDefinitionScope::ImplOverride => {}
                }
            }
            Work::W58(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W20(&value.signature, nesting + 1));
                pending.push(Work::W57(&value.scope, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W62(&value.identity, nesting + 1));
                pending.push(Work::W54(&value.id, nesting + 1));
                pending.push(Work::W60(&value.instances, nesting + 1));
                pending.push(Work::W59(&value.requirements, nesting + 1));
            }
            Work::W59(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W15(item, nesting));
                }
            }
            Work::W60(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W61(item, nesting));
                }
            }
            Work::W61(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W67(&value.method_actuals, nesting + 1));
                pending.push(Work::W67(&value.declaration_actuals, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
                pending.push(Work::W82(&value.function, nesting + 1));
            }
            Work::W62(value, nesting) => {
                visitor.identity(value, nesting)?;
            }
            Work::W63(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W64(&value.default_definition, nesting + 1));
                pending.push(Work::W20(&value.signature, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W59(&value.requirements, nesting + 1));
            }
            Work::W64(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W54(item, nesting));
                }
            }
            Work::W65(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W67(&value.method_actuals, nesting + 1));
                pending.push(Work::W67(&value.declaration_actuals, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
                pending.push(Work::W54(&value.definition, nesting + 1));
                pending.push(Work::W66(&value.header_embedding, nesting + 1));
                pending.push(Work::W5(&value.scope, nesting + 1));
            }
            Work::W66(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W7(item, nesting));
                }
            }
            Work::W67(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W68(item, nesting));
                }
            }
            Work::W68(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynGenericActual::Type(field_value_0) => {
                        pending.push(Work::W1(field_value_0, nesting + 1));
                    }
                    DynGenericActual::Const(field_value_0) => {
                        pending.push(Work::W76(field_value_0, nesting + 1));
                    }
                    DynGenericActual::Life(field_value_0) => {
                        pending.push(Work::W4(field_value_0, nesting + 1));
                    }
                    DynGenericActual::QualifiedType(field_value_0) => {
                        pending.push(Work::W0(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W69(value, nesting) => {
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
            Work::W70(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W59(&value.requirements, nesting + 1));
                pending.push(Work::W72(&value.value, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W71(&value.carrier, nesting + 1));
            }
            Work::W71(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W16(item, nesting));
                }
            }
            Work::W72(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W69(item, nesting));
                }
            }
            Work::W73(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynAssociatedSource::TraitDefault {
                        declaration: field_declaration,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W74(field_declaration, nesting + 1));
                    }
                    DynAssociatedSource::ImplOverride {
                        declaration: field_declaration,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W74(field_declaration, nesting + 1));
                    }
                }
            }
            Work::W74(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W75(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynAssociatedValue::Type(field_value_0) => {
                        pending.push(Work::W0(field_value_0, nesting + 1));
                    }
                    DynAssociatedValue::Constant {
                        ty: field_ty,
                        value: field_value,
                    } => {
                        pending.push(Work::W76(field_value, nesting + 1));
                        pending.push(Work::W0(field_ty, nesting + 1));
                    }
                }
            }
            Work::W76(value, nesting) => {
                visitor.constant(value, nesting)?;
            }
            Work::W77(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W75(&value.value, nesting + 1));
                pending.push(Work::W73(&value.source, nesting + 1));
                pending.push(Work::W78(&value.member, nesting + 1));
            }
            Work::W78(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W6(&value.ordinal, nesting + 1));
                pending.push(Work::W80(&value.application, nesting + 1));
            }
            Work::W79(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W81(&value.associated, nesting + 1));
                pending.push(Work::W67(&value.actuals, nesting + 1));
                pending.push(Work::W74(&value.implementation, nesting + 1));
                pending.push(Work::W80(&value.application, nesting + 1));
                pending.push(Work::W0(&value.receiver, nesting + 1));
                pending.push(Work::W66(&value.to_context, nesting + 1));
                pending.push(Work::W5(&value.scope, nesting + 1));
            }
            Work::W80(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W67(&value.actuals, nesting + 1));
                pending.push(Work::W74(&value.declaration, nesting + 1));
            }
            Work::W81(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W77(item, nesting));
                }
            }
            Work::W82(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    DynFunctionTarget::Core {
                        module: field_module,
                        source: field_source,
                    } => {
                        pending.push(Work::W56(field_source, nesting + 1));
                        pending.push(Work::W55(field_module, nesting + 1));
                    }
                    DynFunctionTarget::Typed(field_value_0) => {
                        pending.push(Work::W83(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W83(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W84(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W6(&value.index, nesting + 1));
                pending.push(Work::W55(&value.module, nesting + 1));
            }
            Work::W85(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W6(&value.index, nesting + 1));
                pending.push(Work::W55(&value.module, nesting + 1));
            }
            Work::W86(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W93(&value.dyn_scopes, nesting + 1));
                pending.push(Work::W91(&value.sites, nesting + 1));
                pending.push(Work::W89(&value.instances, nesting + 1));
                pending.push(Work::W87(&value.definitions, nesting + 1));
            }
            Work::W87(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W88(item, nesting));
                }
            }
            Work::W88(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W106(&value.body, nesting + 1));
                pending.push(Work::W104(&value.environment, nesting + 1));
                pending.push(Work::W102(&value.requirements, nesting + 1));
                pending.push(Work::W100(&value.implicit_lifetimes, nesting + 1));
                pending.push(Work::W98(&value.parameters, nesting + 1));
                pending.push(Work::W97(&value.kind, nesting + 1));
                pending.push(Work::W96(&value.parent, nesting + 1));
                pending.push(Work::W62(&value.identity, nesting + 1));
                pending.push(Work::W84(&value.id, nesting + 1));
            }
            Work::W89(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W90(item, nesting));
                }
            }
            Work::W90(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W132(&value.self_type, nesting + 1));
                pending.push(Work::W130(&value.environment, nesting + 1));
                pending.push(Work::W128(&value.header_embedding, nesting + 1));
                pending.push(Work::W126(&value.actuals, nesting + 1));
                pending.push(Work::W125(&value.parent, nesting + 1));
                pending.push(Work::W124(&value.lifetime_instantiation, nesting + 1));
                pending.push(Work::W123(&value.target, nesting + 1));
                pending.push(Work::W84(&value.definition, nesting + 1));
                pending.push(Work::W85(&value.id, nesting + 1));
            }
            Work::W91(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W92(item, nesting));
                }
            }
            Work::W92(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W150(&value.action, nesting + 1));
                pending.push(Work::W149(&value.at, nesting + 1));
                pending.push(Work::W146(&value.recipe, nesting + 1));
                pending.push(Work::W85(&value.instance, nesting + 1));
            }
            Work::W93(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W94(item, nesting));
                }
            }
            Work::W94(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W84(&value.scope, nesting + 1));
                pending.push(Work::W95(&value.owner, nesting + 1));
            }
            Work::W95(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticDynOwner::Declaration(field_value_0) => {
                        pending.push(Work::W74(field_value_0, nesting + 1));
                    }
                    SemanticDynOwner::MethodSignature {
                        declaration: field_declaration,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W74(field_declaration, nesting + 1));
                    }
                    SemanticDynOwner::MethodDefinition(field_value_0) => {
                        pending.push(Work::W54(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W96(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W84(item, nesting));
                }
            }
            Work::W97(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticDefinitionKind::Function => {}
                    SemanticDefinitionKind::Method => {}
                    SemanticDefinitionKind::Closure => {}
                    SemanticDefinitionKind::TaskEntry => {}
                    SemanticDefinitionKind::Nominal => {}
                    SemanticDefinitionKind::Scope => {}
                }
            }
            Work::W98(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W99(item, nesting));
                }
            }
            Work::W99(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticFormal::Type {
                        requirements: field_requirements,
                    } => {
                        pending.push(Work::W102(field_requirements, nesting + 1));
                    }
                    SemanticFormal::Const {
                        carrier: field_carrier,
                    } => {
                        pending.push(Work::W113(field_carrier, nesting + 1));
                    }
                    SemanticFormal::Lifetime => {}
                }
            }
            Work::W100(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W101(item, nesting));
                }
            }
            Work::W101(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W11(&value.occurrence_path, nesting + 1));
                pending.push(Work::W6(&value.ordinal, nesting + 1));
            }
            Work::W102(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W103(item, nesting));
                }
            }
            Work::W103(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticRequirementRecipe::Structural {
                        ty: field_ty,
                        bound: field_bound,
                    } => {
                        pending.push(Work::W17(field_bound, nesting + 1));
                        pending.push(Work::W113(field_ty, nesting + 1));
                    }
                    SemanticRequirementRecipe::Trait {
                        ty: field_ty,
                        application: field_application,
                    } => {
                        pending.push(Work::W114(field_application, nesting + 1));
                        pending.push(Work::W113(field_ty, nesting + 1));
                    }
                    SemanticRequirementRecipe::Callable {
                        ty: field_ty,
                        kind: field_kind,
                        signature: field_signature,
                    } => {
                        pending.push(Work::W107(field_signature, nesting + 1));
                        pending.push(Work::W19(field_kind, nesting + 1));
                        pending.push(Work::W113(field_ty, nesting + 1));
                    }
                    SemanticRequirementRecipe::TypeEqual {
                        left: field_left,
                        right: field_right,
                    } => {
                        pending.push(Work::W113(field_right, nesting + 1));
                        pending.push(Work::W113(field_left, nesting + 1));
                    }
                    SemanticRequirementRecipe::ConstEqual {
                        carrier: field_carrier,
                        left: field_left,
                        right: field_right,
                    } => {
                        pending.push(Work::W160(field_right, nesting + 1));
                        pending.push(Work::W160(field_left, nesting + 1));
                        pending.push(Work::W113(field_carrier, nesting + 1));
                    }
                    SemanticRequirementRecipe::LifeEqual {
                        left: field_left,
                        right: field_right,
                    } => {
                        pending.push(Work::W157(field_right, nesting + 1));
                        pending.push(Work::W157(field_left, nesting + 1));
                    }
                    SemanticRequirementRecipe::Outlives {
                        longer: field_longer,
                        shorter: field_shorter,
                    } => {
                        pending.push(Work::W157(field_shorter, nesting + 1));
                        pending.push(Work::W157(field_longer, nesting + 1));
                    }
                    SemanticRequirementRecipe::ForAll {
                        lifetimes: field_lifetimes,
                        requirements: field_requirements,
                    } => {
                        pending.push(Work::W102(field_requirements, nesting + 1));
                        pending.push(Work::W6(field_lifetimes, nesting + 1));
                    }
                    SemanticRequirementRecipe::Builtin {
                        ty: field_ty,
                        trait_kind: field_trait_kind,
                    } => {
                        pending.push(Work::W115(field_trait_kind, nesting + 1));
                        pending.push(Work::W113(field_ty, nesting + 1));
                    }
                }
            }
            Work::W104(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W105(item, nesting));
                }
            }
            Work::W105(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W122(&value.source, nesting + 1));
                pending.push(Work::W99(&value.kind, nesting + 1));
            }
            Work::W106(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticDefinitionBody::Function {
                        signature: field_signature,
                        sites: field_sites,
                    } => {
                        pending.push(Work::W108(field_sites, nesting + 1));
                        pending.push(Work::W107(field_signature, nesting + 1));
                    }
                    SemanticDefinitionBody::Nominal {
                        declaration: field_declaration,
                        kind: field_kind,
                        body: field_body,
                    } => {
                        pending.push(Work::W111(field_body, nesting + 1));
                        pending.push(Work::W110(field_kind, nesting + 1));
                        pending.push(Work::W74(field_declaration, nesting + 1));
                    }
                    SemanticDefinitionBody::Scope(field_value_0) => {
                        pending.push(Work::W112(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W107(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W25(&value.call_kind, nesting + 1));
                pending.push(Work::W23(&value.result_mode, nesting + 1));
                pending.push(Work::W113(&value.result, nesting + 1));
                pending.push(Work::W181(&value.parameters, nesting + 1));
                pending.push(Work::W6(&value.invocation_lifetimes, nesting + 1));
                pending.push(Work::W6(&value.bound_lifetimes, nesting + 1));
            }
            Work::W108(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W109(item, nesting));
                }
            }
            Work::W109(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W147(&value.kind, nesting + 1));
                pending.push(Work::W11(&value.occurrence_path, nesting + 1));
                pending.push(Work::W146(&value.id, nesting + 1));
            }
            Work::W110(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    LifeNominalKind::Record => {}
                    LifeNominalKind::Package => {}
                    LifeNominalKind::Resource => {}
                    LifeNominalKind::Enum => {}
                }
            }
            Work::W111(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticNominalBody::Product {
                        fields: field_fields,
                    } => {
                        pending.push(Work::W116(field_fields, nesting + 1));
                    }
                    SemanticNominalBody::Enum {
                        variants: field_variants,
                    } => {
                        pending.push(Work::W118(field_variants, nesting + 1));
                    }
                }
            }
            Work::W112(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticScopeRecipe::Trait {
                        declaration: field_declaration,
                    } => {
                        pending.push(Work::W74(field_declaration, nesting + 1));
                    }
                    SemanticScopeRecipe::InherentImpl {
                        declaration: field_declaration,
                        self_type: field_self_type,
                    } => {
                        pending.push(Work::W113(field_self_type, nesting + 1));
                        pending.push(Work::W74(field_declaration, nesting + 1));
                    }
                    SemanticScopeRecipe::TraitImpl {
                        declaration: field_declaration,
                        self_type: field_self_type,
                        trait_application: field_trait_application,
                    } => {
                        pending.push(Work::W114(field_trait_application, nesting + 1));
                        pending.push(Work::W113(field_self_type, nesting + 1));
                        pending.push(Work::W74(field_declaration, nesting + 1));
                    }
                    SemanticScopeRecipe::BuiltinImpl {
                        self_type: field_self_type,
                        trait_kind: field_trait_kind,
                    } => {
                        pending.push(Work::W115(field_trait_kind, nesting + 1));
                        pending.push(Work::W113(field_self_type, nesting + 1));
                    }
                }
            }
            Work::W113(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticTypeRecipe::Atom(field_value_0) => {
                        pending.push(Work::W1(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::Exact(field_value_0) => {
                        pending.push(Work::W159(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::Formal(field_value_0) => {
                        pending.push(Work::W156(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::Environment {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticTypeRecipe::SelfType { scope: field_scope } => {
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticTypeRecipe::Nominal {
                        definition: field_definition,
                        arguments: field_arguments,
                    } => {
                        pending.push(Work::W148(field_arguments, nesting + 1));
                        pending.push(Work::W84(field_definition, nesting + 1));
                    }
                    SemanticTypeRecipe::AssociatedType {
                        receiver: field_receiver,
                        application: field_application,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W114(field_application, nesting + 1));
                        pending.push(Work::W113(field_receiver, nesting + 1));
                    }
                    SemanticTypeRecipe::Tuple(field_value_0) => {
                        pending.push(Work::W121(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::Array {
                        element: field_element,
                        length: field_length,
                    } => {
                        pending.push(Work::W160(field_length, nesting + 1));
                        pending.push(Work::W113(field_element, nesting + 1));
                    }
                    SemanticTypeRecipe::Slice(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::Address {
                        mutable: field_mutable,
                        lifetime: field_lifetime,
                        inner: field_inner,
                    } => {
                        pending.push(Work::W113(field_inner, nesting + 1));
                        pending.push(Work::W158(field_lifetime, nesting + 1));
                        pending.push(Work::W33(field_mutable, nesting + 1));
                    }
                    SemanticTypeRecipe::Desc {
                        lifetime: field_lifetime,
                        inner: field_inner,
                    } => {
                        pending.push(Work::W113(field_inner, nesting + 1));
                        pending.push(Work::W158(field_lifetime, nesting + 1));
                    }
                    SemanticTypeRecipe::Managed(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::OwnedStore(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::Mutex(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::MutexGuard {
                        payload: field_payload,
                        retained: field_retained,
                    } => {
                        pending.push(Work::W161(field_retained, nesting + 1));
                        pending.push(Work::W113(field_payload, nesting + 1));
                    }
                    SemanticTypeRecipe::Registration(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::AsyncRegistration(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::Task {
                        result: field_result,
                        retained: field_retained,
                    } => {
                        pending.push(Work::W161(field_retained, nesting + 1));
                        pending.push(Work::W113(field_result, nesting + 1));
                    }
                    SemanticTypeRecipe::ErrorUnion {
                        error: field_error,
                        result: field_result,
                    } => {
                        pending.push(Work::W113(field_result, nesting + 1));
                        pending.push(Work::W113(field_error, nesting + 1));
                    }
                    SemanticTypeRecipe::Qualified {
                        storage: field_storage,
                        life: field_life,
                    } => {
                        pending.push(Work::W162(field_life, nesting + 1));
                        pending.push(Work::W113(field_storage, nesting + 1));
                    }
                    SemanticTypeRecipe::Code {
                        kind: field_kind,
                        signature: field_signature,
                    } => {
                        pending.push(Work::W107(field_signature, nesting + 1));
                        pending.push(Work::W163(field_kind, nesting + 1));
                    }
                    SemanticTypeRecipe::Closure {
                        definition: field_definition,
                        arguments: field_arguments,
                        environment: field_environment,
                    } => {
                        pending.push(Work::W148(field_environment, nesting + 1));
                        pending.push(Work::W148(field_arguments, nesting + 1));
                        pending.push(Work::W84(field_definition, nesting + 1));
                    }
                    SemanticTypeRecipe::Dyn {
                        owned: field_owned,
                        interface: field_interface,
                    } => {
                        pending.push(Work::W164(field_interface, nesting + 1));
                        pending.push(Work::W33(field_owned, nesting + 1));
                    }
                    SemanticTypeRecipe::Opaque {
                        definition: field_definition,
                        arguments: field_arguments,
                        hidden: field_hidden,
                    } => {
                        pending.push(Work::W113(field_hidden, nesting + 1));
                        pending.push(Work::W148(field_arguments, nesting + 1));
                        pending.push(Work::W84(field_definition, nesting + 1));
                    }
                    SemanticTypeRecipe::AnonymousSum(field_value_0) => {
                        pending.push(Work::W121(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::UnresolvedNamed(field_value_0) => {
                        pending.push(Work::W120(field_value_0, nesting + 1));
                    }
                    SemanticTypeRecipe::Unsupported(field_value_0) => {
                        pending.push(Work::W120(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W114(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W148(&value.arguments, nesting + 1));
                pending.push(Work::W74(&value.declaration, nesting + 1));
            }
            Work::W115(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticBuiltinTrait::Drop => {}
                    SemanticBuiltinTrait::Copy => {}
                    SemanticBuiltinTrait::Value => {}
                    SemanticBuiltinTrait::SyncDrop => {}
                    SemanticBuiltinTrait::StoreElement => {}
                    SemanticBuiltinTrait::Transfer => {}
                    SemanticBuiltinTrait::Share => {}
                    SemanticBuiltinTrait::ThreadCallable => {}
                    SemanticBuiltinTrait::ClosedEnvironment => {}
                    SemanticBuiltinTrait::Send => {}
                    SemanticBuiltinTrait::Sync => {}
                }
            }
            Work::W116(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W117(item, nesting));
                }
            }
            Work::W117(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W113(&value.ty, nesting + 1));
                pending.push(Work::W120(&value.name, nesting + 1));
            }
            Work::W118(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W119(item, nesting));
                }
            }
            Work::W119(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W121(&value.fields, nesting + 1));
                pending.push(Work::W120(&value.name, nesting + 1));
            }
            Work::W120(value, nesting) => {
                visitor.text(value, nesting)?;
            }
            Work::W121(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W113(item, nesting));
                }
            }
            Work::W122(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticActualRecipe::Type(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticActualRecipe::Const {
                        carrier: field_carrier,
                        value: field_value,
                    } => {
                        pending.push(Work::W160(field_value, nesting + 1));
                        pending.push(Work::W113(field_carrier, nesting + 1));
                    }
                    SemanticActualRecipe::Life(field_value_0) => {
                        pending.push(Work::W157(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W123(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticInstanceTarget::Function(field_value_0) => {
                        pending.push(Work::W134(field_value_0, nesting + 1));
                    }
                    SemanticInstanceTarget::Scope => {}
                }
            }
            Work::W124(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticLifetimeInstantiation::Rigid => {}
                    SemanticLifetimeInstantiation::Partition { rigid: field_rigid } => {
                        pending.push(Work::W85(field_rigid, nesting + 1));
                    }
                }
            }
            Work::W125(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W85(item, nesting));
                }
            }
            Work::W126(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W127(item, nesting));
                }
            }
            Work::W127(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticActual::Type(field_value_0) => {
                        pending.push(Work::W133(field_value_0, nesting + 1));
                    }
                    SemanticActual::Const {
                        ty: field_ty,
                        value: field_value,
                    } => {
                        pending.push(Work::W76(field_value, nesting + 1));
                        pending.push(Work::W133(field_ty, nesting + 1));
                    }
                    SemanticActual::Life(field_value_0) => {
                        pending.push(Work::W137(field_value_0, nesting + 1));
                    }
                    SemanticActual::CoreRecipe {
                        scope: field_scope,
                        recipe: field_recipe,
                    } => {
                        pending.push(Work::W122(field_recipe, nesting + 1));
                        pending.push(Work::W85(field_scope, nesting + 1));
                    }
                }
            }
            Work::W128(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W129(item, nesting));
                }
            }
            Work::W129(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W137(&value.actual, nesting + 1));
                pending.push(Work::W4(&value.header, nesting + 1));
            }
            Work::W130(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W131(item, nesting));
                }
            }
            Work::W131(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W126(&value.actuals, nesting + 1));
                pending.push(Work::W135(&value.closure, nesting + 1));
            }
            Work::W132(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W133(item, nesting));
                }
            }
            Work::W133(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticTypeValue::Resolved {
                        ty: field_ty,
                        life: field_life,
                        classes: field_classes,
                    } => {
                        pending.push(Work::W139(field_classes, nesting + 1));
                        pending.push(Work::W2(field_life, nesting + 1));
                        pending.push(Work::W1(field_ty, nesting + 1));
                    }
                    SemanticTypeValue::Core {
                        scope: field_scope,
                        source: field_source,
                    } => {
                        pending.push(Work::W138(field_source, nesting + 1));
                        pending.push(Work::W85(field_scope, nesting + 1));
                    }
                }
            }
            Work::W134(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticFunctionTarget::Core {
                        module: field_module,
                        function: field_function,
                    } => {
                        pending.push(Work::W56(field_function, nesting + 1));
                        pending.push(Work::W55(field_module, nesting + 1));
                    }
                    SemanticFunctionTarget::CoreClosure {
                        module: field_module,
                        creator: field_creator,
                        expression: field_expression,
                        occurrence: field_occurrence,
                    } => {
                        pending.push(Work::W6(field_occurrence, nesting + 1));
                        pending.push(Work::W56(field_expression, nesting + 1));
                        pending.push(Work::W56(field_creator, nesting + 1));
                        pending.push(Work::W55(field_module, nesting + 1));
                    }
                    SemanticFunctionTarget::Typed(field_value_0) => {
                        pending.push(Work::W83(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W135(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticClosureTarget::Core {
                        module: field_module,
                        creator: field_creator,
                        expression: field_expression,
                        occurrence: field_occurrence,
                    } => {
                        pending.push(Work::W6(field_occurrence, nesting + 1));
                        pending.push(Work::W56(field_expression, nesting + 1));
                        pending.push(Work::W56(field_creator, nesting + 1));
                        pending.push(Work::W55(field_module, nesting + 1));
                    }
                    SemanticClosureTarget::Typed(field_value_0) => {
                        pending.push(Work::W136(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W136(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W137(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticClassExpr::Formal {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticClassExpr::Implicit {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticClassExpr::Bound {
                        depth: field_depth,
                        index: field_index,
                    } => {
                        pending.push(Work::W6(field_index, nesting + 1));
                        pending.push(Work::W6(field_depth, nesting + 1));
                    }
                    SemanticClassExpr::OuterOccurrence {
                        operand: field_operand,
                        path: field_path,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W52(field_mode, nesting + 1));
                        pending.push(Work::W142(field_path, nesting + 1));
                        pending.push(Work::W141(field_operand, nesting + 1));
                    }
                    SemanticClassExpr::IntrinsicClass {
                        operand: field_operand,
                        path: field_path,
                        class: field_class,
                    } => {
                        pending.push(Work::W6(field_class, nesting + 1));
                        pending.push(Work::W142(field_path, nesting + 1));
                        pending.push(Work::W141(field_operand, nesting + 1));
                    }
                    SemanticClassExpr::Environment {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                }
            }
            Work::W138(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticCoreTypeSource::OriginalRecipe(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticCoreTypeSource::Expression {
                        module: field_module,
                        function: field_function,
                        expression: field_expression,
                        occurrence: field_occurrence,
                        recipe: field_recipe,
                    } => {
                        pending.push(Work::W113(field_recipe, nesting + 1));
                        pending.push(Work::W6(field_occurrence, nesting + 1));
                        pending.push(Work::W56(field_expression, nesting + 1));
                        pending.push(Work::W56(field_function, nesting + 1));
                        pending.push(Work::W55(field_module, nesting + 1));
                    }
                }
            }
            Work::W139(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W137(item, nesting));
                }
            }
            Work::W140(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticScopeRef::Ancestor(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W141(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticOperand::Parameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    SemanticOperand::Capture(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    SemanticOperand::Receiver => {}
                    SemanticOperand::Value(field_value_0) => {
                        pending.push(Work::W144(field_value_0, nesting + 1));
                    }
                    SemanticOperand::Local(field_value_0) => {
                        pending.push(Work::W145(field_value_0, nesting + 1));
                    }
                    SemanticOperand::TypeArgument {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticOperand::Environment(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    SemanticOperand::CoreExpression {
                        expression: field_expression,
                        occurrence: field_occurrence,
                    } => {
                        pending.push(Work::W6(field_occurrence, nesting + 1));
                        pending.push(Work::W56(field_expression, nesting + 1));
                    }
                }
            }
            Work::W142(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W143(item, nesting));
                }
            }
            Work::W143(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    LifeTypeProjection::Place(field_value_0) => {
                        pending.push(Work::W176(field_value_0, nesting + 1));
                    }
                    LifeTypeProjection::TypeArgument(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    LifeTypeProjection::Pointee => {}
                    LifeTypeProjection::Element => {}
                    LifeTypeProjection::Parameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    LifeTypeProjection::Result => {}
                    LifeTypeProjection::Port(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    LifeTypeProjection::AssociatedType(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W144(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W145(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W146(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W147(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticSiteRecipeKind::Construct {
                        result: field_result,
                    } => {
                        pending.push(Work::W113(field_result, nesting + 1));
                    }
                    SemanticSiteRecipeKind::Call {
                        target: field_target,
                        actuals: field_actuals,
                    } => {
                        pending.push(Work::W148(field_actuals, nesting + 1));
                        pending.push(Work::W84(field_target, nesting + 1));
                    }
                    SemanticSiteRecipeKind::CallableCall {
                        callee: field_callee,
                    } => {
                        pending.push(Work::W113(field_callee, nesting + 1));
                    }
                    SemanticSiteRecipeKind::TypeUse {
                        expected: field_expected,
                    } => {
                        pending.push(Work::W113(field_expected, nesting + 1));
                    }
                    SemanticSiteRecipeKind::DeclareLocal {
                        expected: field_expected,
                    } => {
                        pending.push(Work::W113(field_expected, nesting + 1));
                    }
                    SemanticSiteRecipeKind::Closure {
                        definition: field_definition,
                        environment: field_environment,
                    } => {
                        pending.push(Work::W148(field_environment, nesting + 1));
                        pending.push(Work::W84(field_definition, nesting + 1));
                    }
                }
            }
            Work::W148(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W122(item, nesting));
                }
            }
            Work::W149(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticSiteAddress::Core {
                        expression: field_expression,
                        occurrence: field_occurrence,
                        role: field_role,
                    } => {
                        pending.push(Work::W151(field_role, nesting + 1));
                        pending.push(Work::W6(field_occurrence, nesting + 1));
                        pending.push(Work::W56(field_expression, nesting + 1));
                    }
                    SemanticSiteAddress::Typed {
                        block: field_block,
                        instruction: field_instruction,
                        edge: field_edge,
                    } => {
                        pending.push(Work::W153(field_edge, nesting + 1));
                        pending.push(Work::W6(field_instruction, nesting + 1));
                        pending.push(Work::W152(field_block, nesting + 1));
                    }
                }
            }
            Work::W150(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticSiteAction::Construct {
                        result: field_result,
                        value: field_value,
                    } => {
                        pending.push(Work::W133(field_value, nesting + 1));
                        pending.push(Work::W141(field_result, nesting + 1));
                    }
                    SemanticSiteAction::Call {
                        target: field_target,
                        actuals: field_actuals,
                    } => {
                        pending.push(Work::W126(field_actuals, nesting + 1));
                        pending.push(Work::W134(field_target, nesting + 1));
                    }
                    SemanticSiteAction::CallableCall {
                        callee: field_callee,
                    } => {
                        pending.push(Work::W141(field_callee, nesting + 1));
                    }
                    SemanticSiteAction::TypeUse {
                        subject: field_subject,
                        expected: field_expected,
                    } => {
                        pending.push(Work::W133(field_expected, nesting + 1));
                        pending.push(Work::W141(field_subject, nesting + 1));
                    }
                    SemanticSiteAction::DeclareLocal {
                        local: field_local,
                        expected: field_expected,
                    } => {
                        pending.push(Work::W133(field_expected, nesting + 1));
                        pending.push(Work::W154(field_local, nesting + 1));
                    }
                    SemanticSiteAction::Closure {
                        result: field_result,
                        closure: field_closure,
                        environment: field_environment,
                    } => {
                        pending.push(Work::W126(field_environment, nesting + 1));
                        pending.push(Work::W135(field_closure, nesting + 1));
                        pending.push(Work::W141(field_result, nesting + 1));
                    }
                }
            }
            Work::W151(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticEmissionRole::Construct => {}
                    SemanticEmissionRole::Call => {}
                    SemanticEmissionRole::Ascription => {}
                    SemanticEmissionRole::Binding => {}
                    SemanticEmissionRole::Closure => {}
                }
            }
            Work::W152(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W153(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticSiteEdge::Before => {}
                    SemanticSiteEdge::After => {}
                }
            }
            Work::W154(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticLocalTarget::Core {
                        statement: field_statement,
                        occurrence: field_occurrence,
                    } => {
                        pending.push(Work::W6(field_occurrence, nesting + 1));
                        pending.push(Work::W56(field_statement, nesting + 1));
                    }
                    SemanticLocalTarget::Typed(field_value_0) => {
                        pending.push(Work::W145(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W155(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticRecipeOperand::TypeFormal(field_value_0) => {
                        pending.push(Work::W156(field_value_0, nesting + 1));
                    }
                    SemanticRecipeOperand::Environment {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticRecipeOperand::Parameter(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    SemanticRecipeOperand::Receiver => {}
                }
            }
            Work::W156(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W6(&value.ordinal, nesting + 1));
                pending.push(Work::W140(&value.scope, nesting + 1));
            }
            Work::W157(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticLifeRecipe::Formal(field_value_0) => {
                        pending.push(Work::W156(field_value_0, nesting + 1));
                    }
                    SemanticLifeRecipe::Implicit {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticLifeRecipe::Environment {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticLifeRecipe::Bound {
                        depth: field_depth,
                        index: field_index,
                    } => {
                        pending.push(Work::W6(field_index, nesting + 1));
                        pending.push(Work::W6(field_depth, nesting + 1));
                    }
                    SemanticLifeRecipe::Invocation(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    SemanticLifeRecipe::OuterOccurrence {
                        operand: field_operand,
                        path: field_path,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W52(field_mode, nesting + 1));
                        pending.push(Work::W142(field_path, nesting + 1));
                        pending.push(Work::W155(field_operand, nesting + 1));
                    }
                    SemanticLifeRecipe::IntrinsicClass {
                        operand: field_operand,
                        path: field_path,
                        class: field_class,
                    } => {
                        pending.push(Work::W6(field_class, nesting + 1));
                        pending.push(Work::W142(field_path, nesting + 1));
                        pending.push(Work::W155(field_operand, nesting + 1));
                    }
                }
            }
            Work::W158(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticOuterLifeRecipe::Class(field_value_0) => {
                        pending.push(Work::W157(field_value_0, nesting + 1));
                    }
                    SemanticOuterLifeRecipe::Static => {}
                }
            }
            Work::W159(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W165(&value.classes, nesting + 1));
                pending.push(Work::W2(&value.life, nesting + 1));
                pending.push(Work::W1(&value.ty, nesting + 1));
            }
            Work::W160(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticConstRecipe::Value(field_value_0) => {
                        pending.push(Work::W76(field_value_0, nesting + 1));
                    }
                    SemanticConstRecipe::IntegerLiteral(field_value_0) => {
                        pending.push(Work::W120(field_value_0, nesting + 1));
                    }
                    SemanticConstRecipe::FloatLiteral(field_value_0) => {
                        pending.push(Work::W120(field_value_0, nesting + 1));
                    }
                    SemanticConstRecipe::Formal(field_value_0) => {
                        pending.push(Work::W156(field_value_0, nesting + 1));
                    }
                    SemanticConstRecipe::Environment {
                        scope: field_scope,
                        ordinal: field_ordinal,
                    } => {
                        pending.push(Work::W6(field_ordinal, nesting + 1));
                        pending.push(Work::W140(field_scope, nesting + 1));
                    }
                    SemanticConstRecipe::Unary {
                        op: field_op,
                        operand_type: field_operand_type,
                        value: field_value,
                    } => {
                        pending.push(Work::W160(field_value, nesting + 1));
                        pending.push(Work::W113(field_operand_type, nesting + 1));
                        pending.push(Work::W179(field_op, nesting + 1));
                    }
                    SemanticConstRecipe::Binary {
                        op: field_op,
                        operand_type: field_operand_type,
                        left: field_left,
                        right: field_right,
                    } => {
                        pending.push(Work::W160(field_right, nesting + 1));
                        pending.push(Work::W160(field_left, nesting + 1));
                        pending.push(Work::W113(field_operand_type, nesting + 1));
                        pending.push(Work::W180(field_op, nesting + 1));
                    }
                    SemanticConstRecipe::Convert {
                        source_type: field_source_type,
                        target_type: field_target_type,
                        value: field_value,
                    } => {
                        pending.push(Work::W160(field_value, nesting + 1));
                        pending.push(Work::W113(field_target_type, nesting + 1));
                        pending.push(Work::W113(field_source_type, nesting + 1));
                    }
                    SemanticConstRecipe::AssociatedConst {
                        receiver: field_receiver,
                        application: field_application,
                        member: field_member,
                    } => {
                        pending.push(Work::W6(field_member, nesting + 1));
                        pending.push(Work::W114(field_application, nesting + 1));
                        pending.push(Work::W113(field_receiver, nesting + 1));
                    }
                }
            }
            Work::W161(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W173(&value.nominal, nesting + 1));
                pending.push(Work::W171(&value.retained, nesting + 1));
                pending.push(Work::W177(&value.ports, nesting + 1));
                pending.push(Work::W166(&value.domain, nesting + 1));
            }
            Work::W162(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W173(&value.nominal, nesting + 1));
                pending.push(Work::W171(&value.retained, nesting + 1));
                pending.push(Work::W169(&value.slots, nesting + 1));
                pending.push(Work::W166(&value.domain, nesting + 1));
            }
            Work::W163(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticCodeKind::FunctionPointer => {}
                    SemanticCodeKind::BorrowedFunctionPointer => {}
                    SemanticCodeKind::ForeignCallback => {}
                }
            }
            Work::W164(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W184(&value.associated, nesting + 1));
                pending.push(Work::W183(&value.context, nesting + 1));
                pending.push(Work::W114(&value.application, nesting + 1));
            }
            Work::W165(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W157(item, nesting));
                }
            }
            Work::W166(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W167(&value.parts, nesting + 1));
            }
            Work::W167(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W168(item, nesting));
                }
            }
            Work::W168(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticClassSequencePart::One(field_value_0) => {
                        pending.push(Work::W157(field_value_0, nesting + 1));
                    }
                    SemanticClassSequencePart::AllClasses {
                        ty: field_ty,
                        path: field_path,
                    } => {
                        pending.push(Work::W142(field_path, nesting + 1));
                        pending.push(Work::W113(field_ty, nesting + 1));
                    }
                }
            }
            Work::W169(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W170(item, nesting));
                }
            }
            Work::W170(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W157(&value.class, nesting + 1));
                pending.push(Work::W52(&value.mode, nesting + 1));
                pending.push(Work::W175(&value.path, nesting + 1));
            }
            Work::W171(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W172(item, nesting));
                }
            }
            Work::W172(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticRetainedPart::One {
                        path: field_path,
                        child_class: field_child_class,
                        parent: field_parent,
                    } => {
                        pending.push(Work::W157(field_parent, nesting + 1));
                        pending.push(Work::W6(field_child_class, nesting + 1));
                        pending.push(Work::W142(field_path, nesting + 1));
                    }
                    SemanticRetainedPart::All {
                        path: field_path,
                        parents: field_parents,
                    } => {
                        pending.push(Work::W166(field_parents, nesting + 1));
                        pending.push(Work::W142(field_path, nesting + 1));
                    }
                }
            }
            Work::W173(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W174(item, nesting));
                }
            }
            Work::W174(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticNominalPart::One {
                        path: field_path,
                        declaration: field_declaration,
                        argument: field_argument,
                        class: field_class,
                    } => {
                        pending.push(Work::W157(field_class, nesting + 1));
                        pending.push(Work::W6(field_argument, nesting + 1));
                        pending.push(Work::W74(field_declaration, nesting + 1));
                        pending.push(Work::W142(field_path, nesting + 1));
                    }
                    SemanticNominalPart::FromType {
                        path: field_path,
                        ty: field_ty,
                        classes: field_classes,
                    } => {
                        pending.push(Work::W166(field_classes, nesting + 1));
                        pending.push(Work::W113(field_ty, nesting + 1));
                        pending.push(Work::W142(field_path, nesting + 1));
                    }
                }
            }
            Work::W175(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W176(item, nesting));
                }
            }
            Work::W176(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    PlaceProjection::Field(field_value_0) => {
                        pending.push(Work::W6(field_value_0, nesting + 1));
                    }
                    PlaceProjection::Variant(field_value_0) => {
                        pending.push(Work::W191(field_value_0, nesting + 1));
                    }
                    PlaceProjection::ErrorArm(field_value_0) => {
                        pending.push(Work::W33(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W177(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W178(item, nesting));
                }
            }
            Work::W178(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticPortPart::One {
                        leaf: field_leaf,
                        mode: field_mode,
                        class: field_class,
                    } => {
                        pending.push(Work::W157(field_class, nesting + 1));
                        pending.push(Work::W52(field_mode, nesting + 1));
                        pending.push(Work::W113(field_leaf, nesting + 1));
                    }
                    SemanticPortPart::FromType {
                        ty: field_ty,
                        classes: field_classes,
                    } => {
                        pending.push(Work::W166(field_classes, nesting + 1));
                        pending.push(Work::W113(field_ty, nesting + 1));
                    }
                }
            }
            Work::W179(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W180(value, nesting) => {
                let _ = value;
                visitor.node(nesting)?;
            }
            Work::W181(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W182(item, nesting));
                }
            }
            Work::W182(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W23(&value.mode, nesting + 1));
                pending.push(Work::W113(&value.ty, nesting + 1));
            }
            Work::W183(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticDynContextRecipe::Closed => {}
                    SemanticDynContextRecipe::Type(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                }
            }
            Work::W184(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W185(item, nesting));
                }
            }
            Work::W185(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W186(&value.value, nesting + 1));
                pending.push(Work::W6(&value.member, nesting + 1));
                pending.push(Work::W114(&value.application, nesting + 1));
            }
            Work::W186(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    SemanticAssociatedRecipe::Type(field_value_0) => {
                        pending.push(Work::W113(field_value_0, nesting + 1));
                    }
                    SemanticAssociatedRecipe::Const {
                        carrier: field_carrier,
                        value: field_value,
                    } => {
                        pending.push(Work::W160(field_value, nesting + 1));
                        pending.push(Work::W113(field_carrier, nesting + 1));
                    }
                }
            }
            Work::W187(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W188(item, nesting));
                }
            }
            Work::W188(value, nesting) => {
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
            Work::W189(value, nesting) => {
                visitor.node(nesting)?;
                if let Some(item) = value {
                    pending.push(Work::W14(item, nesting));
                }
            }
            Work::W190(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W16(item, nesting));
                }
            }
            Work::W191(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W6(&value.index, nesting + 1));
                pending.push(Work::W6(&value.variant, nesting + 1));
            }
            Work::W192(value, nesting) => {
                visitor.node(nesting)?;
                pending.push(Work::W193(&value.sources, nesting + 1));
                pending.push(Work::W28(&value.position, nesting + 1));
            }
            Work::W193(value, nesting) => {
                visitor.rows(value.len(), nesting)?;
                for item in value.iter().rev() {
                    pending.push(Work::W194(item, nesting));
                }
            }
            Work::W194(value, nesting) => {
                visitor.node(nesting)?;
                match value {
                    StorageSource::Exact(field_value_0) => {
                        pending.push(Work::W28(field_value_0, nesting + 1));
                    }
                    StorageSource::PlaceAddress {
                        place: field_place,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W52(field_mode, nesting + 1));
                        pending.push(Work::W28(field_place, nesting + 1));
                    }
                    StorageSource::OwnedView {
                        head: field_head,
                        leaf: field_leaf,
                        mode: field_mode,
                    } => {
                        pending.push(Work::W52(field_mode, nesting + 1));
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
                        pending.push(Work::W52(field_mode, nesting + 1));
                        pending.push(Work::W1(field_leaf, nesting + 1));
                    }
                }
            }
            Work::W195(value, nesting) => {
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
                        pending.push(Work::W193(field_sources, nesting + 1));
                    }
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
impl DynRecipeMetadata for DynStorageSourceRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W51(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynStorageSourceBindingRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W30(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynResultSourceRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W44(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionId {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W54(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionScope {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W57(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionDeclaration {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W58(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynMethodRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W63(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynFunctionInstance {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W65(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedValueRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W69(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedDefinition {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W70(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W73(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedValue {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W75(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynAssociatedResolution {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W77(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynImplementationWitness {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W79(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynFunctionTarget {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W82(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynDefinitionSpecialization {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W61(self, nesting), visitor)
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
impl DynRecipeMetadata for SemanticDefinitionId {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W84(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticInstanceId {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W85(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticModule {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W86(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticDynScopeBinding {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W94(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticDynOwner {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W95(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticDefinition {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W88(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticDefinitionKind {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W97(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticDefinitionBody {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W106(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticScopeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W112(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticBuiltinTrait {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W115(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticNominalBody {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W111(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticFieldRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W117(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticVariantRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W119(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticFormal {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W99(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticImplicitLifetime {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W101(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticEnvironmentFormal {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W105(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticInstance {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W90(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticLifetimeInstantiation {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W124(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticFunctionTarget {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W134(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticInstanceTarget {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W123(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticClosureTarget {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W135(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticHeaderBinding {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W129(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticEnvironmentInstance {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W131(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticActual {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W127(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticCoreTypeSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W138(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticTypeValue {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W133(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticScopeRef {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W140(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticClassExpr {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W137(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticOperand {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W141(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticSiteRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W109(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticSiteRecipeKind {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W147(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticSite {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W92(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticSiteAddress {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W149(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticEmissionRole {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W151(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticSiteEdge {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W153(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticSiteAction {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W150(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticLocalTarget {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W154(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticRecipeOperand {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W155(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticFormalRef {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W156(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticLifeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W157(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticOuterLifeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W158(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticTypeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W113(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticExactTypeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W159(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticCodeKind {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W163(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticActualRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W122(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticClassSequenceRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W166(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticClassSequencePart {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W168(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticStorageShapeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W162(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticSlotRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W170(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticRetainedPart {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W172(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticNominalPart {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W174(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticRetainedShapeRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W161(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticPortPart {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W178(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticConstRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W160(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticTraitRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W114(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticRequirementRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W103(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticSignatureRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W107(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticParameterRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W182(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticDynContextRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W183(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticDynInterfaceRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W164(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticAssociatedBindingRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W185(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for SemanticAssociatedRecipe {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W186(self, nesting), visitor)
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
        visit(Work::W188(self, nesting), visitor)
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
impl DynRecipeMetadata for StoredBorrowMode {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W52(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for LifeNominalKind {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W110(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for LifeTypeProjection {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W143(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for PlaceProjection {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W176(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for VariantProjection {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W191(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for StorageSourceBinding {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W192(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for StorageSource {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W194(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynGenericActual {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W68(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynTraitApplication {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W80(self, nesting), visitor)
    }
}
impl DynRecipeMetadata for DynMember {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        nesting: usize,
    ) -> Result<(), V::Error> {
        visit(Work::W78(self, nesting), visitor)
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
        visit(Work::W195(self, nesting), visitor)
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

//! Generated bounded transport for the complete CT dynamic catalogue.
use super::*;
use crate::*;
impl Wire for CtDynInterfaceId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.module.encode(e)?;
            self.index.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                index: <u32 as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynVtableId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.module.encode(e)?;
            self.index.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                index: <u32 as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynDeclaration {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.implicit_lifetimes.encode(e)?;
            self.direct_supertraits.encode(e)?;
            self.requirements.encode(e)?;
            self.associated_definitions.encode(e)?;
            self.method_signatures.encode(e)?;
            self.method_definitions.encode(e)?;
            self.self_pattern.encode(e)?;
            self.implemented_trait.encode(e)?;
            self.id.encode(e)?;
            self.identity.encode(e)?;
            self.kind.encode(e)?;
            self.parameters.encode(e)?;
            self.members.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                implicit_lifetimes: <Option<Vec<CtDynImplicitLifeDeclaration>> as Wire>::decode(d)?,
                direct_supertraits: <Option<Vec<CtDynSelfPattern>> as Wire>::decode(d)?,
                requirements: <Option<Vec<CtDynObligation>> as Wire>::decode(d)?,
                associated_definitions: <Option<Vec<CtDynAssociatedDefinition>> as Wire>::decode(
                    d,
                )?,
                method_signatures: <Option<Vec<CtDynMethodRecipe>> as Wire>::decode(d)?,
                method_definitions: <Option<Vec<CtDynDefinitionDeclaration>> as Wire>::decode(d)?,
                self_pattern: <Option<CtDynTypePattern> as Wire>::decode(d)?,
                implemented_trait: <Option<CtDynSelfPattern> as Wire>::decode(d)?,
                id: <LifeDeclarationId as Wire>::decode(d)?,
                identity: <LifeDeclarationIdentity as Wire>::decode(d)?,
                kind: <CtDynDeclarationKind as Wire>::decode(d)?,
                parameters: <Vec<CtLifeParameterKind> as Wire>::decode(d)?,
                members: <Vec<CtDynMemberDeclaration> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynSelfPattern {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.declaration.encode(e)?;
            self.arguments.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                declaration: <LifeDeclarationId as Wire>::decode(d)?,
                arguments: <Vec<CtDynPatternActual> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynPatternActual {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Const(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Life(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(<CtDynTypePattern as Wire>::decode(d)?),
                1 => Self::Const(<CtDynConstPattern as Wire>::decode(d)?),
                2 => Self::Life(<CtDynLifePattern as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynConstPattern {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::IntegerLiteral(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::FloatLiteral(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::MethodParameter(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Unary { op, value, ty } => {
                    3u8.encode(e)?;
                    op.encode(e)?;
                    value.encode(e)?;
                    ty.encode(e)?;
                }
                Self::Binary {
                    op,
                    left,
                    right,
                    ty,
                } => {
                    4u8.encode(e)?;
                    op.encode(e)?;
                    left.encode(e)?;
                    right.encode(e)?;
                    ty.encode(e)?;
                }
                Self::Convert { value, ty } => {
                    5u8.encode(e)?;
                    value.encode(e)?;
                    ty.encode(e)?;
                }
                Self::AssociatedProjection {
                    receiver,
                    application,
                    member,
                } => {
                    6u8.encode(e)?;
                    receiver.encode(e)?;
                    application.encode(e)?;
                    member.encode(e)?;
                }
                Self::Value(value_0) => {
                    7u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Parameter(value_0) => {
                    8u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::IntegerLiteral(<String as Wire>::decode(d)?),
                1 => Self::FloatLiteral(<String as Wire>::decode(d)?),
                2 => Self::MethodParameter(<u32 as Wire>::decode(d)?),
                3 => Self::Unary {
                    op: <UnaryOp as Wire>::decode(d)?,
                    value: <Box<CtDynConstPattern> as Wire>::decode(d)?,
                    ty: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                },
                4 => Self::Binary {
                    op: <BinaryOp as Wire>::decode(d)?,
                    left: <Box<CtDynConstPattern> as Wire>::decode(d)?,
                    right: <Box<CtDynConstPattern> as Wire>::decode(d)?,
                    ty: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                },
                5 => Self::Convert {
                    value: <Box<CtDynConstPattern> as Wire>::decode(d)?,
                    ty: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                },
                6 => Self::AssociatedProjection {
                    receiver: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                    application: <Box<CtDynSelfPattern> as Wire>::decode(d)?,
                    member: <u32 as Wire>::decode(d)?,
                },
                7 => Self::Value(<Constant as Wire>::decode(d)?),
                8 => Self::Parameter(<u32 as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynTypePattern {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::SelfType => {
                    0u8.encode(e)?;
                }
                Self::MethodParameter(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::AssociatedProjection {
                    receiver,
                    application,
                    member,
                } => {
                    2u8.encode(e)?;
                    receiver.encode(e)?;
                    application.encode(e)?;
                    member.encode(e)?;
                }
                Self::Concrete(value_0) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Parameter(value_0) => {
                    4u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Nominal(value_0) => {
                    5u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Address {
                    mutable,
                    lifetime,
                    inner,
                } => {
                    6u8.encode(e)?;
                    mutable.encode(e)?;
                    lifetime.encode(e)?;
                    inner.encode(e)?;
                }
                Self::Tuple(value_0) => {
                    7u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Array { element, length } => {
                    8u8.encode(e)?;
                    element.encode(e)?;
                    length.encode(e)?;
                }
                Self::Slice(value_0) => {
                    9u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Managed(value_0) => {
                    10u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Desc { lifetime, inner } => {
                    11u8.encode(e)?;
                    lifetime.encode(e)?;
                    inner.encode(e)?;
                }
                Self::OwnedStore(value_0) => {
                    12u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::ErrorUnion { error, result } => {
                    13u8.encode(e)?;
                    error.encode(e)?;
                    result.encode(e)?;
                }
                Self::Semantic { scope, ty } => {
                    14u8.encode(e)?;
                    scope.encode(e)?;
                    ty.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::SelfType,
                1 => Self::MethodParameter(<u32 as Wire>::decode(d)?),
                2 => Self::AssociatedProjection {
                    receiver: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                    application: <Box<CtDynSelfPattern> as Wire>::decode(d)?,
                    member: <u32 as Wire>::decode(d)?,
                },
                3 => Self::Concrete(<CtDynamicTypeId as Wire>::decode(d)?),
                4 => Self::Parameter(<u32 as Wire>::decode(d)?),
                5 => Self::Nominal(<Box<CtDynSelfPattern> as Wire>::decode(d)?),
                6 => Self::Address {
                    mutable: <bool as Wire>::decode(d)?,
                    lifetime: <Option<CtDynLifePattern> as Wire>::decode(d)?,
                    inner: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                },
                7 => Self::Tuple(<Vec<CtDynTypePattern> as Wire>::decode(d)?),
                8 => Self::Array {
                    element: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                    length: <CtDynConstPattern as Wire>::decode(d)?,
                },
                9 => Self::Slice(<Box<CtDynTypePattern> as Wire>::decode(d)?),
                10 => Self::Managed(<Box<CtDynTypePattern> as Wire>::decode(d)?),
                11 => Self::Desc {
                    lifetime: <Option<CtDynLifePattern> as Wire>::decode(d)?,
                    inner: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                },
                12 => Self::OwnedStore(<Box<CtDynTypePattern> as Wire>::decode(d)?),
                13 => Self::ErrorUnion {
                    error: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                    result: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                },
                14 => Self::Semantic {
                    scope: <CtSemanticDefinitionId as Wire>::decode(d)?,
                    ty: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynDeclarationKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Trait => {
                    0u8.encode(e)?;
                }
                Self::Impl => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Trait,
                1 => Self::Impl,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynMemberDeclaration {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.name.encode(e)?;
            self.kind.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                name: <String as Wire>::decode(d)?,
                kind: <CtDynMemberKind as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynMemberKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type => {
                    0u8.encode(e)?;
                }
                Self::Constant => {
                    1u8.encode(e)?;
                }
                Self::Method(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type,
                1 => Self::Constant,
                2 => Self::Method(<Vec<CtLifeParameterKind> as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynGenericActual {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::QualifiedType(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Const(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Life(value_0) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(<CtDynamicTypeId as Wire>::decode(d)?),
                1 => Self::QualifiedType(<CtDynTypeValue as Wire>::decode(d)?),
                2 => Self::Const(<Constant as Wire>::decode(d)?),
                3 => Self::Life(<StorageBinder as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynTraitApplication {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.declaration.encode(e)?;
            self.actuals.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                declaration: <LifeDeclarationId as Wire>::decode(d)?,
                actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynMember {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.application.encode(e)?;
            self.ordinal.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                application: <CtDynTraitApplication as Wire>::decode(d)?,
                ordinal: <u32 as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynAssociatedType {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.member.encode(e)?;
            self.ty.encode(e)?;
            self.life.encode(e)?;
            self.classes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                member: <CtDynMember as Wire>::decode(d)?,
                ty: <CtDynamicTypeId as Wire>::decode(d)?,
                life: <StorageLifeShape as Wire>::decode(d)?,
                classes: <Vec<StorageBinder> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynAssociatedConstant {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.member.encode(e)?;
            self.ty.encode(e)?;
            self.value.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                member: <CtDynMember as Wire>::decode(d)?,
                ty: <CtDynamicTypeId as Wire>::decode(d)?,
                value: <Constant as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynReceiver {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.mutable.encode(e)?;
            self.mode.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                mutable: <bool as Wire>::decode(d)?,
                mode: <PassingMode as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynSlot {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.member.encode(e)?;
            self.actuals.encode(e)?;
            self.receiver.encode(e)?;
            self.parameters.encode(e)?;
            self.parameter_modes.encode(e)?;
            self.result.encode(e)?;
            self.result_mode.encode(e)?;
            self.call_kind.encode(e)?;
            self.storage_life.encode(e)?;
            self.contract.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                member: <CtDynMember as Wire>::decode(d)?,
                actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
                receiver: <CtDynReceiver as Wire>::decode(d)?,
                parameters: <Vec<CtDynamicTypeId> as Wire>::decode(d)?,
                parameter_modes: <Vec<PassingMode> as Wire>::decode(d)?,
                result: <CtDynamicTypeId as Wire>::decode(d)?,
                result_mode: <PassingMode as Wire>::decode(d)?,
                call_kind: <CallKind as Wire>::decode(d)?,
                storage_life: <Option<StorageLifeContract> as Wire>::decode(d)?,
                contract: <CtDynContract as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynInterface {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.context.encode(e)?;
            self.owning_cleanup.encode(e)?;
            self.id.encode(e)?;
            self.application.encode(e)?;
            self.inherited.encode(e)?;
            self.associated_types.encode(e)?;
            self.associated_constants.encode(e)?;
            self.retained_life.encode(e)?;
            self.slots.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                context: <Option<CtDynamicTypeId> as Wire>::decode(d)?,
                owning_cleanup: <Option<CtDynCleanupContract> as Wire>::decode(d)?,
                id: <CtDynInterfaceId as Wire>::decode(d)?,
                application: <CtDynTraitApplication as Wire>::decode(d)?,
                inherited: <Vec<CtDynTraitApplication> as Wire>::decode(d)?,
                associated_types: <Vec<CtDynAssociatedType> as Wire>::decode(d)?,
                associated_constants: <Vec<CtDynAssociatedConstant> as Wire>::decode(d)?,
                retained_life: <CtRetainedLifeShape as Wire>::decode(d)?,
                slots: <Vec<CtDynSlot> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtCoreDynEntry {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.slot.encode(e)?;
            self.module.encode(e)?;
            self.function.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                slot: <u32 as Wire>::decode(d)?,
                module: <ModuleId as Wire>::decode(d)?,
                function: <NodeId as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynEntry {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.slot.encode(e)?;
            self.function.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                slot: <u32 as Wire>::decode(d)?,
                function: <FunctionId as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynRetainedPortBinding {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.port.encode(e)?;
            self.sources.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                port: <u32 as Wire>::decode(d)?,
                sources: <Vec<StoragePosition> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtCoreDynVtable {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.self_life.encode(e)?;
            self.implementation_witnesses.encode(e)?;
            self.self_classes.encode(e)?;
            self.id.encode(e)?;
            self.interface.encode(e)?;
            self.self_type.encode(e)?;
            self.implementation.encode(e)?;
            self.actuals.encode(e)?;
            self.ports.encode(e)?;
            self.entries.encode(e)?;
            self.cleanup.encode(e)?;
            self.owned_storage.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                self_life: <Option<StorageLifeShape> as Wire>::decode(d)?,
                implementation_witnesses:
                    <Option<Vec<CtDynImplementationWitness>> as Wire>::decode(d)?,
                self_classes: <Vec<StorageBinder> as Wire>::decode(d)?,
                id: <CtDynVtableId as Wire>::decode(d)?,
                interface: <CtDynInterfaceId as Wire>::decode(d)?,
                self_type: <CtDynamicTypeId as Wire>::decode(d)?,
                implementation: <LifeDeclarationId as Wire>::decode(d)?,
                actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
                ports: <Vec<CtDynRetainedPortBinding> as Wire>::decode(d)?,
                entries: <Vec<CtCoreDynEntry> as Wire>::decode(d)?,
                cleanup: <Option<CtCoreDynEntry> as Wire>::decode(d)?,
                owned_storage: <Option<CtDynOwnedStorage> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynVtable {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.self_life.encode(e)?;
            self.implementation_witnesses.encode(e)?;
            self.self_classes.encode(e)?;
            self.id.encode(e)?;
            self.interface.encode(e)?;
            self.self_type.encode(e)?;
            self.implementation.encode(e)?;
            self.actuals.encode(e)?;
            self.ports.encode(e)?;
            self.entries.encode(e)?;
            self.cleanup.encode(e)?;
            self.owned_storage.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                self_life: <Option<StorageLifeShape> as Wire>::decode(d)?,
                implementation_witnesses:
                    <Option<Vec<CtDynImplementationWitness>> as Wire>::decode(d)?,
                self_classes: <Vec<StorageBinder> as Wire>::decode(d)?,
                id: <CtDynVtableId as Wire>::decode(d)?,
                interface: <CtDynInterfaceId as Wire>::decode(d)?,
                self_type: <CtDynamicTypeId as Wire>::decode(d)?,
                implementation: <LifeDeclarationId as Wire>::decode(d)?,
                actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
                ports: <Vec<CtDynRetainedPortBinding> as Wire>::decode(d)?,
                entries: <Vec<CtDynEntry> as Wire>::decode(d)?,
                cleanup: <Option<CtDynEntry> as Wire>::decode(d)?,
                owned_storage: <Option<CtDynOwnedStorage> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynOwnedStorage {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Heap => {
                    0u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Heap,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynRequirement {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Read(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Write(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Manage(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Apart(value_0, value_1) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                    value_1.encode(e)?;
                }
                Self::State {
                    position,
                    ty,
                    variant,
                } => {
                    4u8.encode(e)?;
                    position.encode(e)?;
                    ty.encode(e)?;
                    variant.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Read(<StoragePosition as Wire>::decode(d)?),
                1 => Self::Write(<StoragePosition as Wire>::decode(d)?),
                2 => Self::Manage(<StoragePosition as Wire>::decode(d)?),
                3 => Self::Apart(
                    <StoragePosition as Wire>::decode(d)?,
                    <StoragePosition as Wire>::decode(d)?,
                ),
                4 => Self::State {
                    position: <StoragePosition as Wire>::decode(d)?,
                    ty: <CtDynamicTypeId as Wire>::decode(d)?,
                    variant: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynGuarantee {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::StorageSources(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::ManageResult => {
                    1u8.encode(e)?;
                }
                Self::State {
                    position,
                    ty,
                    variant,
                } => {
                    2u8.encode(e)?;
                    position.encode(e)?;
                    ty.encode(e)?;
                    variant.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::StorageSources(<CtStorageSourceBinding as Wire>::decode(d)?),
                1 => Self::ManageResult,
                2 => Self::State {
                    position: <StoragePosition as Wire>::decode(d)?,
                    ty: <CtDynamicTypeId as Wire>::decode(d)?,
                    variant: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynEffect {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Alloc => {
                    0u8.encode(e)?;
                }
                Self::Io => {
                    1u8.encode(e)?;
                }
                Self::Block => {
                    2u8.encode(e)?;
                }
                Self::Suspend => {
                    3u8.encode(e)?;
                }
                Self::Spawn => {
                    4u8.encode(e)?;
                }
                Self::Ffi => {
                    5u8.encode(e)?;
                }
                Self::Reenter => {
                    6u8.encode(e)?;
                }
                Self::Panic => {
                    7u8.encode(e)?;
                }
                Self::Call(value_0) => {
                    8u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Cleanup(value_0) => {
                    9u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Alloc,
                1 => Self::Io,
                2 => Self::Block,
                3 => Self::Suspend,
                4 => Self::Spawn,
                5 => Self::Ffi,
                6 => Self::Reenter,
                7 => Self::Panic,
                8 => Self::Call(<StoragePosition as Wire>::decode(d)?),
                9 => Self::Cleanup(<CtDynamicTypeId as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynCallAccess {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.reads.encode(e)?;
            self.writes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                reads: <Vec<StoragePosition> as Wire>::decode(d)?,
                writes: <Vec<StoragePosition> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynResultSource {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Value => {
                    0u8.encode(e)?;
                }
                Self::Create => {
                    1u8.encode(e)?;
                }
                Self::Give => {
                    2u8.encode(e)?;
                }
                Self::Borrow { sources, mutable } => {
                    3u8.encode(e)?;
                    sources.encode(e)?;
                    mutable.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Value,
                1 => Self::Create,
                2 => Self::Give,
                3 => Self::Borrow {
                    sources: <Vec<CtStorageSource> as Wire>::decode(d)?,
                    mutable: <bool as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynCleanupContract {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.environment.encode(e)?;
            self.positions.encode(e)?;
            self.access.encode(e)?;
            self.effects.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                environment: <bool as Wire>::decode(d)?,
                positions: <Vec<StoragePosition> as Wire>::decode(d)?,
                access: <CtDynCallAccess as Wire>::decode(d)?,
                effects: <Vec<CtDynEffect> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynResumeBound {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Local => {
                    0u8.encode(e)?;
                }
                Self::Transfer => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Local,
                1 => Self::Transfer,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynThreadRequirements {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.transfer.encode(e)?;
            self.share.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                transfer: <bool as Wire>::decode(d)?,
                share: <bool as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynContract {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.requires.encode(e)?;
            self.guarantees.encode(e)?;
            self.hold.encode(e)?;
            self.call.encode(e)?;
            self.result.encode(e)?;
            self.cleanup.encode(e)?;
            self.effects.encode(e)?;
            self.resume.encode(e)?;
            self.threads.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                requires: <Option<Vec<CtDynRequirement>> as Wire>::decode(d)?,
                guarantees: <Option<Vec<CtDynGuarantee>> as Wire>::decode(d)?,
                hold: <Option<Vec<StoragePosition>> as Wire>::decode(d)?,
                call: <Option<CtDynCallAccess> as Wire>::decode(d)?,
                result: <Option<CtDynResultSource> as Wire>::decode(d)?,
                cleanup: <Option<CtDynCleanupContract> as Wire>::decode(d)?,
                effects: <Option<Vec<CtDynEffect>> as Wire>::decode(d)?,
                resume: <Option<CtDynResumeBound> as Wire>::decode(d)?,
                threads: <Option<CtDynThreadRequirements> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynTypeValue {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.ty.encode(e)?;
            self.life.encode(e)?;
            self.classes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                ty: <CtDynamicTypeId as Wire>::decode(d)?,
                life: <StorageLifeShape as Wire>::decode(d)?,
                classes: <Vec<StorageBinder> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynInstantiationScope {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.fixed_binder_count.encode(e)?;
            self.invocation_binder_count.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                fixed_binder_count: <u32 as Wire>::decode(d)?,
                invocation_binder_count: <u32 as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynLifeEmbedding {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.from.encode(e)?;
            self.to.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                from: <StorageBinder as Wire>::decode(d)?,
                to: <StorageBinder as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynImplicitLifeKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Address => {
                    0u8.encode(e)?;
                }
                Self::Description => {
                    1u8.encode(e)?;
                }
                Self::LifetimeArgument => {
                    2u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Address,
                1 => Self::Description,
                2 => Self::LifetimeArgument,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynImplicitLifeRoot {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::SelfPattern => {
                    0u8.encode(e)?;
                }
                Self::ImplementedTrait => {
                    1u8.encode(e)?;
                }
                Self::Supertrait(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Requirement(value_0) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::AssociatedDefinition(value_0) => {
                    4u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::MethodSignature(value_0) => {
                    5u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::MethodDefinition(value_0) => {
                    6u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::SelfPattern,
                1 => Self::ImplementedTrait,
                2 => Self::Supertrait(<u32 as Wire>::decode(d)?),
                3 => Self::Requirement(<u32 as Wire>::decode(d)?),
                4 => Self::AssociatedDefinition(<u32 as Wire>::decode(d)?),
                5 => Self::MethodSignature(<u32 as Wire>::decode(d)?),
                6 => Self::MethodDefinition(<u32 as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynImplicitLifeOccurrence {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.root.encode(e)?;
            self.path.encode(e)?;
            self.kind.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                root: <CtDynImplicitLifeRoot as Wire>::decode(d)?,
                path: <Vec<u32> as Wire>::decode(d)?,
                kind: <CtDynImplicitLifeKind as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynImplicitLifeDeclaration {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.occurrences.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                occurrences: <Vec<CtDynImplicitLifeOccurrence> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynLifePattern {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::DeclarationParameter(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::MethodParameter(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Bound { depth, index } => {
                    2u8.encode(e)?;
                    depth.encode(e)?;
                    index.encode(e)?;
                }
                Self::Invocation(value_0) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::ImplicitDeclaration(value_0) => {
                    4u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::DeclarationParameter(<u32 as Wire>::decode(d)?),
                1 => Self::MethodParameter(<u32 as Wire>::decode(d)?),
                2 => Self::Bound {
                    depth: <u32 as Wire>::decode(d)?,
                    index: <u32 as Wire>::decode(d)?,
                },
                3 => Self::Invocation(<u32 as Wire>::decode(d)?),
                4 => Self::ImplicitDeclaration(<u32 as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynObligation {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Structural { receiver, bound } => {
                    0u8.encode(e)?;
                    receiver.encode(e)?;
                    bound.encode(e)?;
                }
                Self::Trait {
                    receiver,
                    application,
                } => {
                    1u8.encode(e)?;
                    receiver.encode(e)?;
                    application.encode(e)?;
                }
                Self::Callable {
                    receiver,
                    kind,
                    quantified_lifetimes,
                    signature,
                } => {
                    2u8.encode(e)?;
                    receiver.encode(e)?;
                    kind.encode(e)?;
                    quantified_lifetimes.encode(e)?;
                    signature.encode(e)?;
                }
                Self::TypeEqual { left, right } => {
                    3u8.encode(e)?;
                    left.encode(e)?;
                    right.encode(e)?;
                }
                Self::ConstEqual { ty, left, right } => {
                    4u8.encode(e)?;
                    ty.encode(e)?;
                    left.encode(e)?;
                    right.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Structural {
                    receiver: <CtDynTypePattern as Wire>::decode(d)?,
                    bound: <CtDynStructuralBound as Wire>::decode(d)?,
                },
                1 => Self::Trait {
                    receiver: <CtDynTypePattern as Wire>::decode(d)?,
                    application: <CtDynSelfPattern as Wire>::decode(d)?,
                },
                2 => Self::Callable {
                    receiver: <CtDynTypePattern as Wire>::decode(d)?,
                    kind: <CallableKind as Wire>::decode(d)?,
                    quantified_lifetimes: <u32 as Wire>::decode(d)?,
                    signature: <Box<CtDynSignatureRecipe> as Wire>::decode(d)?,
                },
                3 => Self::TypeEqual {
                    left: <CtDynTypePattern as Wire>::decode(d)?,
                    right: <CtDynTypePattern as Wire>::decode(d)?,
                },
                4 => Self::ConstEqual {
                    ty: <CtDynTypePattern as Wire>::decode(d)?,
                    left: <CtDynConstPattern as Wire>::decode(d)?,
                    right: <CtDynConstPattern as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynParameterRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.ty.encode(e)?;
            self.mode.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                ty: <CtDynTypePattern as Wire>::decode(d)?,
                mode: <PassingMode as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynSignatureRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.parameters.encode(e)?;
            self.result.encode(e)?;
            self.result_mode.encode(e)?;
            self.call_kind.encode(e)?;
            self.invocation_binder_count.encode(e)?;
            self.contract.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                parameters: <Vec<CtDynParameterRecipe> as Wire>::decode(d)?,
                result: <CtDynTypePattern as Wire>::decode(d)?,
                result_mode: <PassingMode as Wire>::decode(d)?,
                call_kind: <CallKind as Wire>::decode(d)?,
                invocation_binder_count: <u32 as Wire>::decode(d)?,
                contract: <CtDynContractRecipe as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynRequirementRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Read(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Write(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Manage(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Apart(value_0, value_1) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                    value_1.encode(e)?;
                }
                Self::State {
                    position,
                    ty,
                    variant,
                } => {
                    4u8.encode(e)?;
                    position.encode(e)?;
                    ty.encode(e)?;
                    variant.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Read(<StoragePosition as Wire>::decode(d)?),
                1 => Self::Write(<StoragePosition as Wire>::decode(d)?),
                2 => Self::Manage(<StoragePosition as Wire>::decode(d)?),
                3 => Self::Apart(
                    <StoragePosition as Wire>::decode(d)?,
                    <StoragePosition as Wire>::decode(d)?,
                ),
                4 => Self::State {
                    position: <StoragePosition as Wire>::decode(d)?,
                    ty: <CtDynTypePattern as Wire>::decode(d)?,
                    variant: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynGuaranteeRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::StorageSources(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::ManageResult => {
                    1u8.encode(e)?;
                }
                Self::State {
                    position,
                    ty,
                    variant,
                } => {
                    2u8.encode(e)?;
                    position.encode(e)?;
                    ty.encode(e)?;
                    variant.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::StorageSources(<CtDynStorageSourceBindingRecipe as Wire>::decode(d)?),
                1 => Self::ManageResult,
                2 => Self::State {
                    position: <StoragePosition as Wire>::decode(d)?,
                    ty: <CtDynTypePattern as Wire>::decode(d)?,
                    variant: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynEffectRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Alloc => {
                    0u8.encode(e)?;
                }
                Self::Io => {
                    1u8.encode(e)?;
                }
                Self::Block => {
                    2u8.encode(e)?;
                }
                Self::Suspend => {
                    3u8.encode(e)?;
                }
                Self::Spawn => {
                    4u8.encode(e)?;
                }
                Self::Ffi => {
                    5u8.encode(e)?;
                }
                Self::Reenter => {
                    6u8.encode(e)?;
                }
                Self::Panic => {
                    7u8.encode(e)?;
                }
                Self::Call(value_0) => {
                    8u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Cleanup(value_0) => {
                    9u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Alloc,
                1 => Self::Io,
                2 => Self::Block,
                3 => Self::Suspend,
                4 => Self::Spawn,
                5 => Self::Ffi,
                6 => Self::Reenter,
                7 => Self::Panic,
                8 => Self::Call(<StoragePosition as Wire>::decode(d)?),
                9 => Self::Cleanup(<CtDynTypePattern as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynCleanupRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.environment.encode(e)?;
            self.positions.encode(e)?;
            self.access.encode(e)?;
            self.effects.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                environment: <bool as Wire>::decode(d)?,
                positions: <Vec<StoragePosition> as Wire>::decode(d)?,
                access: <CtDynCallAccess as Wire>::decode(d)?,
                effects: <Vec<CtDynEffectRecipe> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynContractRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.requires.encode(e)?;
            self.guarantees.encode(e)?;
            self.hold.encode(e)?;
            self.call.encode(e)?;
            self.result.encode(e)?;
            self.cleanup.encode(e)?;
            self.effects.encode(e)?;
            self.resume.encode(e)?;
            self.threads.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                requires: <Option<Vec<CtDynRequirementRecipe>> as Wire>::decode(d)?,
                guarantees: <Option<Vec<CtDynGuaranteeRecipe>> as Wire>::decode(d)?,
                hold: <Option<Vec<StoragePosition>> as Wire>::decode(d)?,
                call: <Option<CtDynCallAccess> as Wire>::decode(d)?,
                result: <Option<CtDynResultSourceRecipe> as Wire>::decode(d)?,
                cleanup: <Option<CtDynCleanupRecipe> as Wire>::decode(d)?,
                effects: <Option<Vec<CtDynEffectRecipe>> as Wire>::decode(d)?,
                resume: <Option<CtDynResumeBound> as Wire>::decode(d)?,
                threads: <Option<CtDynThreadRequirements> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynStorageSourceRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Exact(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::PlaceAddress { place, mode } => {
                    1u8.encode(e)?;
                    place.encode(e)?;
                    mode.encode(e)?;
                }
                Self::OwnedView { head, leaf, mode } => {
                    2u8.encode(e)?;
                    head.encode(e)?;
                    leaf.encode(e)?;
                    mode.encode(e)?;
                }
                Self::ReborrowedLeaf { source, head } => {
                    3u8.encode(e)?;
                    source.encode(e)?;
                    head.encode(e)?;
                }
                Self::Static { leaf, mode } => {
                    4u8.encode(e)?;
                    leaf.encode(e)?;
                    mode.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Exact(<StoragePosition as Wire>::decode(d)?),
                1 => Self::PlaceAddress {
                    place: <StoragePosition as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                },
                2 => Self::OwnedView {
                    head: <StoragePosition as Wire>::decode(d)?,
                    leaf: <CtDynTypePattern as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                },
                3 => Self::ReborrowedLeaf {
                    source: <StoragePosition as Wire>::decode(d)?,
                    head: <StoragePosition as Wire>::decode(d)?,
                },
                4 => Self::Static {
                    leaf: <CtDynTypePattern as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynStorageSourceBindingRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.position.encode(e)?;
            self.sources.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                position: <StoragePosition as Wire>::decode(d)?,
                sources: <Vec<CtDynStorageSourceRecipe> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynResultSourceRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Value => {
                    0u8.encode(e)?;
                }
                Self::Create => {
                    1u8.encode(e)?;
                }
                Self::Give => {
                    2u8.encode(e)?;
                }
                Self::Borrow { sources, mutable } => {
                    3u8.encode(e)?;
                    sources.encode(e)?;
                    mutable.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Value,
                1 => Self::Create,
                2 => Self::Give,
                3 => Self::Borrow {
                    sources: <Vec<CtDynStorageSourceRecipe> as Wire>::decode(d)?,
                    mutable: <bool as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynDefinitionId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.module.encode(e)?;
            self.source.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                source: <NodeId as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynDefinitionScope {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::TraitDefault => {
                    0u8.encode(e)?;
                }
                Self::ImplOverride => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::TraitDefault,
                1 => Self::ImplOverride,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynDefinitionDeclaration {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.requirements.encode(e)?;
            self.instances.encode(e)?;
            self.id.encode(e)?;
            self.identity.encode(e)?;
            self.member.encode(e)?;
            self.scope.encode(e)?;
            self.signature.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                requirements: <Vec<CtDynObligation> as Wire>::decode(d)?,
                instances: <Vec<CtDynDefinitionSpecialization> as Wire>::decode(d)?,
                id: <CtDynDefinitionId as Wire>::decode(d)?,
                identity: <LifeDeclarationIdentity as Wire>::decode(d)?,
                member: <u32 as Wire>::decode(d)?,
                scope: <CtDynDefinitionScope as Wire>::decode(d)?,
                signature: <CtDynSignatureRecipe as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynMethodRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.requirements.encode(e)?;
            self.member.encode(e)?;
            self.signature.encode(e)?;
            self.default_definition.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                requirements: <Vec<CtDynObligation> as Wire>::decode(d)?,
                member: <u32 as Wire>::decode(d)?,
                signature: <CtDynSignatureRecipe as Wire>::decode(d)?,
                default_definition: <Option<CtDynDefinitionId> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynFunctionInstance {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.scope.encode(e)?;
            self.header_embedding.encode(e)?;
            self.definition.encode(e)?;
            self.receiver.encode(e)?;
            self.declaration_actuals.encode(e)?;
            self.method_actuals.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                scope: <CtDynInstantiationScope as Wire>::decode(d)?,
                header_embedding: <Vec<CtDynLifeEmbedding> as Wire>::decode(d)?,
                definition: <CtDynDefinitionId as Wire>::decode(d)?,
                receiver: <CtDynTypeValue as Wire>::decode(d)?,
                declaration_actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
                method_actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynAssociatedValueRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Constant { ty, value } => {
                    1u8.encode(e)?;
                    ty.encode(e)?;
                    value.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(<CtDynTypePattern as Wire>::decode(d)?),
                1 => Self::Constant {
                    ty: <CtDynTypePattern as Wire>::decode(d)?,
                    value: <CtDynConstPattern as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynAssociatedDefinition {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.carrier.encode(e)?;
            self.member.encode(e)?;
            self.value.encode(e)?;
            self.requirements.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                carrier: <Option<CtDynTypePattern> as Wire>::decode(d)?,
                member: <u32 as Wire>::decode(d)?,
                value: <Option<CtDynAssociatedValueRecipe> as Wire>::decode(d)?,
                requirements: <Vec<CtDynObligation> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynAssociatedSource {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::TraitDefault {
                    declaration,
                    member,
                } => {
                    0u8.encode(e)?;
                    declaration.encode(e)?;
                    member.encode(e)?;
                }
                Self::ImplOverride {
                    declaration,
                    member,
                } => {
                    1u8.encode(e)?;
                    declaration.encode(e)?;
                    member.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::TraitDefault {
                    declaration: <LifeDeclarationId as Wire>::decode(d)?,
                    member: <u32 as Wire>::decode(d)?,
                },
                1 => Self::ImplOverride {
                    declaration: <LifeDeclarationId as Wire>::decode(d)?,
                    member: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynAssociatedValue {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Constant { ty, value } => {
                    1u8.encode(e)?;
                    ty.encode(e)?;
                    value.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(<CtDynTypeValue as Wire>::decode(d)?),
                1 => Self::Constant {
                    ty: <CtDynTypeValue as Wire>::decode(d)?,
                    value: <Constant as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynAssociatedResolution {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.member.encode(e)?;
            self.source.encode(e)?;
            self.value.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                member: <CtDynMember as Wire>::decode(d)?,
                source: <CtDynAssociatedSource as Wire>::decode(d)?,
                value: <CtDynAssociatedValue as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynImplementationWitness {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.scope.encode(e)?;
            self.to_context.encode(e)?;
            self.receiver.encode(e)?;
            self.application.encode(e)?;
            self.implementation.encode(e)?;
            self.actuals.encode(e)?;
            self.associated.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                scope: <CtDynInstantiationScope as Wire>::decode(d)?,
                to_context: <Vec<CtDynLifeEmbedding> as Wire>::decode(d)?,
                receiver: <CtDynTypeValue as Wire>::decode(d)?,
                application: <CtDynTraitApplication as Wire>::decode(d)?,
                implementation: <LifeDeclarationId as Wire>::decode(d)?,
                actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
                associated: <Vec<CtDynAssociatedResolution> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynFunctionTarget {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Core { module, source } => {
                    0u8.encode(e)?;
                    module.encode(e)?;
                    source.encode(e)?;
                }
                Self::Typed(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Core {
                    module: <ModuleId as Wire>::decode(d)?,
                    source: <NodeId as Wire>::decode(d)?,
                },
                1 => Self::Typed(<FunctionId as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynDefinitionSpecialization {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.function.encode(e)?;
            self.receiver.encode(e)?;
            self.declaration_actuals.encode(e)?;
            self.method_actuals.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                function: <CtDynFunctionTarget as Wire>::decode(d)?,
                receiver: <CtDynTypeValue as Wire>::decode(d)?,
                declaration_actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
                method_actuals: <Vec<CtDynGenericActual> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynStructuralBound {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Copy => {
                    0u8.encode(e)?;
                }
                Self::Value => {
                    1u8.encode(e)?;
                }
                Self::SyncDrop => {
                    2u8.encode(e)?;
                }
                Self::StoreElement => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Copy,
                1 => Self::Value,
                2 => Self::SyncDrop,
                3 => Self::StoreElement,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticDefinitionId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.module.encode(e)?;
            self.index.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                index: <u32 as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticInstanceId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.module.encode(e)?;
            self.index.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                index: <u32 as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticModule {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.definitions.encode(e)?;
            self.instances.encode(e)?;
            self.sites.encode(e)?;
            self.dyn_scopes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                definitions: <Vec<CtSemanticDefinition> as Wire>::decode(d)?,
                instances: <Vec<CtSemanticInstance> as Wire>::decode(d)?,
                sites: <Vec<CtSemanticSite> as Wire>::decode(d)?,
                dyn_scopes: <Vec<CtSemanticDynScopeBinding> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticDynScopeBinding {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.owner.encode(e)?;
            self.scope.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                owner: <CtSemanticDynOwner as Wire>::decode(d)?,
                scope: <CtSemanticDefinitionId as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticDynOwner {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Declaration(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::MethodSignature {
                    declaration,
                    member,
                } => {
                    1u8.encode(e)?;
                    declaration.encode(e)?;
                    member.encode(e)?;
                }
                Self::MethodDefinition(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Declaration(<LifeDeclarationId as Wire>::decode(d)?),
                1 => Self::MethodSignature {
                    declaration: <LifeDeclarationId as Wire>::decode(d)?,
                    member: <u32 as Wire>::decode(d)?,
                },
                2 => Self::MethodDefinition(<CtDynDefinitionId as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticDefinition {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.id.encode(e)?;
            self.identity.encode(e)?;
            self.parent.encode(e)?;
            self.kind.encode(e)?;
            self.parameters.encode(e)?;
            self.implicit_lifetimes.encode(e)?;
            self.requirements.encode(e)?;
            self.environment.encode(e)?;
            self.body.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                id: <CtSemanticDefinitionId as Wire>::decode(d)?,
                identity: <LifeDeclarationIdentity as Wire>::decode(d)?,
                parent: <Option<CtSemanticDefinitionId> as Wire>::decode(d)?,
                kind: <CtSemanticDefinitionKind as Wire>::decode(d)?,
                parameters: <Vec<CtSemanticFormal> as Wire>::decode(d)?,
                implicit_lifetimes: <Vec<CtSemanticImplicitLifetime> as Wire>::decode(d)?,
                requirements: <Vec<CtSemanticRequirementRecipe> as Wire>::decode(d)?,
                environment: <Vec<CtSemanticEnvironmentFormal> as Wire>::decode(d)?,
                body: <CtSemanticDefinitionBody as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticDefinitionKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Function => {
                    0u8.encode(e)?;
                }
                Self::Method => {
                    1u8.encode(e)?;
                }
                Self::Closure => {
                    2u8.encode(e)?;
                }
                Self::TaskEntry => {
                    3u8.encode(e)?;
                }
                Self::Nominal => {
                    4u8.encode(e)?;
                }
                Self::Scope => {
                    5u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Function,
                1 => Self::Method,
                2 => Self::Closure,
                3 => Self::TaskEntry,
                4 => Self::Nominal,
                5 => Self::Scope,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticDefinitionBody {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Function { signature, sites } => {
                    0u8.encode(e)?;
                    signature.encode(e)?;
                    sites.encode(e)?;
                }
                Self::Nominal {
                    declaration,
                    kind,
                    body,
                } => {
                    1u8.encode(e)?;
                    declaration.encode(e)?;
                    kind.encode(e)?;
                    body.encode(e)?;
                }
                Self::Scope(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Function {
                    signature: <CtSemanticSignatureRecipe as Wire>::decode(d)?,
                    sites: <Vec<CtSemanticSiteRecipe> as Wire>::decode(d)?,
                },
                1 => Self::Nominal {
                    declaration: <LifeDeclarationId as Wire>::decode(d)?,
                    kind: <CtLifeNominalKind as Wire>::decode(d)?,
                    body: <CtSemanticNominalBody as Wire>::decode(d)?,
                },
                2 => Self::Scope(<CtSemanticScopeRecipe as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticScopeRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Trait { declaration } => {
                    0u8.encode(e)?;
                    declaration.encode(e)?;
                }
                Self::InherentImpl {
                    declaration,
                    self_type,
                } => {
                    1u8.encode(e)?;
                    declaration.encode(e)?;
                    self_type.encode(e)?;
                }
                Self::TraitImpl {
                    declaration,
                    self_type,
                    trait_application,
                } => {
                    2u8.encode(e)?;
                    declaration.encode(e)?;
                    self_type.encode(e)?;
                    trait_application.encode(e)?;
                }
                Self::BuiltinImpl {
                    self_type,
                    trait_kind,
                } => {
                    3u8.encode(e)?;
                    self_type.encode(e)?;
                    trait_kind.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Trait {
                    declaration: <LifeDeclarationId as Wire>::decode(d)?,
                },
                1 => Self::InherentImpl {
                    declaration: <LifeDeclarationId as Wire>::decode(d)?,
                    self_type: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                },
                2 => Self::TraitImpl {
                    declaration: <LifeDeclarationId as Wire>::decode(d)?,
                    self_type: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    trait_application: <CtSemanticTraitRecipe as Wire>::decode(d)?,
                },
                3 => Self::BuiltinImpl {
                    self_type: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    trait_kind: <CtSemanticBuiltinTrait as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticBuiltinTrait {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Drop => {
                    0u8.encode(e)?;
                }
                Self::Copy => {
                    1u8.encode(e)?;
                }
                Self::Value => {
                    2u8.encode(e)?;
                }
                Self::SyncDrop => {
                    3u8.encode(e)?;
                }
                Self::StoreElement => {
                    4u8.encode(e)?;
                }
                Self::Transfer => {
                    5u8.encode(e)?;
                }
                Self::Share => {
                    6u8.encode(e)?;
                }
                Self::ThreadCallable => {
                    7u8.encode(e)?;
                }
                Self::ClosedEnvironment => {
                    8u8.encode(e)?;
                }
                Self::Send => {
                    9u8.encode(e)?;
                }
                Self::Sync => {
                    10u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Drop,
                1 => Self::Copy,
                2 => Self::Value,
                3 => Self::SyncDrop,
                4 => Self::StoreElement,
                5 => Self::Transfer,
                6 => Self::Share,
                7 => Self::ThreadCallable,
                8 => Self::ClosedEnvironment,
                9 => Self::Send,
                10 => Self::Sync,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticNominalBody {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Product { fields } => {
                    0u8.encode(e)?;
                    fields.encode(e)?;
                }
                Self::Enum { variants } => {
                    1u8.encode(e)?;
                    variants.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Product {
                    fields: <Vec<CtSemanticFieldRecipe> as Wire>::decode(d)?,
                },
                1 => Self::Enum {
                    variants: <Vec<CtSemanticVariantRecipe> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticFieldRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.name.encode(e)?;
            self.ty.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                name: <String as Wire>::decode(d)?,
                ty: <CtSemanticTypeRecipe as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticVariantRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.name.encode(e)?;
            self.fields.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                name: <String as Wire>::decode(d)?,
                fields: <Vec<CtSemanticTypeRecipe> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticFormal {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type { requirements } => {
                    0u8.encode(e)?;
                    requirements.encode(e)?;
                }
                Self::Const { carrier } => {
                    1u8.encode(e)?;
                    carrier.encode(e)?;
                }
                Self::Lifetime => {
                    2u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type {
                    requirements: <Vec<CtSemanticRequirementRecipe> as Wire>::decode(d)?,
                },
                1 => Self::Const {
                    carrier: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                },
                2 => Self::Lifetime,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticImplicitLifetime {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.ordinal.encode(e)?;
            self.occurrence_path.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                ordinal: <u32 as Wire>::decode(d)?,
                occurrence_path: <Vec<u32> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticEnvironmentFormal {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.kind.encode(e)?;
            self.source.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                kind: <CtSemanticFormal as Wire>::decode(d)?,
                source: <CtSemanticActualRecipe as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticInstance {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.id.encode(e)?;
            self.definition.encode(e)?;
            self.target.encode(e)?;
            self.lifetime_instantiation.encode(e)?;
            self.parent.encode(e)?;
            self.actuals.encode(e)?;
            self.header_embedding.encode(e)?;
            self.environment.encode(e)?;
            self.self_type.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                id: <CtSemanticInstanceId as Wire>::decode(d)?,
                definition: <CtSemanticDefinitionId as Wire>::decode(d)?,
                target: <CtSemanticInstanceTarget as Wire>::decode(d)?,
                lifetime_instantiation: <CtSemanticLifetimeInstantiation as Wire>::decode(d)?,
                parent: <Option<CtSemanticInstanceId> as Wire>::decode(d)?,
                actuals: <Vec<CtSemanticActual> as Wire>::decode(d)?,
                header_embedding: <Vec<CtSemanticHeaderBinding> as Wire>::decode(d)?,
                environment: <Option<CtSemanticEnvironmentInstance> as Wire>::decode(d)?,
                self_type: <Option<CtSemanticTypeValue> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticLifetimeInstantiation {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Rigid => {
                    0u8.encode(e)?;
                }
                Self::Partition { rigid } => {
                    1u8.encode(e)?;
                    rigid.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Rigid,
                1 => Self::Partition {
                    rigid: <CtSemanticInstanceId as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticFunctionTarget {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Core { module, function } => {
                    0u8.encode(e)?;
                    module.encode(e)?;
                    function.encode(e)?;
                }
                Self::CoreClosure {
                    module,
                    creator,
                    expression,
                    occurrence,
                } => {
                    1u8.encode(e)?;
                    module.encode(e)?;
                    creator.encode(e)?;
                    expression.encode(e)?;
                    occurrence.encode(e)?;
                }
                Self::Typed(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Core {
                    module: <ModuleId as Wire>::decode(d)?,
                    function: <NodeId as Wire>::decode(d)?,
                },
                1 => Self::CoreClosure {
                    module: <ModuleId as Wire>::decode(d)?,
                    creator: <NodeId as Wire>::decode(d)?,
                    expression: <NodeId as Wire>::decode(d)?,
                    occurrence: <u32 as Wire>::decode(d)?,
                },
                2 => Self::Typed(<FunctionId as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticInstanceTarget {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Function(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Scope => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Function(<CtSemanticFunctionTarget as Wire>::decode(d)?),
                1 => Self::Scope,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticClosureTarget {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Core {
                    module,
                    creator,
                    expression,
                    occurrence,
                } => {
                    0u8.encode(e)?;
                    module.encode(e)?;
                    creator.encode(e)?;
                    expression.encode(e)?;
                    occurrence.encode(e)?;
                }
                Self::Typed(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Core {
                    module: <ModuleId as Wire>::decode(d)?,
                    creator: <NodeId as Wire>::decode(d)?,
                    expression: <NodeId as Wire>::decode(d)?,
                    occurrence: <u32 as Wire>::decode(d)?,
                },
                1 => Self::Typed(<ClosureId as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticHeaderBinding {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.header.encode(e)?;
            self.actual.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                header: <StorageBinder as Wire>::decode(d)?,
                actual: <CtSemanticClassExpr as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticEnvironmentInstance {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.closure.encode(e)?;
            self.actuals.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                closure: <CtSemanticClosureTarget as Wire>::decode(d)?,
                actuals: <Vec<CtSemanticActual> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticActual {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Const { ty, value } => {
                    1u8.encode(e)?;
                    ty.encode(e)?;
                    value.encode(e)?;
                }
                Self::Life(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::CoreRecipe { scope, recipe } => {
                    3u8.encode(e)?;
                    scope.encode(e)?;
                    recipe.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(<CtSemanticTypeValue as Wire>::decode(d)?),
                1 => Self::Const {
                    ty: <CtSemanticTypeValue as Wire>::decode(d)?,
                    value: <Constant as Wire>::decode(d)?,
                },
                2 => Self::Life(<CtSemanticClassExpr as Wire>::decode(d)?),
                3 => Self::CoreRecipe {
                    scope: <CtSemanticInstanceId as Wire>::decode(d)?,
                    recipe: <Box<CtSemanticActualRecipe> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticCoreTypeSource {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::OriginalRecipe(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Expression {
                    module,
                    function,
                    expression,
                    occurrence,
                    recipe,
                } => {
                    1u8.encode(e)?;
                    module.encode(e)?;
                    function.encode(e)?;
                    expression.encode(e)?;
                    occurrence.encode(e)?;
                    recipe.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::OriginalRecipe(<Box<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                1 => Self::Expression {
                    module: <ModuleId as Wire>::decode(d)?,
                    function: <NodeId as Wire>::decode(d)?,
                    expression: <NodeId as Wire>::decode(d)?,
                    occurrence: <u32 as Wire>::decode(d)?,
                    recipe: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticTypeValue {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Resolved { ty, life, classes } => {
                    0u8.encode(e)?;
                    ty.encode(e)?;
                    life.encode(e)?;
                    classes.encode(e)?;
                }
                Self::Core { scope, source } => {
                    1u8.encode(e)?;
                    scope.encode(e)?;
                    source.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Resolved {
                    ty: <CtDynamicTypeId as Wire>::decode(d)?,
                    life: <StorageLifeShape as Wire>::decode(d)?,
                    classes: <Vec<CtSemanticClassExpr> as Wire>::decode(d)?,
                },
                1 => Self::Core {
                    scope: <CtSemanticInstanceId as Wire>::decode(d)?,
                    source: <CtSemanticCoreTypeSource as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticScopeRef {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Ancestor(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Ancestor(<u32 as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticClassExpr {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Formal { scope, ordinal } => {
                    0u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
                Self::Implicit { scope, ordinal } => {
                    1u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
                Self::Bound { depth, index } => {
                    2u8.encode(e)?;
                    depth.encode(e)?;
                    index.encode(e)?;
                }
                Self::OuterOccurrence {
                    operand,
                    path,
                    mode,
                } => {
                    3u8.encode(e)?;
                    operand.encode(e)?;
                    path.encode(e)?;
                    mode.encode(e)?;
                }
                Self::IntrinsicClass {
                    operand,
                    path,
                    class,
                } => {
                    4u8.encode(e)?;
                    operand.encode(e)?;
                    path.encode(e)?;
                    class.encode(e)?;
                }
                Self::Environment { scope, ordinal } => {
                    5u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Formal {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                1 => Self::Implicit {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                2 => Self::Bound {
                    depth: <u32 as Wire>::decode(d)?,
                    index: <u32 as Wire>::decode(d)?,
                },
                3 => Self::OuterOccurrence {
                    operand: <CtSemanticOperand as Wire>::decode(d)?,
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                },
                4 => Self::IntrinsicClass {
                    operand: <CtSemanticOperand as Wire>::decode(d)?,
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                    class: <u32 as Wire>::decode(d)?,
                },
                5 => Self::Environment {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticOperand {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Parameter(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Capture(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Receiver => {
                    2u8.encode(e)?;
                }
                Self::Value(value_0) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Local(value_0) => {
                    4u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::TypeArgument { scope, ordinal } => {
                    5u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
                Self::Environment(value_0) => {
                    6u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::CoreExpression {
                    expression,
                    occurrence,
                } => {
                    7u8.encode(e)?;
                    expression.encode(e)?;
                    occurrence.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Parameter(<u32 as Wire>::decode(d)?),
                1 => Self::Capture(<u32 as Wire>::decode(d)?),
                2 => Self::Receiver,
                3 => Self::Value(<ValueId as Wire>::decode(d)?),
                4 => Self::Local(<LocalId as Wire>::decode(d)?),
                5 => Self::TypeArgument {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                6 => Self::Environment(<u32 as Wire>::decode(d)?),
                7 => Self::CoreExpression {
                    expression: <NodeId as Wire>::decode(d)?,
                    occurrence: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticSiteRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.id.encode(e)?;
            self.occurrence_path.encode(e)?;
            self.kind.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                id: <SemanticSiteId as Wire>::decode(d)?,
                occurrence_path: <Vec<u32> as Wire>::decode(d)?,
                kind: <CtSemanticSiteRecipeKind as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticSiteRecipeKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Construct { result } => {
                    0u8.encode(e)?;
                    result.encode(e)?;
                }
                Self::Call { target, actuals } => {
                    1u8.encode(e)?;
                    target.encode(e)?;
                    actuals.encode(e)?;
                }
                Self::CallableCall { callee } => {
                    2u8.encode(e)?;
                    callee.encode(e)?;
                }
                Self::TypeUse { expected } => {
                    3u8.encode(e)?;
                    expected.encode(e)?;
                }
                Self::DeclareLocal { expected } => {
                    4u8.encode(e)?;
                    expected.encode(e)?;
                }
                Self::Closure {
                    definition,
                    environment,
                } => {
                    5u8.encode(e)?;
                    definition.encode(e)?;
                    environment.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Construct {
                    result: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                },
                1 => Self::Call {
                    target: <CtSemanticDefinitionId as Wire>::decode(d)?,
                    actuals: <Vec<CtSemanticActualRecipe> as Wire>::decode(d)?,
                },
                2 => Self::CallableCall {
                    callee: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                },
                3 => Self::TypeUse {
                    expected: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                },
                4 => Self::DeclareLocal {
                    expected: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                },
                5 => Self::Closure {
                    definition: <CtSemanticDefinitionId as Wire>::decode(d)?,
                    environment: <Vec<CtSemanticActualRecipe> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticSite {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.instance.encode(e)?;
            self.recipe.encode(e)?;
            self.at.encode(e)?;
            self.action.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                instance: <CtSemanticInstanceId as Wire>::decode(d)?,
                recipe: <SemanticSiteId as Wire>::decode(d)?,
                at: <CtSemanticSiteAddress as Wire>::decode(d)?,
                action: <CtSemanticSiteAction as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticSiteAddress {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Core {
                    expression,
                    occurrence,
                    role,
                } => {
                    0u8.encode(e)?;
                    expression.encode(e)?;
                    occurrence.encode(e)?;
                    role.encode(e)?;
                }
                Self::Typed {
                    block,
                    instruction,
                    edge,
                } => {
                    1u8.encode(e)?;
                    block.encode(e)?;
                    instruction.encode(e)?;
                    edge.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Core {
                    expression: <NodeId as Wire>::decode(d)?,
                    occurrence: <u32 as Wire>::decode(d)?,
                    role: <CtSemanticEmissionRole as Wire>::decode(d)?,
                },
                1 => Self::Typed {
                    block: <BlockId as Wire>::decode(d)?,
                    instruction: <u32 as Wire>::decode(d)?,
                    edge: <CtSemanticSiteEdge as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticEmissionRole {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Construct => {
                    0u8.encode(e)?;
                }
                Self::Call => {
                    1u8.encode(e)?;
                }
                Self::Ascription => {
                    2u8.encode(e)?;
                }
                Self::Binding => {
                    3u8.encode(e)?;
                }
                Self::Closure => {
                    4u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Construct,
                1 => Self::Call,
                2 => Self::Ascription,
                3 => Self::Binding,
                4 => Self::Closure,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticSiteEdge {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Before => {
                    0u8.encode(e)?;
                }
                Self::After => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Before,
                1 => Self::After,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticSiteAction {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Construct { result, value } => {
                    0u8.encode(e)?;
                    result.encode(e)?;
                    value.encode(e)?;
                }
                Self::Call { target, actuals } => {
                    1u8.encode(e)?;
                    target.encode(e)?;
                    actuals.encode(e)?;
                }
                Self::CallableCall { callee } => {
                    2u8.encode(e)?;
                    callee.encode(e)?;
                }
                Self::TypeUse { subject, expected } => {
                    3u8.encode(e)?;
                    subject.encode(e)?;
                    expected.encode(e)?;
                }
                Self::DeclareLocal { local, expected } => {
                    4u8.encode(e)?;
                    local.encode(e)?;
                    expected.encode(e)?;
                }
                Self::Closure {
                    result,
                    closure,
                    environment,
                } => {
                    5u8.encode(e)?;
                    result.encode(e)?;
                    closure.encode(e)?;
                    environment.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Construct {
                    result: <CtSemanticOperand as Wire>::decode(d)?,
                    value: <CtSemanticTypeValue as Wire>::decode(d)?,
                },
                1 => Self::Call {
                    target: <CtSemanticFunctionTarget as Wire>::decode(d)?,
                    actuals: <Vec<CtSemanticActual> as Wire>::decode(d)?,
                },
                2 => Self::CallableCall {
                    callee: <CtSemanticOperand as Wire>::decode(d)?,
                },
                3 => Self::TypeUse {
                    subject: <CtSemanticOperand as Wire>::decode(d)?,
                    expected: <CtSemanticTypeValue as Wire>::decode(d)?,
                },
                4 => Self::DeclareLocal {
                    local: <CtSemanticLocalTarget as Wire>::decode(d)?,
                    expected: <CtSemanticTypeValue as Wire>::decode(d)?,
                },
                5 => Self::Closure {
                    result: <CtSemanticOperand as Wire>::decode(d)?,
                    closure: <CtSemanticClosureTarget as Wire>::decode(d)?,
                    environment: <Vec<CtSemanticActual> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticLocalTarget {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Core {
                    statement,
                    occurrence,
                } => {
                    0u8.encode(e)?;
                    statement.encode(e)?;
                    occurrence.encode(e)?;
                }
                Self::Typed(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Core {
                    statement: <NodeId as Wire>::decode(d)?,
                    occurrence: <u32 as Wire>::decode(d)?,
                },
                1 => Self::Typed(<LocalId as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticRecipeOperand {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::TypeFormal(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Environment { scope, ordinal } => {
                    1u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
                Self::Parameter(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Receiver => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::TypeFormal(<CtSemanticFormalRef as Wire>::decode(d)?),
                1 => Self::Environment {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                2 => Self::Parameter(<u32 as Wire>::decode(d)?),
                3 => Self::Receiver,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticFormalRef {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.scope.encode(e)?;
            self.ordinal.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                ordinal: <u32 as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticLifeRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Formal(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Implicit { scope, ordinal } => {
                    1u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
                Self::Environment { scope, ordinal } => {
                    2u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
                Self::Bound { depth, index } => {
                    3u8.encode(e)?;
                    depth.encode(e)?;
                    index.encode(e)?;
                }
                Self::Invocation(value_0) => {
                    4u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::OuterOccurrence {
                    operand,
                    path,
                    mode,
                } => {
                    5u8.encode(e)?;
                    operand.encode(e)?;
                    path.encode(e)?;
                    mode.encode(e)?;
                }
                Self::IntrinsicClass {
                    operand,
                    path,
                    class,
                } => {
                    6u8.encode(e)?;
                    operand.encode(e)?;
                    path.encode(e)?;
                    class.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Formal(<CtSemanticFormalRef as Wire>::decode(d)?),
                1 => Self::Implicit {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                2 => Self::Environment {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                3 => Self::Bound {
                    depth: <u32 as Wire>::decode(d)?,
                    index: <u32 as Wire>::decode(d)?,
                },
                4 => Self::Invocation(<u32 as Wire>::decode(d)?),
                5 => Self::OuterOccurrence {
                    operand: <CtSemanticRecipeOperand as Wire>::decode(d)?,
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                },
                6 => Self::IntrinsicClass {
                    operand: <CtSemanticRecipeOperand as Wire>::decode(d)?,
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                    class: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticOuterLifeRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Class(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Static => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Class(<CtSemanticLifeRecipe as Wire>::decode(d)?),
                1 => Self::Static,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticTypeRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Atom(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Exact(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Formal(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Environment { scope, ordinal } => {
                    3u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
                Self::SelfType { scope } => {
                    4u8.encode(e)?;
                    scope.encode(e)?;
                }
                Self::Nominal {
                    definition,
                    arguments,
                } => {
                    5u8.encode(e)?;
                    definition.encode(e)?;
                    arguments.encode(e)?;
                }
                Self::AssociatedType {
                    receiver,
                    application,
                    member,
                } => {
                    6u8.encode(e)?;
                    receiver.encode(e)?;
                    application.encode(e)?;
                    member.encode(e)?;
                }
                Self::Tuple(value_0) => {
                    7u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Array { element, length } => {
                    8u8.encode(e)?;
                    element.encode(e)?;
                    length.encode(e)?;
                }
                Self::Slice(value_0) => {
                    9u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Address {
                    mutable,
                    lifetime,
                    inner,
                } => {
                    10u8.encode(e)?;
                    mutable.encode(e)?;
                    lifetime.encode(e)?;
                    inner.encode(e)?;
                }
                Self::Desc { lifetime, inner } => {
                    11u8.encode(e)?;
                    lifetime.encode(e)?;
                    inner.encode(e)?;
                }
                Self::Managed(value_0) => {
                    12u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::OwnedStore(value_0) => {
                    13u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Mutex(value_0) => {
                    14u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::MutexGuard { payload, retained } => {
                    15u8.encode(e)?;
                    payload.encode(e)?;
                    retained.encode(e)?;
                }
                Self::Registration(value_0) => {
                    16u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::AsyncRegistration(value_0) => {
                    17u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Task { result, retained } => {
                    18u8.encode(e)?;
                    result.encode(e)?;
                    retained.encode(e)?;
                }
                Self::ErrorUnion { error, result } => {
                    19u8.encode(e)?;
                    error.encode(e)?;
                    result.encode(e)?;
                }
                Self::Qualified { storage, life } => {
                    20u8.encode(e)?;
                    storage.encode(e)?;
                    life.encode(e)?;
                }
                Self::Code { kind, signature } => {
                    21u8.encode(e)?;
                    kind.encode(e)?;
                    signature.encode(e)?;
                }
                Self::Closure {
                    definition,
                    arguments,
                    environment,
                } => {
                    22u8.encode(e)?;
                    definition.encode(e)?;
                    arguments.encode(e)?;
                    environment.encode(e)?;
                }
                Self::Dyn { owned, interface } => {
                    23u8.encode(e)?;
                    owned.encode(e)?;
                    interface.encode(e)?;
                }
                Self::Opaque {
                    definition,
                    arguments,
                    hidden,
                } => {
                    24u8.encode(e)?;
                    definition.encode(e)?;
                    arguments.encode(e)?;
                    hidden.encode(e)?;
                }
                Self::AnonymousSum(value_0) => {
                    25u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::UnresolvedNamed(value_0) => {
                    26u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Unsupported(value_0) => {
                    27u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Atom(<CtDynamicTypeId as Wire>::decode(d)?),
                1 => Self::Exact(<Box<CtSemanticExactTypeRecipe> as Wire>::decode(d)?),
                2 => Self::Formal(<CtSemanticFormalRef as Wire>::decode(d)?),
                3 => Self::Environment {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                4 => Self::SelfType {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                },
                5 => Self::Nominal {
                    definition: <CtSemanticDefinitionId as Wire>::decode(d)?,
                    arguments: <Vec<CtSemanticActualRecipe> as Wire>::decode(d)?,
                },
                6 => Self::AssociatedType {
                    receiver: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    application: <Box<CtSemanticTraitRecipe> as Wire>::decode(d)?,
                    member: <u32 as Wire>::decode(d)?,
                },
                7 => Self::Tuple(<Vec<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                8 => Self::Array {
                    element: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    length: <Box<CtSemanticConstRecipe> as Wire>::decode(d)?,
                },
                9 => Self::Slice(<Box<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                10 => Self::Address {
                    mutable: <bool as Wire>::decode(d)?,
                    lifetime: <CtSemanticOuterLifeRecipe as Wire>::decode(d)?,
                    inner: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                },
                11 => Self::Desc {
                    lifetime: <CtSemanticOuterLifeRecipe as Wire>::decode(d)?,
                    inner: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                },
                12 => Self::Managed(<Box<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                13 => Self::OwnedStore(<Box<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                14 => Self::Mutex(<Box<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                15 => Self::MutexGuard {
                    payload: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    retained: <Box<CtSemanticRetainedShapeRecipe> as Wire>::decode(d)?,
                },
                16 => Self::Registration(<Box<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                17 => Self::AsyncRegistration(<Box<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                18 => Self::Task {
                    result: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    retained: <Box<CtSemanticRetainedShapeRecipe> as Wire>::decode(d)?,
                },
                19 => Self::ErrorUnion {
                    error: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    result: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                },
                20 => Self::Qualified {
                    storage: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    life: <Box<CtSemanticStorageShapeRecipe> as Wire>::decode(d)?,
                },
                21 => Self::Code {
                    kind: <CtSemanticCodeKind as Wire>::decode(d)?,
                    signature: <Box<CtSemanticSignatureRecipe> as Wire>::decode(d)?,
                },
                22 => Self::Closure {
                    definition: <CtSemanticDefinitionId as Wire>::decode(d)?,
                    arguments: <Vec<CtSemanticActualRecipe> as Wire>::decode(d)?,
                    environment: <Vec<CtSemanticActualRecipe> as Wire>::decode(d)?,
                },
                23 => Self::Dyn {
                    owned: <bool as Wire>::decode(d)?,
                    interface: <Box<CtSemanticDynInterfaceRecipe> as Wire>::decode(d)?,
                },
                24 => Self::Opaque {
                    definition: <CtSemanticDefinitionId as Wire>::decode(d)?,
                    arguments: <Vec<CtSemanticActualRecipe> as Wire>::decode(d)?,
                    hidden: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                },
                25 => Self::AnonymousSum(<Vec<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                26 => Self::UnresolvedNamed(<String as Wire>::decode(d)?),
                27 => Self::Unsupported(<String as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticExactTypeRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.ty.encode(e)?;
            self.life.encode(e)?;
            self.classes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                ty: <CtDynamicTypeId as Wire>::decode(d)?,
                life: <StorageLifeShape as Wire>::decode(d)?,
                classes: <Vec<CtSemanticLifeRecipe> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticCodeKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::FunctionPointer => {
                    0u8.encode(e)?;
                }
                Self::BorrowedFunctionPointer => {
                    1u8.encode(e)?;
                }
                Self::ForeignCallback => {
                    2u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::FunctionPointer,
                1 => Self::BorrowedFunctionPointer,
                2 => Self::ForeignCallback,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticActualRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Const { carrier, value } => {
                    1u8.encode(e)?;
                    carrier.encode(e)?;
                    value.encode(e)?;
                }
                Self::Life(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(<CtSemanticTypeRecipe as Wire>::decode(d)?),
                1 => Self::Const {
                    carrier: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    value: <CtSemanticConstRecipe as Wire>::decode(d)?,
                },
                2 => Self::Life(<CtSemanticLifeRecipe as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticClassSequenceRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.parts.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                parts: <Vec<CtSemanticClassSequencePart> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticClassSequencePart {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::One(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::AllClasses { ty, path } => {
                    1u8.encode(e)?;
                    ty.encode(e)?;
                    path.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::One(<CtSemanticLifeRecipe as Wire>::decode(d)?),
                1 => Self::AllClasses {
                    ty: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticStorageShapeRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.domain.encode(e)?;
            self.slots.encode(e)?;
            self.retained.encode(e)?;
            self.nominal.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                domain: <CtSemanticClassSequenceRecipe as Wire>::decode(d)?,
                slots: <Vec<CtSemanticSlotRecipe> as Wire>::decode(d)?,
                retained: <Vec<CtSemanticRetainedPart> as Wire>::decode(d)?,
                nominal: <Vec<CtSemanticNominalPart> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticSlotRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.path.encode(e)?;
            self.mode.encode(e)?;
            self.class.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                path: <Vec<PlaceProjection> as Wire>::decode(d)?,
                mode: <StoredBorrowMode as Wire>::decode(d)?,
                class: <CtSemanticLifeRecipe as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticRetainedPart {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::One {
                    path,
                    child_class,
                    parent,
                } => {
                    0u8.encode(e)?;
                    path.encode(e)?;
                    child_class.encode(e)?;
                    parent.encode(e)?;
                }
                Self::All { path, parents } => {
                    1u8.encode(e)?;
                    path.encode(e)?;
                    parents.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::One {
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                    child_class: <u32 as Wire>::decode(d)?,
                    parent: <CtSemanticLifeRecipe as Wire>::decode(d)?,
                },
                1 => Self::All {
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                    parents: <CtSemanticClassSequenceRecipe as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticNominalPart {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::One {
                    path,
                    declaration,
                    argument,
                    class,
                } => {
                    0u8.encode(e)?;
                    path.encode(e)?;
                    declaration.encode(e)?;
                    argument.encode(e)?;
                    class.encode(e)?;
                }
                Self::FromType { path, ty, classes } => {
                    1u8.encode(e)?;
                    path.encode(e)?;
                    ty.encode(e)?;
                    classes.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::One {
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                    declaration: <LifeDeclarationId as Wire>::decode(d)?,
                    argument: <u32 as Wire>::decode(d)?,
                    class: <CtSemanticLifeRecipe as Wire>::decode(d)?,
                },
                1 => Self::FromType {
                    path: <Vec<LifeTypeProjection> as Wire>::decode(d)?,
                    ty: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    classes: <CtSemanticClassSequenceRecipe as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticRetainedShapeRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.domain.encode(e)?;
            self.ports.encode(e)?;
            self.retained.encode(e)?;
            self.nominal.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                domain: <CtSemanticClassSequenceRecipe as Wire>::decode(d)?,
                ports: <Vec<CtSemanticPortPart> as Wire>::decode(d)?,
                retained: <Vec<CtSemanticRetainedPart> as Wire>::decode(d)?,
                nominal: <Vec<CtSemanticNominalPart> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticPortPart {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::One { leaf, mode, class } => {
                    0u8.encode(e)?;
                    leaf.encode(e)?;
                    mode.encode(e)?;
                    class.encode(e)?;
                }
                Self::FromType { ty, classes } => {
                    1u8.encode(e)?;
                    ty.encode(e)?;
                    classes.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::One {
                    leaf: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                    class: <CtSemanticLifeRecipe as Wire>::decode(d)?,
                },
                1 => Self::FromType {
                    ty: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    classes: <CtSemanticClassSequenceRecipe as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticConstRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Value(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::IntegerLiteral(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::FloatLiteral(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Formal(value_0) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Environment { scope, ordinal } => {
                    4u8.encode(e)?;
                    scope.encode(e)?;
                    ordinal.encode(e)?;
                }
                Self::Unary {
                    op,
                    operand_type,
                    value,
                } => {
                    5u8.encode(e)?;
                    op.encode(e)?;
                    operand_type.encode(e)?;
                    value.encode(e)?;
                }
                Self::Binary {
                    op,
                    operand_type,
                    left,
                    right,
                } => {
                    6u8.encode(e)?;
                    op.encode(e)?;
                    operand_type.encode(e)?;
                    left.encode(e)?;
                    right.encode(e)?;
                }
                Self::Convert {
                    source_type,
                    target_type,
                    value,
                } => {
                    7u8.encode(e)?;
                    source_type.encode(e)?;
                    target_type.encode(e)?;
                    value.encode(e)?;
                }
                Self::AssociatedConst {
                    receiver,
                    application,
                    member,
                } => {
                    8u8.encode(e)?;
                    receiver.encode(e)?;
                    application.encode(e)?;
                    member.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Value(<Constant as Wire>::decode(d)?),
                1 => Self::IntegerLiteral(<String as Wire>::decode(d)?),
                2 => Self::FloatLiteral(<String as Wire>::decode(d)?),
                3 => Self::Formal(<CtSemanticFormalRef as Wire>::decode(d)?),
                4 => Self::Environment {
                    scope: <CtSemanticScopeRef as Wire>::decode(d)?,
                    ordinal: <u32 as Wire>::decode(d)?,
                },
                5 => Self::Unary {
                    op: <UnaryOp as Wire>::decode(d)?,
                    operand_type: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    value: <Box<CtSemanticConstRecipe> as Wire>::decode(d)?,
                },
                6 => Self::Binary {
                    op: <BinaryOp as Wire>::decode(d)?,
                    operand_type: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    left: <Box<CtSemanticConstRecipe> as Wire>::decode(d)?,
                    right: <Box<CtSemanticConstRecipe> as Wire>::decode(d)?,
                },
                7 => Self::Convert {
                    source_type: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    target_type: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    value: <Box<CtSemanticConstRecipe> as Wire>::decode(d)?,
                },
                8 => Self::AssociatedConst {
                    receiver: <Box<CtSemanticTypeRecipe> as Wire>::decode(d)?,
                    application: <Box<CtSemanticTraitRecipe> as Wire>::decode(d)?,
                    member: <u32 as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticTraitRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.declaration.encode(e)?;
            self.arguments.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                declaration: <LifeDeclarationId as Wire>::decode(d)?,
                arguments: <Vec<CtSemanticActualRecipe> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticRequirementRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Structural { ty, bound } => {
                    0u8.encode(e)?;
                    ty.encode(e)?;
                    bound.encode(e)?;
                }
                Self::Trait { ty, application } => {
                    1u8.encode(e)?;
                    ty.encode(e)?;
                    application.encode(e)?;
                }
                Self::Callable {
                    ty,
                    kind,
                    signature,
                } => {
                    2u8.encode(e)?;
                    ty.encode(e)?;
                    kind.encode(e)?;
                    signature.encode(e)?;
                }
                Self::TypeEqual { left, right } => {
                    3u8.encode(e)?;
                    left.encode(e)?;
                    right.encode(e)?;
                }
                Self::ConstEqual {
                    carrier,
                    left,
                    right,
                } => {
                    4u8.encode(e)?;
                    carrier.encode(e)?;
                    left.encode(e)?;
                    right.encode(e)?;
                }
                Self::LifeEqual { left, right } => {
                    5u8.encode(e)?;
                    left.encode(e)?;
                    right.encode(e)?;
                }
                Self::Outlives { longer, shorter } => {
                    6u8.encode(e)?;
                    longer.encode(e)?;
                    shorter.encode(e)?;
                }
                Self::ForAll {
                    lifetimes,
                    requirements,
                } => {
                    7u8.encode(e)?;
                    lifetimes.encode(e)?;
                    requirements.encode(e)?;
                }
                Self::Builtin { ty, trait_kind } => {
                    8u8.encode(e)?;
                    ty.encode(e)?;
                    trait_kind.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Structural {
                    ty: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    bound: <CtDynStructuralBound as Wire>::decode(d)?,
                },
                1 => Self::Trait {
                    ty: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    application: <CtSemanticTraitRecipe as Wire>::decode(d)?,
                },
                2 => Self::Callable {
                    ty: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    kind: <CallableKind as Wire>::decode(d)?,
                    signature: <Box<CtSemanticSignatureRecipe> as Wire>::decode(d)?,
                },
                3 => Self::TypeEqual {
                    left: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    right: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                },
                4 => Self::ConstEqual {
                    carrier: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    left: <CtSemanticConstRecipe as Wire>::decode(d)?,
                    right: <CtSemanticConstRecipe as Wire>::decode(d)?,
                },
                5 => Self::LifeEqual {
                    left: <CtSemanticLifeRecipe as Wire>::decode(d)?,
                    right: <CtSemanticLifeRecipe as Wire>::decode(d)?,
                },
                6 => Self::Outlives {
                    longer: <CtSemanticLifeRecipe as Wire>::decode(d)?,
                    shorter: <CtSemanticLifeRecipe as Wire>::decode(d)?,
                },
                7 => Self::ForAll {
                    lifetimes: <u32 as Wire>::decode(d)?,
                    requirements: <Vec<CtSemanticRequirementRecipe> as Wire>::decode(d)?,
                },
                8 => Self::Builtin {
                    ty: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    trait_kind: <CtSemanticBuiltinTrait as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticSignatureRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.bound_lifetimes.encode(e)?;
            self.invocation_lifetimes.encode(e)?;
            self.parameters.encode(e)?;
            self.result.encode(e)?;
            self.result_mode.encode(e)?;
            self.call_kind.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                bound_lifetimes: <u32 as Wire>::decode(d)?,
                invocation_lifetimes: <u32 as Wire>::decode(d)?,
                parameters: <Vec<CtSemanticParameterRecipe> as Wire>::decode(d)?,
                result: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                result_mode: <PassingMode as Wire>::decode(d)?,
                call_kind: <CallKind as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticParameterRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.ty.encode(e)?;
            self.mode.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                ty: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                mode: <PassingMode as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticDynContextRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Closed => {
                    0u8.encode(e)?;
                }
                Self::Type(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Closed,
                1 => Self::Type(<Box<CtSemanticTypeRecipe> as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtSemanticDynInterfaceRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.application.encode(e)?;
            self.context.encode(e)?;
            self.associated.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                application: <CtSemanticTraitRecipe as Wire>::decode(d)?,
                context: <CtSemanticDynContextRecipe as Wire>::decode(d)?,
                associated: <Vec<CtSemanticAssociatedBindingRecipe> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticAssociatedBindingRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.application.encode(e)?;
            self.member.encode(e)?;
            self.value.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                application: <CtSemanticTraitRecipe as Wire>::decode(d)?,
                member: <u32 as Wire>::decode(d)?,
                value: <CtSemanticAssociatedRecipe as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtSemanticAssociatedRecipe {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Const { carrier, value } => {
                    1u8.encode(e)?;
                    carrier.encode(e)?;
                    value.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(<CtSemanticTypeRecipe as Wire>::decode(d)?),
                1 => Self::Const {
                    carrier: <CtSemanticTypeRecipe as Wire>::decode(d)?,
                    value: <CtSemanticConstRecipe as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtStorageSource {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Exact(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::PlaceAddress { place, mode } => {
                    1u8.encode(e)?;
                    place.encode(e)?;
                    mode.encode(e)?;
                }
                Self::OwnedView { head, leaf, mode } => {
                    2u8.encode(e)?;
                    head.encode(e)?;
                    leaf.encode(e)?;
                    mode.encode(e)?;
                }
                Self::ReborrowedLeaf { source, head } => {
                    3u8.encode(e)?;
                    source.encode(e)?;
                    head.encode(e)?;
                }
                Self::Static { leaf, mode } => {
                    4u8.encode(e)?;
                    leaf.encode(e)?;
                    mode.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Exact(<StoragePosition as Wire>::decode(d)?),
                1 => Self::PlaceAddress {
                    place: <StoragePosition as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                },
                2 => Self::OwnedView {
                    head: <StoragePosition as Wire>::decode(d)?,
                    leaf: <CtDynamicTypeId as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                },
                3 => Self::ReborrowedLeaf {
                    source: <StoragePosition as Wire>::decode(d)?,
                    head: <StoragePosition as Wire>::decode(d)?,
                },
                4 => Self::Static {
                    leaf: <CtDynamicTypeId as Wire>::decode(d)?,
                    mode: <StoredBorrowMode as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtRetainedBorrowPort {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.leaf.encode(e)?;
            self.mode.encode(e)?;
            self.binder.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                leaf: <CtDynamicTypeId as Wire>::decode(d)?,
                mode: <StoredBorrowMode as Wire>::decode(d)?,
                binder: <u32 as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtRetainedLifeShape {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.binder_count.encode(e)?;
            self.ports.encode(e)?;
            self.retained.encode(e)?;
            self.nominal_arguments.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                binder_count: <u32 as Wire>::decode(d)?,
                ports: <Vec<CtRetainedBorrowPort> as Wire>::decode(d)?,
                retained: <Vec<StorageRetainedBinding> as Wire>::decode(d)?,
                nominal_arguments: <Vec<NominalLifeArgument> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtStorageSourceBinding {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.position.encode(e)?;
            self.sources.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                position: <StoragePosition as Wire>::decode(d)?,
                sources: <Vec<CtStorageSource> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtLifeNominalKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Record => {
                    0u8.encode(e)?;
                }
                Self::Package => {
                    1u8.encode(e)?;
                }
                Self::Resource => {
                    2u8.encode(e)?;
                }
                Self::Enum => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Record,
                1 => Self::Package,
                2 => Self::Resource,
                3 => Self::Enum,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtLifeParameterKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type => {
                    0u8.encode(e)?;
                }
                Self::Const(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Lifetime => {
                    2u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type,
                1 => Self::Const(<CtDynamicTypeId as Wire>::decode(d)?),
                2 => Self::Lifetime,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtLifeNominalDeclaration {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.id.encode(e)?;
            self.identity.encode(e)?;
            self.kind.encode(e)?;
            self.parameters.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                id: <LifeDeclarationId as Wire>::decode(d)?,
                identity: <LifeDeclarationIdentity as Wire>::decode(d)?,
                kind: <CtLifeNominalKind as Wire>::decode(d)?,
                parameters: <Vec<CtLifeParameterKind> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtLifeNominalActual {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Type(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::QualifiedType { ty, life } => {
                    1u8.encode(e)?;
                    ty.encode(e)?;
                    life.encode(e)?;
                }
                Self::Const(value_0) => {
                    2u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Lifetime(value_0) => {
                    3u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(<CtDynamicTypeId as Wire>::decode(d)?),
                1 => Self::QualifiedType {
                    ty: <CtDynamicTypeId as Wire>::decode(d)?,
                    life: <StorageLifeShape as Wire>::decode(d)?,
                },
                2 => Self::Const(<Constant as Wire>::decode(d)?),
                3 => Self::Lifetime(<u32 as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtLifeNominalInstance {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.declaration.encode(e)?;
            self.ty.encode(e)?;
            self.arguments.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                declaration: <LifeDeclarationId as Wire>::decode(d)?,
                ty: <CtDynamicTypeId as Wire>::decode(d)?,
                arguments: <Vec<CtLifeNominalActual> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynamicTypeNode {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Dyn { interface } => {
                    0u8.encode(e)?;
                    interface.encode(e)?;
                }
                Self::OwnedDyn { interface } => {
                    1u8.encode(e)?;
                    interface.encode(e)?;
                }
                Self::BorrowedStorage { storage, life } => {
                    2u8.encode(e)?;
                    storage.encode(e)?;
                    life.encode(e)?;
                }
                Self::Unit => {
                    3u8.encode(e)?;
                }
                Self::Bool => {
                    4u8.encode(e)?;
                }
                Self::I32 => {
                    5u8.encode(e)?;
                }
                Self::I64 => {
                    6u8.encode(e)?;
                }
                Self::U64 => {
                    7u8.encode(e)?;
                }
                Self::Usize => {
                    8u8.encode(e)?;
                }
                Self::F32 => {
                    9u8.encode(e)?;
                }
                Self::F64 => {
                    10u8.encode(e)?;
                }
                Self::U8 => {
                    11u8.encode(e)?;
                }
                Self::U16 => {
                    12u8.encode(e)?;
                }
                Self::U32 => {
                    13u8.encode(e)?;
                }
                Self::I8 => {
                    14u8.encode(e)?;
                }
                Self::I16 => {
                    15u8.encode(e)?;
                }
                Self::Isize => {
                    16u8.encode(e)?;
                }
                Self::ByteBuffer => {
                    17u8.encode(e)?;
                }
                Self::OwnedStore { element } => {
                    18u8.encode(e)?;
                    element.encode(e)?;
                }
                Self::ForeignHandle => {
                    19u8.encode(e)?;
                }
                Self::SyncCell(value_0) => {
                    20u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Mutex { payload } => {
                    21u8.encode(e)?;
                    payload.encode(e)?;
                }
                Self::MutexGuard {
                    payload,
                    retained_life,
                } => {
                    22u8.encode(e)?;
                    payload.encode(e)?;
                    retained_life.encode(e)?;
                }
                Self::Registration { environment } => {
                    23u8.encode(e)?;
                    environment.encode(e)?;
                }
                Self::AsyncRegistration { environment } => {
                    24u8.encode(e)?;
                    environment.encode(e)?;
                }
                Self::TaskI32 => {
                    25u8.encode(e)?;
                }
                Self::Task {
                    result,
                    retained_life,
                } => {
                    26u8.encode(e)?;
                    result.encode(e)?;
                    retained_life.encode(e)?;
                }
                Self::CWideString => {
                    27u8.encode(e)?;
                }
                Self::ForeignCallback { parameters, result } => {
                    28u8.encode(e)?;
                    parameters.encode(e)?;
                    result.encode(e)?;
                }
                Self::Tuple(value_0) => {
                    29u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Array { element, length } => {
                    30u8.encode(e)?;
                    element.encode(e)?;
                    length.encode(e)?;
                }
                Self::Slice { element } => {
                    31u8.encode(e)?;
                    element.encode(e)?;
                }
                Self::Named(value_0) => {
                    32u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Record { name, fields } => {
                    33u8.encode(e)?;
                    name.encode(e)?;
                    fields.encode(e)?;
                }
                Self::Package { name, fields } => {
                    34u8.encode(e)?;
                    name.encode(e)?;
                    fields.encode(e)?;
                }
                Self::Enum { name, variants } => {
                    35u8.encode(e)?;
                    name.encode(e)?;
                    variants.encode(e)?;
                }
                Self::ErrorUnion { error, result } => {
                    36u8.encode(e)?;
                    error.encode(e)?;
                    result.encode(e)?;
                }
                Self::Address { mutable, inner } => {
                    37u8.encode(e)?;
                    mutable.encode(e)?;
                    inner.encode(e)?;
                }
                Self::Managed(value_0) => {
                    38u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Desc(value_0) => {
                    39u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::FunctionPointer { parameters, result } => {
                    40u8.encode(e)?;
                    parameters.encode(e)?;
                    result.encode(e)?;
                }
                Self::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                } => {
                    41u8.encode(e)?;
                    parameters.encode(e)?;
                    result.encode(e)?;
                    life.encode(e)?;
                }
                Self::Closure(value_0) => {
                    42u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Opaque { identity, hidden } => {
                    43u8.encode(e)?;
                    identity.encode(e)?;
                    hidden.encode(e)?;
                }
                Self::Unsupported(value_0) => {
                    44u8.encode(e)?;
                    value_0.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Dyn {
                    interface: <CtDynInterfaceId as Wire>::decode(d)?,
                },
                1 => Self::OwnedDyn {
                    interface: <CtDynInterfaceId as Wire>::decode(d)?,
                },
                2 => Self::BorrowedStorage {
                    storage: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                    life: <StorageLifeShape as Wire>::decode(d)?,
                },
                3 => Self::Unit,
                4 => Self::Bool,
                5 => Self::I32,
                6 => Self::I64,
                7 => Self::U64,
                8 => Self::Usize,
                9 => Self::F32,
                10 => Self::F64,
                11 => Self::U8,
                12 => Self::U16,
                13 => Self::U32,
                14 => Self::I8,
                15 => Self::I16,
                16 => Self::Isize,
                17 => Self::ByteBuffer,
                18 => Self::OwnedStore {
                    element: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                19 => Self::ForeignHandle,
                20 => Self::SyncCell(<SyncCellKind as Wire>::decode(d)?),
                21 => Self::Mutex {
                    payload: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                22 => Self::MutexGuard {
                    payload: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                    retained_life: <CtRetainedLifeShape as Wire>::decode(d)?,
                },
                23 => Self::Registration {
                    environment: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                24 => Self::AsyncRegistration {
                    environment: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                25 => Self::TaskI32,
                26 => Self::Task {
                    result: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                    retained_life: <CtRetainedLifeShape as Wire>::decode(d)?,
                },
                27 => Self::CWideString,
                28 => Self::ForeignCallback {
                    parameters: <Vec<CtDynamicTypeId> as Wire>::decode(d)?,
                    result: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                29 => Self::Tuple(<Vec<CtDynamicTypeId> as Wire>::decode(d)?),
                30 => Self::Array {
                    element: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                    length: <u32 as Wire>::decode(d)?,
                },
                31 => Self::Slice {
                    element: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                32 => Self::Named(<String as Wire>::decode(d)?),
                33 => Self::Record {
                    name: <String as Wire>::decode(d)?,
                    fields: <Vec<(String, CtDynamicTypeId)> as Wire>::decode(d)?,
                },
                34 => Self::Package {
                    name: <String as Wire>::decode(d)?,
                    fields: <Vec<(String, CtDynamicTypeId)> as Wire>::decode(d)?,
                },
                35 => Self::Enum {
                    name: <String as Wire>::decode(d)?,
                    variants: <Vec<(String, Vec<CtDynamicTypeId>)> as Wire>::decode(d)?,
                },
                36 => Self::ErrorUnion {
                    error: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                    result: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                37 => Self::Address {
                    mutable: <bool as Wire>::decode(d)?,
                    inner: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                38 => Self::Managed(<Box<CtDynamicTypeId> as Wire>::decode(d)?),
                39 => Self::Desc(<Box<CtDynamicTypeId> as Wire>::decode(d)?),
                40 => Self::FunctionPointer {
                    parameters: <Vec<CtDynamicTypeId> as Wire>::decode(d)?,
                    result: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                41 => Self::BorrowedFunctionPointer {
                    parameters: <Vec<CtDynamicTypeId> as Wire>::decode(d)?,
                    result: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                    life: <StorageLifeContract as Wire>::decode(d)?,
                },
                42 => Self::Closure(<ClosureId as Wire>::decode(d)?),
                43 => Self::Opaque {
                    identity: <NodeId as Wire>::decode(d)?,
                    hidden: <Box<CtDynamicTypeId> as Wire>::decode(d)?,
                },
                44 => Self::Unsupported(<String as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CallKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Direct => {
                    0u8.encode(e)?;
                }
                Self::Suspend => {
                    1u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Direct,
                1 => Self::Suspend,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for LifeDeclarationIdentity {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.package.encode(e)?;
            self.module_path.encode(e)?;
            self.declaration_path.encode(e)?;
            self.generation_path.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                package: <String as Wire>::decode(d)?,
                module_path: <Vec<String> as Wire>::decode(d)?,
                declaration_path: <Vec<String> as Wire>::decode(d)?,
                generation_path: <Vec<u32> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for PassingMode {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Copy => {
                    0u8.encode(e)?;
                }
                Self::Move => {
                    1u8.encode(e)?;
                }
                Self::Describe => {
                    2u8.encode(e)?;
                }
                Self::Create => {
                    3u8.encode(e)?;
                }
                Self::Give => {
                    4u8.encode(e)?;
                }
                Self::Borrow => {
                    5u8.encode(e)?;
                }
                Self::BorrowMut => {
                    6u8.encode(e)?;
                }
                Self::Infer => {
                    7u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Copy,
                1 => Self::Move,
                2 => Self::Describe,
                3 => Self::Create,
                4 => Self::Give,
                5 => Self::Borrow,
                6 => Self::BorrowMut,
                7 => Self::Infer,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CallableKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::FunctionPointer => {
                    0u8.encode(e)?;
                }
                Self::Fn => {
                    1u8.encode(e)?;
                }
                Self::FnMut => {
                    2u8.encode(e)?;
                }
                Self::FnOnce => {
                    3u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::FunctionPointer,
                1 => Self::Fn,
                2 => Self::FnMut,
                3 => Self::FnOnce,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtStageIdentity {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.source_fingerprint.encode(e)?;
            self.epoch.encode(e)?;
            self.committed_generation_fingerprint.encode(e)?;
            self.declaration_inventory_fingerprint.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                source_fingerprint: <Fingerprint as Wire>::decode(d)?,
                epoch: <u64 as Wire>::decode(d)?,
                committed_generation_fingerprint: <Fingerprint as Wire>::decode(d)?,
                declaration_inventory_fingerprint: <Fingerprint as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtStageFacet {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Shape => {
                    0u8.encode(e)?;
                }
                Self::Contract => {
                    1u8.encode(e)?;
                }
                Self::Layout => {
                    2u8.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Shape,
                1 => Self::Contract,
                2 => Self::Layout,
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtStageSubject {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            match self {
                Self::Declaration(value_0) => {
                    0u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Type(value_0) => {
                    1u8.encode(e)?;
                    value_0.encode(e)?;
                }
                Self::Instance {
                    declaration,
                    context,
                    arguments,
                } => {
                    2u8.encode(e)?;
                    declaration.encode(e)?;
                    context.encode(e)?;
                    arguments.encode(e)?;
                }
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Declaration(<CtSymbolId as Wire>::decode(d)?),
                1 => Self::Type(<CtTypeRef as Wire>::decode(d)?),
                2 => Self::Instance {
                    declaration: <CtSymbolId as Wire>::decode(d)?,
                    context: <CtTypeRef as Wire>::decode(d)?,
                    arguments: <Vec<CtTypeArgument> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtBodySourceRef {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.declaration.encode(e)?;
            self.body_fingerprint.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                declaration: <CtSymbolId as Wire>::decode(d)?,
                body_fingerprint: <Fingerprint as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtStageFacetRequest {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.subject.encode(e)?;
            self.facet.encode(e)?;
            self.content_fingerprint.encode(e)?;
            self.body_source.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                subject: <CtStageSubject as Wire>::decode(d)?,
                facet: <CtStageFacet as Wire>::decode(d)?,
                content_fingerprint: <Option<Fingerprint> as Wire>::decode(d)?,
                body_source: <Option<CtBodySourceRef> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtTypeCatalogue {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.stage.encode(e)?;
            self.types.encode(e)?;
            self.interfaces.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                stage: <CtStageIdentity as Wire>::decode(d)?,
                types: <Vec<CtDynamicTypeNode> as Wire>::decode(d)?,
                interfaces: <Vec<CtDynInterface> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynamicCatalogue {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.semantic.encode(e)?;
            self.stage.encode(e)?;
            self.facet_requests.encode(e)?;
            self.types.encode(e)?;
            self.life_declarations.encode(e)?;
            self.life_instances.encode(e)?;
            self.declarations.encode(e)?;
            self.interfaces.encode(e)?;
            self.vtables.encode(e)?;
            self.function_instances.encode(e)?;
            self.function_sources.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                semantic: <Option<CtSemanticModule> as Wire>::decode(d)?,
                stage: <Option<CtStageIdentity> as Wire>::decode(d)?,
                facet_requests: <Vec<CtStageFacetRequest> as Wire>::decode(d)?,
                types: <Vec<CtDynamicTypeNode> as Wire>::decode(d)?,
                life_declarations: <Vec<CtLifeNominalDeclaration> as Wire>::decode(d)?,
                life_instances: <Vec<CtLifeNominalInstance> as Wire>::decode(d)?,
                declarations: <Vec<CtDynDeclaration> as Wire>::decode(d)?,
                interfaces: <Vec<CtDynInterface> as Wire>::decode(d)?,
                vtables: <Vec<CtCoreDynVtable> as Wire>::decode(d)?,
                function_instances: <Vec<CtDynFunctionInstanceRecord> as Wire>::decode(d)?,
                function_sources: <Vec<CtFunctionSourceBindings> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynFunctionInstanceRecord {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.module.encode(e)?;
            self.source.encode(e)?;
            self.instance.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                source: <NodeId as Wire>::decode(d)?,
                instance: <CtDynFunctionInstance as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtFunctionSourceBindings {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        if e.depth >= 128 {
            return Err(WireError::Limit);
        }
        e.descend(|e| {
            self.module.encode(e)?;
            self.source.encode(e)?;
            self.retained_return_sources.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.depth >= 128 {
            return Err(WireError::Limit);
        }
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                source: <NodeId as Wire>::decode(d)?,
                retained_return_sources: <Option<Vec<CtStorageSourceBinding>> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynamicTypeId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}
impl Wire for ClosureId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}
impl Wire for FunctionId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}
impl Wire for ValueId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}
impl Wire for LocalId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}
impl Wire for BlockId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}
impl Wire for SemanticSiteId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.0.encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self(u32::decode(d)?))
    }
}

impl<A: Wire, B: Wire> Wire for (A, B) {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.0.encode(e)?;
            self.1.encode(e)
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| Ok((A::decode(d)?, B::decode(d)?)))
    }
}
impl Wire for SyncCellKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        (*self as u8).encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        match u8::decode(d)? {
            0 => Ok(Self::MutexI32),
            1 => Ok(Self::AtomicI32),
            2 => Ok(Self::AtomicI64),
            3 => Ok(Self::AtomicU64),
            4 => Ok(Self::AtomicUsize),
            _ => Err(WireError::UnknownTag),
        }
    }
}

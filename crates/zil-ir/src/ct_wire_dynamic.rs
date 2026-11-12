//! Generated bounded transport for the complete CT dynamic catalogue.
use super::*;
use crate::*;
impl Wire for CtDynInterfaceId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.module.encode(e)?;
            self.index.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.module.encode(e)?;
            self.index.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
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
        d.descend(|d| {
            Ok(Self {
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
        e.descend(|e| {
            self.declaration.encode(e)?;
            self.arguments.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
                Self::Desc(value_0) => {
                    11u8.encode(e)?;
                    value_0.encode(e)?;
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
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
                11 => Self::Desc(<Box<CtDynTypePattern> as Wire>::decode(d)?),
                12 => Self::OwnedStore(<Box<CtDynTypePattern> as Wire>::decode(d)?),
                13 => Self::ErrorUnion {
                    error: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                    result: <Box<CtDynTypePattern> as Wire>::decode(d)?,
                },
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynDeclarationKind {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
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
        e.descend(|e| {
            self.name.encode(e)?;
            self.kind.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.declaration.encode(e)?;
            self.actuals.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.application.encode(e)?;
            self.ordinal.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.member.encode(e)?;
            self.ty.encode(e)?;
            self.life.encode(e)?;
            self.classes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.member.encode(e)?;
            self.ty.encode(e)?;
            self.value.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.mutable.encode(e)?;
            self.mode.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.slot.encode(e)?;
            self.module.encode(e)?;
            self.function.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.slot.encode(e)?;
            self.function.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.port.encode(e)?;
            self.sources.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.reads.encode(e)?;
            self.writes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.environment.encode(e)?;
            self.positions.encode(e)?;
            self.access.encode(e)?;
            self.effects.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.transfer.encode(e)?;
            self.share.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.ty.encode(e)?;
            self.life.encode(e)?;
            self.classes.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                ty: <CtDynamicTypeId as Wire>::decode(d)?,
                life: <StorageLifeShape as Wire>::decode(d)?,
                classes: <Vec<StorageBinder> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynLifePattern {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
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
            }
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::DeclarationParameter(<u32 as Wire>::decode(d)?),
                1 => Self::MethodParameter(<u32 as Wire>::decode(d)?),
                2 => Self::Bound {
                    depth: <u32 as Wire>::decode(d)?,
                    index: <u32 as Wire>::decode(d)?,
                },
                3 => Self::Invocation(<u32 as Wire>::decode(d)?),
                _ => return Err(WireError::UnknownTag),
            })
        })
    }
}
impl Wire for CtDynObligation {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
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
        e.descend(|e| {
            self.ty.encode(e)?;
            self.mode.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::StorageSources(<CtStorageSourceBinding as Wire>::decode(d)?),
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
        e.descend(|e| {
            self.environment.encode(e)?;
            self.positions.encode(e)?;
            self.access.encode(e)?;
            self.effects.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        d.descend(|d| {
            Ok(Self {
                requires: <Option<Vec<CtDynRequirementRecipe>> as Wire>::decode(d)?,
                guarantees: <Option<Vec<CtDynGuaranteeRecipe>> as Wire>::decode(d)?,
                hold: <Option<Vec<StoragePosition>> as Wire>::decode(d)?,
                call: <Option<CtDynCallAccess> as Wire>::decode(d)?,
                result: <Option<CtDynResultSource> as Wire>::decode(d)?,
                cleanup: <Option<CtDynCleanupRecipe> as Wire>::decode(d)?,
                effects: <Option<Vec<CtDynEffectRecipe>> as Wire>::decode(d)?,
                resume: <Option<CtDynResumeBound> as Wire>::decode(d)?,
                threads: <Option<CtDynThreadRequirements> as Wire>::decode(d)?,
            })
        })
    }
}
impl Wire for CtDynDefinitionId {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            self.module.encode(e)?;
            self.source.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.requirements.encode(e)?;
            self.member.encode(e)?;
            self.signature.encode(e)?;
            self.default_definition.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.definition.encode(e)?;
            self.receiver.encode(e)?;
            self.declaration_actuals.encode(e)?;
            self.method_actuals.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
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
        e.descend(|e| {
            self.carrier.encode(e)?;
            self.member.encode(e)?;
            self.value.encode(e)?;
            self.requirements.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.member.encode(e)?;
            self.source.encode(e)?;
            self.value.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.receiver.encode(e)?;
            self.application.encode(e)?;
            self.implementation.encode(e)?;
            self.actuals.encode(e)?;
            self.associated.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
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
        e.descend(|e| {
            self.function.encode(e)?;
            self.receiver.encode(e)?;
            self.declaration_actuals.encode(e)?;
            self.method_actuals.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
impl Wire for CtStorageSource {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
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
        e.descend(|e| {
            self.leaf.encode(e)?;
            self.mode.encode(e)?;
            self.binder.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.binder_count.encode(e)?;
            self.ports.encode(e)?;
            self.retained.encode(e)?;
            self.nominal_arguments.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.position.encode(e)?;
            self.sources.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.id.encode(e)?;
            self.identity.encode(e)?;
            self.kind.encode(e)?;
            self.parameters.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.declaration.encode(e)?;
            self.ty.encode(e)?;
            self.arguments.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.package.encode(e)?;
            self.module_path.encode(e)?;
            self.declaration_path.encode(e)?;
            self.generation_path.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
impl Wire for CtDynamicCatalogue {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
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
        d.descend(|d| {
            Ok(Self {
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
        e.descend(|e| {
            self.module.encode(e)?;
            self.source.encode(e)?;
            self.instance.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
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
        e.descend(|e| {
            self.module.encode(e)?;
            self.source.encode(e)?;
            self.retained_return_sources.encode(e)?;
            self.returned_capture_sources.encode(e)?;
            Ok(())
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(Self {
                module: <ModuleId as Wire>::decode(d)?,
                source: <NodeId as Wire>::decode(d)?,
                retained_return_sources: <Option<Vec<CtStorageSourceBinding>> as Wire>::decode(d)?,
                returned_capture_sources: <Option<Vec<CtStorageSourceBinding>> as Wire>::decode(d)?,
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

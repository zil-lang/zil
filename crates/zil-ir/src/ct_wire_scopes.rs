//! Bounded transport of complete callable relations and ambient scope claims.
//! Source authentication and independent semantic checks remain consumers.
use super::*;
use crate::{
    ContractProjection, SignatureLifeClass, SignatureLifeClassBinding, StorageBase,
    StorageLifeBinding, StoragePosition,
};

macro_rules! fields {
    ($ty:ty, $($field:ident : $field_ty:ty),+ $(,)?) => {
        impl Wire for $ty {
            fn encode(&self, e: &mut Encoder) -> Result<()> {
                e.descend(|e| { $(self.$field.encode(e)?;)+ Ok(()) })
            }
            fn decode(d: &mut Decoder<'_>) -> Result<Self> {
                d.descend(|d| Ok(Self { $($field: <$field_ty as Wire>::decode(d)?,)+ }))
            }
        }
    };
}

impl Wire for StorageBase {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        match self {
            Self::Parameter(i) => {
                0u8.encode(e)?;
                i.encode(e)
            }
            Self::Capture(i) => {
                1u8.encode(e)?;
                i.encode(e)
            }
            Self::Result => 2u8.encode(e),
            Self::Receiver => 3u8.encode(e),
        }
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(match u8::decode(d)? {
            0 => Self::Parameter(u32::decode(d)?),
            1 => Self::Capture(u32::decode(d)?),
            2 => Self::Result,
            3 => Self::Receiver,
            _ => return Err(WireError::NonCanonical),
        })
    }
}
impl Wire for ContractProjection {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        match self {
            Self::Place(p) => {
                0u8.encode(e)?;
                p.encode(e)
            }
            Self::RetainedPort(i) => {
                1u8.encode(e)?;
                i.encode(e)
            }
            // Decoding raw historical spelling is not semantic admission:
            // complete40 contracts use classes, never this borrowed-position row.
            Self::RetainedClass(i) => {
                2u8.encode(e)?;
                i.encode(e)
            }
            Self::Capture(i) => {
                3u8.encode(e)?;
                i.encode(e)
            }
        }
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        Ok(match u8::decode(d)? {
            0 => Self::Place(crate::PlaceProjection::decode(d)?),
            1 => Self::RetainedPort(u32::decode(d)?),
            2 => Self::RetainedClass(u32::decode(d)?),
            3 => Self::Capture(u32::decode(d)?),
            _ => return Err(WireError::NonCanonical),
        })
    }
}
fields!(StoragePosition, base: StorageBase, path: Vec<ContractProjection>);
fields!(StorageLifeBinding, position: StoragePosition, binder: crate::StorageBinder);
fields!(SignatureLifeClass, base: StorageBase, path: Vec<crate::LifeTypeProjection>, class: u32);
fields!(SignatureLifeClassBinding, position: SignatureLifeClass, binder: crate::StorageBinder);
impl Wire for StorageLifeContract {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| {
            e.nodes(self.fixed_binder_count as usize)?;
            e.nodes(self.invocation_binder_count as usize)?;
            self.fixed_binder_count.encode(e)?;
            self.invocation_binder_count.encode(e)?;
            self.bindings.encode(e)?;
            self.static_bindings.encode(e)?;
            self.classes.encode(e)
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            let fixed_binder_count = u32::decode(d)?;
            let invocation_binder_count = u32::decode(d)?;
            d.nodes(fixed_binder_count as usize)?;
            d.nodes(invocation_binder_count as usize)?;
            Ok(Self {
                fixed_binder_count,
                invocation_binder_count,
                bindings: Vec::<StorageLifeBinding>::decode(d)?,
                static_bindings: Vec::<StoragePosition>::decode(d)?,
                classes: Vec::<SignatureLifeClassBinding>::decode(d)?,
            })
        })
    }
}
fields!(CtAmbientLifeReference, scope: CtAmbientScopeId, parameter: u32);
fields!(CtAmbientClassBinding, owner: CtTypeNodeId, class: u32, source: CtAmbientLifeReference);
fields!(CtAmbientLifeScope, enclosing: Option<CtAmbientScopeId>, declaration: CtSymbolId, binder_site: Vec<u32>, instance: CtScopeInstance);
impl Wire for CtScopeActual {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| match self {
            Self::Type(ty) => {
                0u8.encode(e)?;
                ty.encode(e)
            }
            Self::Scalar(value) => {
                1u8.encode(e)?;
                value.encode(e)
            }
            Self::Lifetime(life) => {
                2u8.encode(e)?;
                life.encode(e)
            }
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Type(CtTypeRef::decode(d)?),
                1 => Self::Scalar(Constant::decode(d)?),
                2 => Self::Lifetime(CtAmbientLifeReference::decode(d)?),
                _ => return Err(WireError::NonCanonical),
            })
        })
    }
}
impl Wire for CtScopeInstance {
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.descend(|e| match self {
            Self::Definition => 0u8.encode(e),
            Self::Instantiated(actuals) => {
                1u8.encode(e)?;
                actuals.encode(e)
            }
        })
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.descend(|d| {
            Ok(match u8::decode(d)? {
                0 => Self::Definition,
                1 => Self::Instantiated(Vec::<CtScopeActual>::decode(d)?),
                _ => return Err(WireError::NonCanonical),
            })
        })
    }
}

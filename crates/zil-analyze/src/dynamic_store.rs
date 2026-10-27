//! Producer-side prefix resource admission; Verify rebuilds it independently.
use zil_ir::{InstructionKind, Type, ValueId};

pub(super) fn is_carrier(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) | Type::Desc(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. }))
}

pub(super) fn access(kind: &InstructionKind) -> Option<(ValueId, bool)> {
    match *kind {
        InstructionKind::OwnedStoreReserve { store, .. } | InstructionKind::OwnedStorePush { store, .. }
        | InstructionKind::OwnedStorePop { store } => Some((store, true)),
        InstructionKind::OwnedStoreLen { store } | InstructionKind::OwnedStoreCapacity { store } => Some((store, false)),
        _ => None,
    }
}

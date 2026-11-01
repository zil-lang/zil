//! Ordinary finite owning composition. This gate grants no container, capture,
//! shared-address or resource-header qualification. Callers reserve the bounded
//! complete type before querying; classification visits each shape node once.
use super::*;
fn shape(ty: &Type) -> Option<bool> {
    match ty {
        Type::Managed(_) => resources::supported(ty).then_some(true),
        Type::Package { name, fields } => {
            if name.is_empty() {
                return None;
            }
            let mut owns = false;
            for (name, field) in fields {
                if name.is_empty() {
                    return None;
                }
                owns |= shape(field)?;
            }
            owns.then_some(true)
        }
        Type::Enum { name, variants } => {
            if name.is_empty() {
                return None;
            }
            let mut owns = false;
            for (name, fields) in variants {
                if name.is_empty() {
                    return None;
                }
                for field in fields {
                    owns |= shape(field)?;
                }
            }
            Some(owns)
        }
        Type::ErrorUnion { error, result } => {
            // The older direct Managed-success profile retains its Copy-error rule.
            if matches!(result.as_ref(), Type::Managed(_)) {
                return (error.is_copy_value() && resources::supported(result)).then_some(true);
            }
            if !matches!(error.as_ref(), Type::Package { .. } | Type::Enum { .. })
                && !error.is_copy_value()
            {
                return None;
            }
            if !matches!(result.as_ref(), Type::Package { .. } | Type::Enum { .. })
                && !result.is_copy_value()
            {
                return None;
            }
            Some(shape(error)? | shape(result)?)
        }
        _ => ty.is_copy_value().then_some(false),
    }
}
pub(super) fn value(ty: &Type) -> bool {
    shape(ty).is_some()
}
pub(super) fn enumeration(ty: &Type) -> bool {
    matches!(ty, Type::Enum { .. }) && shape(ty) == Some(true)
}
pub(super) fn sum(ty: &Type) -> bool {
    matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) && shape(ty) == Some(true)
}
pub(super) fn error_union(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { .. }) && shape(ty).is_some()
}
pub(super) fn field(ty: &Type) -> bool {
    matches!(ty, Type::Managed(_) | Type::Package { .. }) || sum(ty)
}

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

pub(super) fn validate(ty: &Type, remaining: &mut usize, depth_limit: usize, span: Span) -> R<()> {
    let work = inference::type_snapshot_work(
        ty,
        span,
        &Limits {
            max_analysis_steps: *remaining,
            max_call_depth: depth_limit,
            ..Limits::default()
        },
    )?;
    *remaining = remaining
        .checked_sub(work.saturating_mul(3))
        .ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "ordinary Sum admission work exceeded",
                Some(span),
            )
        })?;
    if !value(ty) {
        return Err(unsupported(
            span,
            "ordinary owning composition requires complete Copy, Managed, Product or Sum values",
        ));
    }
    fn row<'a>(
        names: impl Iterator<Item = &'a str>,
        count: usize,
        remaining: &mut usize,
        span: Span,
    ) -> R<()> {
        let mut seen = std::collections::BTreeSet::new();
        for name in names {
            let cost = name
                .len()
                .saturating_add(1)
                .saturating_mul(count.saturating_add(1).ilog2() as usize + 2);
            *remaining = remaining.checked_sub(cost).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "ordinary Sum identity work exceeded",
                    Some(span),
                )
            })?;
            if name.is_empty() || !seen.insert(name) {
                return Err(error(
                    "E-OWNED-SHAPE",
                    span,
                    "owning fields and variants require unique nonempty names",
                ));
            }
        }
        Ok(())
    }
    fn names(ty: &Type, remaining: &mut usize, span: Span) -> R<()> {
        match ty {
            Type::Package { fields, .. } => {
                row(
                    fields.iter().map(|(n, _)| n.as_str()),
                    fields.len(),
                    remaining,
                    span,
                )?;
                for (_, ty) in fields {
                    names(ty, remaining, span)?;
                }
            }
            Type::Enum { variants, .. } => {
                row(
                    variants.iter().map(|(n, _)| n.as_str()),
                    variants.len(),
                    remaining,
                    span,
                )?;
                for (_, fields) in variants {
                    for ty in fields {
                        names(ty, remaining, span)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                names(error, remaining, span)?;
                names(result, remaining, span)?;
            }
            _ => {}
        }
        Ok(())
    }
    names(ty, remaining, span)
}

pub(super) fn product_with_sum(ty: &Type) -> bool {
    fn contains(ty: &Type) -> bool {
        match ty {
            Type::Enum { .. } | Type::ErrorUnion { .. } => sum(ty),
            Type::Package { fields, .. } => fields.iter().any(|(_, field)| contains(field)),
            _ => false,
        }
    }
    matches!(ty, Type::Package { .. }) && contains(ty) && value(ty)
}

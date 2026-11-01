//! Ordinary complete Package Products. This classification grants no capture,
//! Store element, Sum, owned-array or borrowed-package capability.
use super::*;
use std::collections::BTreeSet;

pub(crate) fn nested(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, field)| matches!(field, Type::Package { .. })))
}

// Callers already bound and reserve the complete input type tree. This walk
// allocates no auxiliary collection and cannot follow a recursive type ID.
pub(crate) fn supported(ty: &Type) -> bool {
    shape(ty, false)
}

/// Ordinary ownership alone admits Store leaves; container, shared and capture
/// callers retain `supported` and never inherit this qualification.
pub(crate) fn owned_supported(ty: &Type) -> bool {
    shape(ty, true)
}

fn shape(ty: &Type, stores: bool) -> bool {
    let Type::Package { name, fields } = ty else {
        return false;
    };
    !name.is_empty()
        && fields
            .iter()
            .any(|(_, field)| matches!(field, Type::Managed(_) | Type::Package { .. }))
        && fields.iter().all(|(name, field)| {
            !name.is_empty()
                && (field.is_copy_value()
                    || matches!(field, Type::Managed(payload)
                        if (stores && dynamic_store::is_carrier(field))
                            || **payload == Type::ByteBuffer
                            || (matches!(payload.as_ref(), Type::Record { .. })
                                && payload.is_copy_value()))
                    || shape(field, stores))
        })
}

pub(crate) fn validate(ty: &Type, remaining: &mut usize, depth_limit: usize, span: Span) -> R<()> {
    validate_shape(ty, remaining, depth_limit, span, false)
}

pub(crate) fn validate_owned(
    ty: &Type,
    remaining: &mut usize,
    depth_limit: usize,
    span: Span,
) -> R<()> {
    validate_shape(ty, remaining, depth_limit, span, true)
}

fn validate_shape(
    ty: &Type,
    remaining: &mut usize,
    depth_limit: usize,
    span: Span,
    stores: bool,
) -> R<()> {
    let work = inference::type_snapshot_work(
        ty,
        span,
        &Limits {
            max_analysis_steps: *remaining,
            max_call_depth: depth_limit,
            ..Limits::default()
        },
    )?;
    *remaining -= work;
    if !shape(ty, stores) {
        return Err(unsupported(
            span,
            if stores {
                "ordinary Package Product requires complete children, finite Copy fields and explicit supported Managed leaves"
            } else {
                "ordinary Package Product requires complete children, finite Copy fields and explicit Managed ByteBuffer or user-resource leaves"
            },
        ));
    }
    fn names(ty: &Type, remaining: &mut usize, span: Span) -> R<()> {
        let Type::Package { fields, .. } = ty else {
            return Ok(());
        };
        let work = fields.iter().fold(fields.len(), |work, (name, _)| {
            work.saturating_add(
                name.len()
                    .saturating_mul(fields.len().saturating_add(1).ilog2() as usize + 2),
            )
        });
        *remaining = remaining.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "Product field identity budget exceeded",
                Some(span),
            )
        })?;
        let mut seen = BTreeSet::new();
        for (name, field) in fields {
            if !seen.insert(name) {
                return Err(error(
                    "E-FIELD-DUPLICATE",
                    span,
                    "duplicate Product field name",
                ));
            }
            names(field, remaining, span)?;
        }
        Ok(())
    }
    names(ty, remaining, span)
}

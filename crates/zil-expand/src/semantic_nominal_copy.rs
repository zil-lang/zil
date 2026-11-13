//! Allocation prepayment for an actual retained Type clone. This visitor is
//! structural only; source type qualification is checked by later stages.
use super::*;
fn rows<T>(count: usize, budget: &mut Budget<'_>, span: Span) -> Result<(), Diagnostic> {
    let bytes = count.checked_mul(std::mem::size_of::<T>()).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Expand,
            "nominal Type clone allocation overflow",
            Some(span),
        )
    })?;
    budget.lineage_bytes(bytes, span)
}
fn text(value: &str, budget: &mut Budget<'_>, span: Span) -> Result<(), Diagnostic> {
    budget.lineage_bytes(value.len(), span)
}
fn storage(
    value: &StorageLifeShape,
    budget: &mut Budget<'_>,
    span: Span,
) -> Result<(), Diagnostic> {
    rows::<StorageLifeSlot>(value.slots.len(), budget, span)?;
    rows::<StorageRetainedBinding>(value.retained.len(), budget, span)?;
    rows::<NominalLifeArgument>(value.nominal_arguments.len(), budget, span)?;
    for slot in &value.slots {
        rows::<PlaceProjection>(slot.path.len(), budget, span)?;
    }
    for slot in &value.retained {
        rows::<LifeTypeProjection>(slot.path.len(), budget, span)?;
    }
    for slot in &value.nominal_arguments {
        rows::<LifeTypeProjection>(slot.path.len(), budget, span)?;
    }
    Ok(())
}
fn retained(
    value: &RetainedLifeShape,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> Result<(), Diagnostic> {
    rows::<RetainedBorrowPort>(value.ports.len(), budget, span)?;
    rows::<StorageRetainedBinding>(value.retained.len(), budget, span)?;
    rows::<NominalLifeArgument>(value.nominal_arguments.len(), budget, span)?;
    for slot in &value.ports {
        space(&slot.leaf, budget, span, depth + 1)?;
    }
    for slot in &value.retained {
        rows::<LifeTypeProjection>(slot.path.len(), budget, span)?;
    }
    for slot in &value.nominal_arguments {
        rows::<LifeTypeProjection>(slot.path.len(), budget, span)?;
    }
    Ok(())
}
fn space(
    value: &Type,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> Result<(), Diagnostic> {
    budget.node(span, depth)?;
    match value {
        Type::Named(name) | Type::Unsupported(name) => text(name, budget, span)?,
        Type::Record { name, fields } | Type::Package { name, fields } => {
            text(name, budget, span)?;
            rows::<(String, Type)>(fields.len(), budget, span)?;
            for (name, field) in fields {
                text(name, budget, span)?;
                space(field, budget, span, depth + 1)?;
            }
        }
        Type::Enum { name, variants } => {
            text(name, budget, span)?;
            rows::<(String, Vec<Type>)>(variants.len(), budget, span)?;
            for (name, fields) in variants {
                text(name, budget, span)?;
                rows::<Type>(fields.len(), budget, span)?;
                for field in fields {
                    space(field, budget, span, depth + 1)?;
                }
            }
        }
        Type::Tuple(fields) => {
            rows::<Type>(fields.len(), budget, span)?;
            for field in fields {
                space(field, budget, span, depth + 1)?;
            }
        }
        Type::BorrowedStorage {
            storage: inner,
            life,
        } => {
            storage(life, budget, span)?;
            rows::<Type>(1, budget, span)?;
            space(inner, budget, span, depth + 1)?;
        }
        Type::Address { inner, .. }
        | Type::Desc(inner)
        | Type::Managed(inner)
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::OwnedStore { element: inner }
        | Type::Mutex { payload: inner }
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner }
        | Type::Opaque { hidden: inner, .. } => {
            rows::<Type>(1, budget, span)?;
            space(inner, budget, span, depth + 1)?;
        }
        Type::Task {
            result,
            retained_life,
        } => {
            rows::<Type>(1, budget, span)?;
            space(result, budget, span, depth + 1)?;
            retained(retained_life, budget, span, depth + 1)?;
        }
        Type::MutexGuard {
            payload,
            retained_life,
        } => {
            rows::<Type>(1, budget, span)?;
            space(payload, budget, span, depth + 1)?;
            retained(retained_life, budget, span, depth + 1)?;
        }
        Type::ErrorUnion { error, result } => {
            rows::<Type>(2, budget, span)?;
            space(error, budget, span, depth + 1)?;
            space(result, budget, span, depth + 1)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result }
        | Type::BorrowedFunctionPointer {
            parameters, result, ..
        } => {
            rows::<Type>(parameters.len(), budget, span)?;
            rows::<Type>(1, budget, span)?;
            for parameter in parameters {
                space(parameter, budget, span, depth + 1)?;
            }
            space(result, budget, span, depth + 1)?;
            if let Type::BorrowedFunctionPointer { life, .. } = value {
                rows::<StorageLifeBinding>(life.bindings.len(), budget, span)?;
                rows::<SignatureLifeClassBinding>(life.classes.len(), budget, span)?;
                rows::<StoragePosition>(life.static_bindings.len(), budget, span)?;
                for binding in &life.bindings {
                    rows::<LifeTypeProjection>(binding.position.path.len(), budget, span)?;
                }
                for binding in &life.classes {
                    rows::<LifeTypeProjection>(binding.position.path.len(), budget, span)?;
                }
                for binding in &life.static_bindings {
                    rows::<LifeTypeProjection>(binding.path.len(), budget, span)?;
                }
            }
        }
        Type::Dyn { .. }
        | Type::OwnedDyn { .. }
        | Type::ByteBuffer
        | Type::TaskI32
        | Type::ForeignHandle
        | Type::CWideString
        | Type::SyncCell(_)
        | Type::Closure(_)
        | Type::Unit
        | Type::Bool
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Usize
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::Isize
        | Type::F32
        | Type::F64 => {}
    }
    Ok(())
}
pub(super) fn ty(
    value: &Type,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> Result<(), Diagnostic> {
    crate::stored_types::pay_type(value, budget, span, depth)?;
    space(value, budget, span, depth)
}

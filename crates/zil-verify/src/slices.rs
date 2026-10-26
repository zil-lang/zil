//! Independent admission of unsized borrowed sequence windows. This module
//! classifies raw shapes only; CFG provenance and loans remain independently
//! reconstructed in `borrows`, never supplied by a producer assertion.
use super::*;

pub(super) fn target(target: &TargetProfile) -> Check<()> {
    if !target.is_supported_x64_llvm() {
        return Err(unsupported(
            "borrowed slices require the supported checked x64 little-endian Linux GNU or Win64 LLVM profile",
            None,
        ));
    }
    Ok(())
}

/// Called only after the entire raw type tree was charged and bounded. An
/// unsized type is admitted solely as this one address layer's pointee. Its
/// elements are checked by the independent finite-value checker; nested
/// Address/Slice/resource/owning shapes cannot obtain admission here.
pub(super) fn check_pointee(ty: &Type, span: Option<Span>) -> Check<()> {
    match ty {
        Type::Slice { element } => check_type(element, span),
        _ => check_type(ty, span),
    }
}

pub(super) fn is_window(ty: &Type) -> bool {
    matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Slice { .. }))
}

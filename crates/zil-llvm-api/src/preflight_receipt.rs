//! Process-local receipts owned by the machine preflight checker.
//!
//! This proves only `validate_module`'s structural machine obligations. It is
//! neither a ZIL verification capability nor a Builder schedule, LLVM verifier
//! result, native target/configuration check, or object-cache integrity proof.
use crate::model::{Item, Module, SharedVec};
use crate::{Error, preflight, validate_module};

/// A successful machine preflight bound to its complete checked inputs.
///
/// Construction is private and there is no serialization or hash-based import.
/// The receipt therefore belongs to the same running checker binary, including
/// its fixed type/constant nesting limits and all other compiled rules. Those
/// rules have no caller-controlled configuration or external mutable inputs.
///
/// Retaining the immutable item allocation prevents safe in-place mutation and
/// address reuse; `SharedVec` mutations copy before writing. Exact structural
/// equality also admits independently assembled identical inputs. The model's
/// diagnostic `fingerprint` is absent from the checker's input projection.
///
/// Raw modules cannot construct a proof:
/// ```compile_fail
/// use zil_llvm_api::{PreflightReceipt, model::Module};
/// fn counterfeit(module: Module) -> PreflightReceipt {
///     PreflightReceipt { triple: module.triple, items: module.items }
/// }
/// ```
#[derive(Clone, Debug)]
pub struct PreflightReceipt {
    triple: String,
    items: SharedVec<Item>,
}

/// Independently authorize the current module, reusing only an exact matching
/// checker-issued proof. `true` means the previous proof was reused; `false`
/// means the complete ordinary preflight executed successfully on this call.
///
/// Every changed checked input falls through to `validate_module`, preserving
/// its checks, limits and first error. Failure returns no receipt. On success
/// the returned receipt retains the *current* allocation, without a history
/// chain. The caller controls when to publish it in its surrounding transaction.
pub fn validate_module_with_receipt(
    module: &Module,
    prior: Option<&PreflightReceipt>,
) -> Result<(PreflightReceipt, bool), Error> {
    let (triple, items) = preflight::inputs(module);
    let reused = prior.is_some_and(|prior| prior.triple == triple && prior.items == *items);
    if !reused {
        validate_module(module)?;
    }
    Ok((
        PreflightReceipt {
            triple: triple.to_owned(),
            items: items.clone(),
        },
        reused,
    ))
}

#[cfg(test)]
mod tests;

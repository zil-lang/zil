//! Closed Store instructions still use the independent sealed resource and
//! actual descriptor/loan kernels; this is structural admission only.
use super::*;

pub(super) fn operation(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.ordinary_closed_stores {
        return Ok(false);
    }
    let element = if let InstructionKind::OwnedStoreNew { element } = &i.kind {
        element
    } else if let Some((value, _)) = crate::dynamic_store::access(&i.kind) {
        let Some(element) = crate::dynamic_store::element(vt(f, value)?) else {
            return Err(err("ordinary Store operation has no actual Store carrier"));
        };
        element
    } else {
        return Ok(false);
    };
    if !pure_copy::ordinary_owned_fields(vec![(element, 0)], b)? {
        return Ok(false);
    }
    crate::dynamic_store::check_instruction(module, f, i, b)?;
    Ok(true)
}

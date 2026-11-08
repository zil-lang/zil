//! Whole Array reads require every physical candidate of every actual root.
//! Fixed-point transfer may be bottom; this check runs only at final windows.
use super::*;

pub(super) fn check(
    state: &State,
    address: &Permission,
    length: u32,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, address.targets.len().saturating_add(1))?;
    if length == 0 || address.targets.is_empty() {
        return Err(failure(
            "whole shared Array snapshot has no actual carrier source",
        ));
    }
    for root in &address.targets {
        if !root.stored.is_empty() {
            return Err(failure(
                "whole shared Array snapshot crosses a stored target boundary",
            ));
        }
        let slots = match root.base {
            HoldRoot::Static => return Err(failure("static text cannot supply an Array snapshot")),
            HoldRoot::Local(local) => {
                ordered(b, state.locals.len(), 0)?;
                &state
                    .locals
                    .get(&local)
                    .ok_or_else(|| failure("Array snapshot actual carrier is absent"))?
                    .slots
            }
            HoldRoot::Parameter(_) | HoldRoot::Capture(_) => {
                ordered(b, state.external.len(), 0)?;
                state
                    .external
                    .get(&root.base)
                    .ok_or_else(|| failure("Array snapshot external carrier is absent"))?
            }
        };
        fee(b, length as usize)?;
        for index in 0..length {
            // Copy the complete actual root and append only the checked Array
            // ordinal. This is a physical position, never a source substitute.
            fee(b, target_width(root).saturating_add(2))?;
            let mut position = root.clone();
            position.place.push(PlaceProjection::Field(index));
            if !state.vacant.is_empty() {
                ordered(b, state.vacant.len(), target_width(&position))?;
                if state.vacant.contains(&position) {
                    return Err(failure("whole shared Array snapshot reads a vacant slot"));
                }
            }
            ordered(b, slots.len(), position.place.len())?;
            if slots
                .get(&position.place)
                .is_none_or(|leaf| leaf.targets.is_empty())
            {
                return Err(failure(
                    "whole shared Array snapshot lacks an actual source leaf",
                ));
            }
        }
    }
    Ok(())
}

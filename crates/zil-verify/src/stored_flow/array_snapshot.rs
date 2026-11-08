//! The final point state must supply a complete Array for each real source.
//! A union from another root is not evidence for a missing or vacant leaf.
use super::*;

pub(super) fn check(state: &Frame, address: &Permission, length: u32, b: &mut Budget) -> Check<()> {
    b.spend(address.sources.len().saturating_add(1))?;
    if length == 0 || address.sources.is_empty() {
        return Err(err(
            "whole shared Array snapshot has no actual carrier source",
        ));
    }
    for root in &address.sources {
        if !root.leaf.is_empty() {
            return Err(err(
                "whole shared Array snapshot crosses a stored target boundary",
            ));
        }
        b.spend(1)?;
        let payload = match root.base {
            HoldRoot::Static => return Err(err("static text cannot supply an Array snapshot")),
            HoldRoot::Local(local) => state.locals.get(local.0 as usize),
            HoldRoot::Parameter(parameter) => state.external.get(parameter as usize),
            HoldRoot::Capture(capture) => state.captures.get(capture as usize),
        }
        .ok_or_else(|| err("Array snapshot actual carrier is absent"))?;
        b.spend(length as usize)?;
        for index in 0..length {
            b.spend(width(root).saturating_add(2))?;
            let mut position = root.clone();
            position.inline.push(PlaceProjection::Field(index));
            if !state.vacant.is_empty() {
                b.spend(amount(
                    state.vacant.len().saturating_add(1),
                    width(&position),
                )?)?;
                if state.vacant.contains(&position) {
                    return Err(err("whole shared Array snapshot reads a vacant slot"));
                }
            }
            if leaf(payload, &position.inline, b)?.is_none_or(|leaf| leaf.sources.is_empty()) {
                return Err(err(
                    "whole shared Array snapshot lacks an actual source leaf",
                ));
            }
        }
    }
    Ok(())
}

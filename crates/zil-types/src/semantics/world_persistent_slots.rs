//! One actual owned-array element represents the source callback position.
//! This transports its complete type domain, never execution permissions.
use super::*;

pub(super) fn storage(world: &mut World<'_, '_>, physical: &Type, element: &SemanticTypeValue) -> R<SharedType> {
    let value = super::super::materialize::binding_slot_storage(physical, element, world.limits, world.left)?;
    world.pay(1)?;
    Ok(Rc::new(value))
}

pub(super) fn constructor(
    world: &mut World<'_, '_>, function: &TypedFunction, block: BlockId, instruction: usize,
    fields: &[ValueId], values: &ssa::Values,
) -> R<Option<SharedType>> {
    let Some(slot) = world.ledger.binding_slot_constructor(
        function, super::super::emission::Point { block, instruction }, world.limits, world.left,
    )? else { return Ok(None); };
    if fields != [slot.input] {
        return Err(bad("persistent callback constructor changes its actual initializer"));
    }
    let element = world.get(values, slot.input)?;
    let physical = function.values.get(slot.carrier.0 as usize)
        .ok_or_else(|| bad("binding cell constructor lacks its actual storage"))?;
    storage(world, physical, &element).map(Some)
}

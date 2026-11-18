//! Borrowing a whole Source callee consumes its exact prepared place receipt.
//! Field paths and argument-side invalidation are checked by the ordinary
//! actual projection/world flow; type equality never selects the callee.
use super::*;

pub(super) fn check(
    world: &mut World<'_, '_>,
    caller: &SemanticInstance,
    original: &SemanticDefinition,
    call: &SemanticSite,
    node: zil_base::NodeId,
    raw: &SemanticTypeValue,
    values: &ssa::Values,
) -> R<()> {
    let address = occurrences::prepared_callee(world, caller, original, call, node)?
        .ok_or_else(|| bad("borrowed whole callee lacks its exact prepared place receipt"))?;
    let selected = world.get(values, address)?;
    let selected = roles::outer(world, &selected)?;
    let expected = roles::outer(world, raw)?;
    for source in [&selected, &expected] {
        if let roles::Source::Class(class) = source {
            crate::dyn_metadata::semantic_operand_snapshot(class, world.limits, world.left, 1)?;
        }
    }
    if selected != expected {
        return Err(bad(
            "prepared callee changes its complete actual address occurrence",
        ));
    }
    Ok(())
}

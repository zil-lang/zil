//! Instantiating one anonymous readonly outer loan with a real static input
//! creates no class. This substitution is confined to one call boundary.
use super::*;
#[derive(Default)]
pub(super) struct Inputs {
    pub classes: Vec<(SemanticClassExpr, SemanticClassExpr)>,
    pub statics: Vec<SemanticClassExpr>,
}
pub(super) fn static_input(
    world: &mut World<'_, '_>,
    inputs: &mut Inputs,
    source: &SemanticClassExpr,
    parameter: &SemanticTypeValue,
    position: &StoragePosition,
) -> R<bool> {
    if !position.path.is_empty()
        || !matches!(
            World::type_of(parameter)?,
            Type::Address { mutable: false, .. }
        )
        || roles::outer(world, parameter)? != roles::Source::Static
    {
        return Ok(false);
    }
    world.pay(
        inputs
            .classes
            .len()
            .saturating_add(inputs.statics.len())
            .saturating_add(1),
    )?;
    crate::dyn_metadata::semantic_operand_snapshot(source, world.limits, world.left, 1)?;
    if inputs
        .classes
        .iter()
        .any(|(candidate, _)| candidate == source)
    {
        return Err(bad(
            "one anonymous call input has both static and variable class images",
        ));
    }
    if !inputs.statics.contains(source) {
        inputs.statics.push(source.clone())
    }
    Ok(true)
}
pub(super) fn map(
    world: &mut World<'_, '_>,
    value: &SemanticTypeValue,
    actuals: &[SemanticActual],
    environment: Option<(&StorageLifeContract, &SemanticInstance, &SemanticTypeValue)>,
    inputs: &Inputs,
) -> R<SemanticTypeValue> {
    let mut value = world.copy(value)?;
    let SemanticTypeValue::Resolved { ty, life, classes } = &mut value else {
        return Err(bad("static invocation retains an unresolved whole type"));
    };
    for source in &inputs.statics {
        world.pay(
            classes
                .len()
                .saturating_add(life.slots.len())
                .saturating_add(life.retained.len())
                .saturating_add(life.nominal_arguments.len()),
        )?;
        let mut positions = classes
            .iter()
            .enumerate()
            .filter(|(_, candidate)| *candidate == source);
        let Some((position, _)) = positions.next() else {
            continue;
        };
        if positions.next().is_some() {
            return Err(bad(
                "static outer instantiation would merge distinct intrinsic class slots",
            ));
        }
        let position =
            u32::try_from(position).map_err(|_| bad("static invocation class index overflows"))?;
        if !matches!(ty, Type::Address { mutable: false, .. })
            || life
                .slots
                .iter()
                .filter(|slot| slot.binder == position && slot.path.is_empty())
                .count()
                != 1
            || life
                .slots
                .iter()
                .any(|slot| slot.binder == position && !slot.path.is_empty())
            || life.retained.iter().any(|row| row.binder == position)
            || life
                .nominal_arguments
                .iter()
                .any(|row| row.binder == position)
        {
            return Err(pending(
                "static invocation image occurs inside a complete intrinsic or opaque domain",
            ));
        }
        // Only the quantified outer head disappears. Every child intrinsic
        // position and nominal identity remains, with parent indices rebased.
        life.slots.retain(|slot| slot.binder != position);
        for slot in &mut life.slots {
            if slot.binder > position {
                slot.binder -= 1
            }
        }
        for row in &mut life.retained {
            if row.binder > position {
                row.binder -= 1
            }
        }
        for row in &mut life.nominal_arguments {
            if row.binder > position {
                row.binder -= 1
            }
        }
        classes.remove(position as usize);
        life.binder_count = life
            .binder_count
            .checked_sub(1)
            .ok_or_else(|| bad("static invocation changes an empty complete domain"))?;
        if life.binder_count as usize != classes.len() {
            return Err(bad("static invocation truncates the entire class domain"));
        }
    }
    world.map_call_value(&value, actuals, environment, &inputs.classes)
}

//! Actual code can implement several original instantiation worlds. Every
//! world keeps its own classes; equal machine code does not merge lifetimes.
use super::*;
impl<'a> Check<'a, '_> {
    pub(super) fn function_worlds(
        &self,
        target: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<Vec<&'a SemanticInstance>> {
        let mut worlds = Vec::new();
        for instance in &self.program.instances {
            b.spend(1, WORK)?;
            if matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id==target)
            {
                self.push_value(&mut worlds, instance, b)?;
            }
        }
        Ok(worlds)
    }

    // Some consumers ask only for a definition-independent, zero-class view.
    // Canonicalize only after checking *every* full view for equivalence. This
    // path cannot identify two Life actuals, parent environments or Fixed rows.
    pub(super) fn shared_zero_view(
        &self,
        target: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticInstance> {
        let worlds = self.function_worlds(target, b)?;
        let [only] = worlds.as_slice() else {
            let mut canonical = None;
            for world in &worlds {
                if world.parent.is_some()
                    || world.environment.is_some()
                    || world.self_type.is_some()
                    || !world.header_embedding.is_empty()
                {
                    return Err(unknown(
                        "shared code needs its actual caller/creator world and complete class boundary",
                    ));
                }
                for actual in &world.actuals {
                    let value = self.actual_value(world, actual, b, 0)?;
                    match value {
                        crate::semantic::value::Argument::Type(value)
                            if empty(&value.life) && value.classes.is_empty() => {}
                        crate::semantic::value::Argument::Const { .. } => {}
                        _ => {
                            return Err(unknown(
                                "shared code lifetime worlds require exact contextual instantiation, not a global representative",
                            ));
                        }
                    }
                }
                if let Some(previous) = canonical {
                    let previous: &SemanticInstance = previous;
                    b.spend(
                        world
                            .actuals
                            .len()
                            .saturating_add(previous.actuals.len() + 1),
                        WORK,
                    )?;
                    for actual in world.actuals.iter().chain(&previous.actuals) {
                        crate::dyn_metadata::recipe(actual, b)?;
                    }
                    if world.definition != previous.definition
                        || world.lifetime_instantiation != previous.lifetime_instantiation
                        || world.actuals != previous.actuals
                    {
                        return Err(unknown(
                            "shared code has distinct original actual worlds; its caller must select the complete boundary",
                        ));
                    }
                }
                if canonical.is_none_or(|previous: &SemanticInstance| {
                    (world.id.module.0, world.id.index) < (previous.id.module.0, previous.id.index)
                }) {
                    canonical = Some(*world);
                }
            }
            return canonical
                .ok_or_else(|| fail("actual function has no complete original semantic world"));
        };
        Ok(*only)
    }
}

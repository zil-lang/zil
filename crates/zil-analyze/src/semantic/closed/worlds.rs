//! Actual code can implement several original instantiation worlds. Every
//! world keeps its own classes; equal machine code does not merge lifetimes.
use super::*;
impl<'a> Check<'a, '_> {
    pub(super) fn function_worlds(
        &self,
        target: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<Vec<&'a SemanticInstance>> {
        self.inventory.worlds(self.program, target, b)
    }

    pub(super) fn function_definition(
        &self,
        target: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<SemanticDefinitionId> {
        let worlds = self.function_worlds(target, b)?;
        let definition = worlds
            .first()
            .ok_or_else(|| fail("actual function has no complete original semantic world"))?
            .definition;
        if worlds.iter().any(|world| world.definition != definition) {
            return Err(fail("shared code changes its original declaration"));
        }
        Ok(definition)
    }

    pub(super) fn closure_creator_world(
        &self,
        target: FunctionId,
        creator: SemanticInstanceId,
        closure: ClosureId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticInstance> {
        let mut selected = None;
        for world in self.function_worlds(target, b)? {
            if world.parent == Some(creator)
                && world.environment.as_ref().is_some_and(|environment| {
                    environment.closure == SemanticClosureTarget::Typed(closure)
                })
                && selected.replace(world).is_some()
            {
                return Err(fail("closure construction has competing actual creator worlds"));
            }
        }
        selected.ok_or_else(|| fail("closure construction has no actual creator world"))
    }
}

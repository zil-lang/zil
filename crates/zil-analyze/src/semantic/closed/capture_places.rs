//! Original environment slots and actual physical captures are separate
//! namespaces. Every relation here comes from its actual name-site operation.
use super::*;
use crate::semantic::value::{self, Value};
impl Check<'_, '_> {
    pub(super) fn environment_capture(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        ordinal: u32,
        b: &mut Budget,
    ) -> CompileResult<u32> {
        let id = function
            .environment
            .ok_or_else(|| fail("original environment place has no actual closure entry"))?;
        // Identity is formed from every real creator before whole-directory
        // validation. Requiring that validation here creates a false dependency
        // from a directory's own occurrence checks back to itself. Publication
        // still requires every complete directory and every actual site.
        let closure = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|item| item.id == id && item.function == function.id)
            .ok_or_else(|| fail("environment binding has another actual closure entry"))?;
        let creators = self.capture_creator_bindings(instance, closure, b)?;
        b.lookup(creators.len(), WORK)?;
        if let Some(binding) = creators.get(&ordinal) {
            // This is binding identity from every real MakeClosure. It does
            // not assert that a later occupant still has its initial data.
            return Ok(binding.capture);
        }
        let definition = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. } = &definition.body
        else {
            return Err(fail(
                "original environment place has no complete original body",
            ));
        };
        let mut selected = None;
        for recipe in recipes {
            b.spend(1, WORK)?;
            if self
                .original
                .capture_recipe_ordinal(instance.definition, recipe, b)?
                != Some(ordinal)
            {
                continue;
            }
            for site in &self.program.sites {
                b.spend(1, WORK)?;
                if site.instance != instance.id || site.recipe != recipe.id {
                    continue;
                }
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge,
                } = site.at
                else {
                    return Err(fail("original capture place retains a Core site"));
                };
                let SemanticSiteAction::TypeUse { subject, .. } = &site.action else {
                    return Err(fail("original capture place changes its name-site role"));
                };
                let capture = match subject {
                    SemanticOperand::Capture(index) => *index,
                    SemanticOperand::Value(value) => {
                        self.capture_value_origin(function, *value, b, 0)?
                    }
                    SemanticOperand::Local(local) => self.capture_local_origin(
                        function,
                        *local,
                        (
                            block,
                            instruction as usize + usize::from(edge == SemanticSiteEdge::After),
                        ),
                        b,
                        0,
                    )?,
                    _ => {
                        return Err(unknown(
                            "original environment place needs its exact actual capture-name selector",
                        ));
                    }
                };
                if selected.is_some_and(|old| old != capture) {
                    return Err(fail(
                        "one original environment name selects several actual captures",
                    ));
                }
                selected = Some(capture);
            }
        }
        selected.ok_or_else(|| {
            fail("original environment place has no independently checked actual name emission")
        })
    }
    pub(super) fn capture_whole_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        index: u32,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual whole capture value exceeds its finite depth",
            ));
        }
        let closure = function
            .environment
            .and_then(|id| self.module.closures.get(id.0 as usize))
            .filter(|closure| closure.function == function.id)
            .ok_or_else(|| fail("actual whole capture has another environment entry"))?;
        let ty = closure
            .captures
            .get(index as usize)
            .ok_or_else(|| fail("actual whole capture ordinal is absent"))?;
        self.actual_type(ty, b)?;
        // This reconstructs the selected creator-bound value; it is not a
        // certificate for the containing directory. The publication pass checks
        // that directory separately after all identities have been formed.
            let creators = self.capture_creator_bindings(instance, closure, b)?;
            let mut selected = None;
            for binding in creators.values() {
                b.spend(1, WORK)?;
                if binding.capture != index {
                    continue;
                }
                if selected.replace(&binding.stored).is_some() {
                    return Err(fail(
                        "actual captured head has several original lexical sources",
                    ));
                }
            }
            let Some(stored) = selected else {
                // A scalar contributes no class domain, but its exact capture
                // identity/read is still checked at every original occurrence.
                if !ty.is_b0_scalar() {
                    return Err(unknown("actual captured value has no complete original creator binding"));
                }
                self.zero_data(ty, b, depth + 1)?;
                crate::dyn_metadata::ty(ty, b)?;
                return Ok(value::scalar(ty.clone()));
            };
            let contract = closure
                .storage_life
                .as_ref()
                .ok_or_else(|| unknown("actual captured head has no complete header domain"))?;
            let mut classes = Vec::new();
            for (class, source) in stored.classes.iter().enumerate() {
                for (_, _, binder) in
                    self.capture_class_headers(contract, index, stored, class as u32, b)?
                {
                    let actual = self.header_class(instance, binder, b)?;
                    if actual != *source && actual != self.capture_class_image(source.clone(), b)? {
                        return Err(fail(
                            "actual captured head changes its original outer or intrinsic child class",
                        ));
                    }
                }
                self.push_value(&mut classes, source.clone(), b)?;
            }
            stored.with_classes(classes, b)

    }
}

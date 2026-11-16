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
        self.capture_directory(id, b)?;
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
        self.capture_directory(closure.id, b)?;
        if matches!(
            ty,
            Type::Address { .. } | Type::Desc(_) | Type::BorrowedStorage { .. } | Type::Closure(_)
        ) || self.task_domain(ty, b)?.is_some() {
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
            let stored = selected.ok_or_else(|| {
                fail("actual captured head has no complete original creator binding")
            })?;
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
                    if actual != *source {
                        return Err(fail(
                            "actual captured head changes its original outer or intrinsic child class",
                        ));
                    }
                }
                self.push_value(&mut classes, source.clone(), b)?;
            }
            return stored.with_classes(classes, b);
        }
        self.zero_data(ty, b, depth + 1)?;
        crate::dyn_metadata::ty(ty, b)?;
        Ok(value::scalar(ty.clone()))
    }
}

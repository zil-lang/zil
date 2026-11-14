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
            if !matches!(recipe.kind, SemanticSiteRecipeKind::TypeUse { expected: SemanticTypeRecipe::Environment { scope: SemanticScopeRef::Ancestor(0), ordinal: actual } } if actual == ordinal)
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
        let life = match ty {
            Type::BorrowedStorage { life, .. } => {
                crate::dyn_metadata::shape(life, b)?;
                life.clone()
            }
            _ => {
                self.zero_data(ty, b, depth + 1)?;
                value::empty()
            }
        };
        let contract = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| unknown("actual whole capture has an unknown persistent domain"))?;
        let mut classes = Vec::new();
        for child in 0..life.binder_count {
            let mut selected = None;
            for row in &contract.classes {
                b.spend(row.position.path.len() + 1, WORK)?;
                if row.position.base == StorageBase::Capture(index)
                    && row.position.path.is_empty()
                    && row.position.class == child
                    && selected.replace(row.binder).is_some()
                {
                    return Err(fail(
                        "actual whole capture repeats an intrinsic class occurrence",
                    ));
                }
            }
            let class = self.header_class(
                instance,
                selected.ok_or_else(|| {
                    fail("actual whole capture omits an intrinsic class occurrence")
                })?,
                b,
            )?;
            self.push_value(&mut classes, class, b)?;
        }
        crate::dyn_metadata::ty(ty, b)?;
        Ok(Value {
            ty: ty.clone(),
            life,
            classes,
        })
    }
}

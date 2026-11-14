//! A header selector names an actual parameter occurrence; its semantic image
//! comes from the independently checked original signature, not the header
//! binder that happens to select it. This creates no parameter access rights.
use super::*;
use crate::semantic::value::{Class, Value};

impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn parameter_header_class(
        &self,
        instance: &SemanticInstance,
        binder: StorageBinder,
        parameter: u32,
        path: &[LifeTypeProjection],
        mode: Option<StoredBorrowMode>,
        child: Option<u32>,
        b: &mut Budget,
    ) -> CompileResult<Class> {
        let declaration = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &declaration.body else {
            return Err(fail(
                "parameter header selector omits its original signature",
            ));
        };
        let recipe = signature
            .parameters
            .get(parameter as usize)
            .ok_or_else(|| fail("parameter header selector exceeds original parameter arity"))?;
        let source: Value = self.recipe_value(instance, &recipe.ty, b, 0)?;
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail("parameter header selector has no actual function"));
        };
        let function = self.inventory.function(target, b)?;
        let local = function
            .parameters
            .get(parameter as usize)
            .ok_or_else(|| fail("parameter header selector exceeds actual parameter arity"))?;
        if function.locals.get(local.0 as usize).map(|local| &local.ty) != Some(&source.ty) {
            return Err(fail(
                "parameter header selector changes its complete original type",
            ));
        }
        let contract = function
            .storage_life
            .as_ref()
            .ok_or_else(|| fail("parameter header selector omits the actual header"))?;
        crate::signature_life::charge(contract, b)?;
        let mut selected = None;
        if let Some(mode) = mode {
            for slot in &source.life.slots {
                b.spend(slot.path.len().saturating_add(path.len() + 1), WORK)?;
                if slot.mode != mode
                    || slot.path.len() != path.len()
                    || !path
                        .iter()
                        .zip(&slot.path)
                        .all(|(a, z)| matches!(a, LifeTypeProjection::Place(p) if p == z))
                {
                    continue;
                }
                let mut actual = None;
                for row in &contract.bindings {
                    b.spend(row.position.path.len().saturating_add(1), WORK)?;
                    if row.position.base == StorageBase::Parameter(parameter)
                        && row.position.path.len() == slot.path.len()
                        && row
                            .position
                            .path
                            .iter()
                            .zip(&slot.path)
                            .all(|(a, z)| matches!(a, ContractProjection::Place(p) if p == z))
                    {
                        if actual.replace(row.binder).is_some() {
                            return Err(fail("parameter header repeats an outer occurrence"));
                        }
                    }
                }
                if actual != Some(binder) {
                    return Err(fail(
                        "parameter header selects a different actual outer binder",
                    ));
                }
                if selected.replace(slot.binder).is_some() {
                    return Err(fail(
                        "parameter header selects competing original outer occurrences",
                    ));
                }
            }
        } else if let Some(child) = child {
            let whole = if path.is_empty() {
                if child >= source.life.binder_count {
                    return Err(fail(
                        "parameter intrinsic class exceeds complete original domain",
                    ));
                }
                child
            } else {
                let mut found = None;
                for row in &source.life.retained {
                    b.spend(row.path.len().saturating_add(path.len() + 1), WORK)?;
                    if row.path == path && row.class == child && found.replace(row.binder).is_some()
                    {
                        return Err(fail(
                            "parameter intrinsic selector repeats an original embedding",
                        ));
                    }
                }
                found.ok_or_else(|| {
                    fail("parameter intrinsic selector omits its original child occurrence")
                })?
            };
            let mut actual = None;
            for row in &contract.classes {
                b.spend(row.position.path.len().saturating_add(path.len() + 1), WORK)?;
                if row.position.base == StorageBase::Parameter(parameter)
                    && row.position.path == path
                    && row.position.class == child
                    && actual.replace(row.binder).is_some()
                {
                    return Err(fail("parameter header repeats an intrinsic occurrence"));
                }
            }
            if actual != Some(binder) {
                return Err(fail(
                    "parameter header selects a different actual intrinsic binder",
                ));
            }
            selected = Some(whole);
        }
        source
            .classes
            .get(
                selected
                    .ok_or_else(|| fail("parameter header has no complete original occurrence"))?
                    as usize,
            )
            .cloned()
            .ok_or_else(|| fail("parameter header loses its original whole class"))
    }
}

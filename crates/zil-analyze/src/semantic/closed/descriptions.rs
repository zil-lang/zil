//! An implicit description is a source argument occurrence, not a change to
//! the owning local's type. Its actual weak route and epoch are checked again.
use super::*;
use crate::semantic::value::Value;
impl Check<'_, '_> {
    pub(super) fn source_description(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "implicit description source exceeds its finite depth",
            ));
        }
        self.defining(function, id, b)?;
        let mut selected = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.action,SemanticSiteAction::TypeUse {subject:SemanticOperand::Value(value),..} if value==id)
            {
                continue;
            }
            let Some(original) = self.original.description_source(
                instance,
                site.recipe,
                self.program,
                self.inventory,
                b,
            )?
            else {
                continue;
            };
            let source = &original.source;
            let expected = original.output;
            let Type::Managed(payload) = &source.ty else {
                return Err(fail(
                    "implicit description source is not an original owning value",
                ));
            };
            self.sealed_resource_place(
                instance,
                function,
                id,
                original.anchor,
                &original.root,
                source,
                &original.path,
                payload,
                b,
            )?;
            if expected.ty != Type::Desc(payload.clone())
                || function.values.get(id.0 as usize) != Some(&expected.ty)
                || expected.life.slots.len() != 1
                || !expected.life.slots[0].path.is_empty()
                || expected.life.slots[0].mode != StoredBorrowMode::WritableDescription
            {
                return Err(fail(
                    "implicit description changes its full original weak argument role",
                ));
            }
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge,
            } = site.at
            else {
                return Err(fail(
                    "implicit description retains a Core emission boundary",
                ));
            };
            operations::available_before(function, (block, instruction as usize, edge), id, b)?;
            if selected.as_ref().is_some_and(|old| old != &expected) {
                return Err(fail(
                    "implicit description has several original occurrence identities",
                ));
            }
            selected = Some(expected);
        }
        selected.ok_or_else(||unknown("actual weak description lacks its exact original argument occurrence and owning source"))
    }
}

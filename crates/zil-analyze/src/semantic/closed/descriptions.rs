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
        let (block, index, instruction) = self.defining(function, id, b)?;
        let mut selected = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.action,SemanticSiteAction::TypeUse {subject:SemanticOperand::Value(value),..} if value==id)
            {
                continue;
            }
            let Some((anchor, source, expected)) = self.original.description_source(
                instance,
                site.recipe,
                self.program,
                self.inventory,
                b,
            )?
            else {
                continue;
            };
            let local = self.anchored_local(instance, function, anchor, b)?;
            let actual = match instruction.kind {
                InstructionKind::DescribeLocal { local: actual } if actual == local => {
                    self.local_value(instance, function, local, (block, index), b, depth + 1)?
                }
                InstructionKind::DescribeAddress { address } => {
                    operations::available_before(
                        function,
                        (block, index, SemanticSiteEdge::Before),
                        address,
                        b,
                    )?;
                    let (_, _, actual, _) = self.borrow_root(function, address, b, depth + 1)?;
                    if actual != local {
                        return Err(fail(
                            "implicit description borrows another original owning head",
                        ));
                    }
                    let value = self.ssa_value(instance, function, address, b, depth + 1)?;
                    self.pointee(&value, b)?
                }
                _ => {
                    return Err(fail(
                        "implicit description selects another actual owning slot or opcode",
                    ));
                }
            };
            let Type::Managed(payload) = &source.ty else {
                return Err(fail(
                    "implicit description source is not an original owning value",
                ));
            };
            if actual != source
                || expected.ty != Type::Desc(payload.clone())
                || function.values.get(id.0 as usize) != Some(&expected.ty)
                || expected.life.slots.len() != 1
                || !expected.life.slots[0].path.is_empty()
                || expected.life.slots[0].mode != StoredBorrowMode::WritableDescription
            {
                return Err(fail(
                    "implicit description changes its complete original owner or weak occurrence",
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

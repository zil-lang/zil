//! A call adaptation is proved for one original argument and its actual SSA.
//! This relation is never a general class equivalence or a loan permission.
use super::*;
use crate::semantic::value::{self, Value};
impl Check<'_, '_> {
    pub(super) fn outer_call_role(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        at: (BlockId, usize),
        ordinal: usize,
        operand: ValueId,
        actual: &Value,
        expected: &Value,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        if self.outer_role(instance, function, actual, expected, b, depth + 1)? {
            return Ok(true);
        }
        if !geometry::same_type(&actual.ty, &expected.ty, b)? {
            return Ok(false);
        }
        if actual.life.slots.len() != 1
            || expected.life.slots.len() != 1
            || !actual.life.slots[0].path.is_empty()
            || !expected.life.slots[0].path.is_empty()
            || actual.life.slots[0].mode != expected.life.slots[0].mode
        {
            return Ok(false);
        }
        match &expected.ty {
            Type::Address { .. } => {
                if !self.capture_values_equal(
                    &value::pointee(actual, self.inventory, b)?,
                    &value::pointee(expected, self.inventory, b)?,
                    b,
                )? {
                    return Ok(false);
                }
            }
            Type::Desc(_) => {
                if actual.life != expected.life
                    || actual.classes.len() != 1
                    || expected.classes.len() != 1
                {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
        let target = expected
            .classes
            .get(expected.life.slots[0].binder as usize)
            .ok_or_else(|| fail("call role target has no complete outer class"))?;
        let ordinal = u32::try_from(ordinal).map_err(|_| fail("actual call parameter ordinal overflows"))?;
        let mut source_value = actual.copy(b)?;
        let mut source_operand = operand;
        for step in 0..=b.partial_depth {
            b.spend(1, WORK)?;
            if source_value.life.slots.len() != 1 {
                return Ok(false);
            }
            let source = source_value
                .classes
                .get(source_value.life.slots[0].binder as usize)
                .ok_or_else(|| fail("call role source has no complete outer class"))?;
            let source = self.capture_class_image(source.clone(), b)?;
            for call in self.inventory.sites_at(self.program, instance.id, at.0, at.1, SemanticSiteEdge::After, b)? {
                let nodes = self.original.call_region_nodes(instance, call.recipe, ordinal, target, &source, self.inventory, b)?;
                if nodes.is_empty() { continue; }
                // An implicit method reborrow may have no separate TypeUse.
                // Bind the original Name to its actual Load/Take/Borrow local;
                // equal Types or lifetime uppers never select this identity.
                for node in &nodes {
                    if let Some(anchor) = self.original.call_input_anchor(instance.definition, *node, b)? {
                        if self.anchored_value(instance, function, anchor, source_operand, b, depth + 1)? {
                            return Ok(true);
                        }
                    }
                }
                for site in self.inventory.sites_for_value(self.program, instance.id, source_operand, b)? {
                    let SemanticSiteAction::TypeUse { subject: SemanticOperand::Value(value), .. } = site.action else { continue; };
                    if value != source_operand { continue; }
                    let definition = self.inventory.definition(instance.definition, b)?;
                    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
                        return Err(fail("call argument has no complete original function"));
                    };
                    for link in site_nodes {
                        b.spend(nodes.len().saturating_add(1), WORK)?;
                        if link.site == site.recipe && matches!(link.node, SemanticOriginalNode::Expression(node) if nodes.contains(&node)) {
                            return Ok(true);
                        }
                    }
                }
            }
            let value::Class::Reborrow(owner, value) = source else {
                return Ok(false);
            };
            if owner != function.id {
                return Ok(false);
            }
            if step == b.partial_depth {
                return Err(unknown(
                    "actual call region source ancestry exceeds its complete depth",
                ));
            }
            let (block, index, instruction) = self.defining(function, value, b)?;
            let InstructionKind::ReborrowAddress { address, .. } = instruction.kind else {
                return Err(fail("call region source does not name an actual reborrow"));
            };
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                address,
                b,
            )?;
            source_value = self.ssa_value(instance, function, address, b, depth + 1)?;
            source_operand = address;
        }
        Err(unknown(
            "actual call region source ancestry exceeds its complete depth",
        ))
    }
}

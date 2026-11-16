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
        if self.outer_call_role_direct(instance, function, at, ordinal, operand, actual, expected, b, depth + 1)? {
            return Ok(true);
        }
        // Source sequencing may stage a real argument through initialized
        // locals. Follow every actual reaching Store, preserving the complete
        // value, rather than requiring a new original node for a compiler Load.
        let origins = self.call_operand_origins(function, operand, b)?;
        if origins.len() == 1 && origins[0] == operand { return Ok(false); }
        for source in origins {
            let value = self.ssa_value(instance, function, source, b, depth + 1)?;
            if !self.capture_values_equal(&value, actual, b)?
                || !self.outer_call_role_direct(instance, function, at, ordinal, source, &value, expected, b, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn outer_call_role_direct(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        at: (BlockId, usize), ordinal: usize, operand: ValueId,
        actual: &Value, expected: &Value, b: &mut Budget, depth: usize,
    ) -> CompileResult<bool> {
        if depth > b.partial_depth { return Err(unknown("actual call operand transport exceeds its complete depth")); }
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

impl Check<'_, '_> {
    fn call_operand_origins(&self, function: &TypedFunction, operand: ValueId,
        b: &mut Budget) -> CompileResult<Vec<ValueId>> {
        let initialized = crate::shared_initialization(function, b)?;
        let mut values = vec![operand];
        let mut seen_values = BTreeSet::new();
        let mut seen_locals = BTreeSet::new();
        let mut origins = BTreeSet::new();
        while let Some(value) = values.pop() {
            b.lookup(seen_values.len(), WORK)?;
            if !seen_values.insert(value) { continue; }
            let (block, instruction, producer) = self.defining(function, value, b)?;
            let local = match producer.kind {
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => local,
                InstructionKind::ReborrowAddress { address, .. } => {
                    operations::available_before(function, (block, instruction, SemanticSiteEdge::Before), address, b)?;
                    self.push_value(&mut values, address, b)?;
                    continue;
                }
                _ => { b.lookup(origins.len(), WORK)?; origins.insert(value); continue; }
            };
            operations::initialized_local_before(function, (block, instruction), local, b)?;
            let mut pending = vec![(block, instruction)];
            while let Some((block_id, end)) = pending.pop() {
                b.lookup(seen_locals.len(), WORK)?;
                if !seen_locals.insert((local, block_id, end)) { continue; }
                let block = function.blocks.get(block_id.0 as usize).filter(|row| row.id == block_id)
                    .ok_or_else(|| fail("call staging local has no actual block"))?;
                let mut assigned = false;
                for (index, instruction) in block.instructions.iter().enumerate().take(end).rev() {
                    b.spend(1, WORK)?;
                    match instruction.kind {
                        InstructionKind::Store { local: target, value: source } if target == local => {
                            operations::available_before(function, (block_id, index, SemanticSiteEdge::Before), source, b)?;
                            if function.values.get(source.0 as usize) != function.values.get(value.0 as usize) {
                                return Err(fail("call staging Store changes its complete physical type"));
                            }
                            self.push_value(&mut values, source, b)?;
                            assigned = true;
                            break;
                        }
                        InstructionKind::StorageLive(target) | InstructionKind::StorageDead(target)
                        | InstructionKind::TakeLocal { local: target } if target == local => {
                            return Err(fail("call staging read crosses a vacant activation"));
                        }
                        InstructionKind::BindPackageCapture { local: target, .. } if target == local => {
                            b.lookup(origins.len(), WORK)?; origins.insert(value); assigned = true; break;
                        }
                        _ => {}
                    }
                }
                if assigned { continue; }
                if block_id == function.entry {
                    if !function.parameters.contains(&local) {
                        return Err(fail("call staging read has no initialized original input"));
                    }
                    b.lookup(origins.len(), WORK)?; origins.insert(value);
                }
                for predecessor in &initialized.predecessors[block_id.0 as usize] {
                    b.spend(1, WORK)?;
                    if initialized.reachable[*predecessor] {
                        let source = &function.blocks[*predecessor];
                        self.push_value(&mut pending, (source.id, source.instructions.len()), b)?;
                    }
                }
            }
        }
        if origins.is_empty() { return Err(fail("call staging cycle has no concrete incoming source")); }
        b.spend(origins.len(), WORK)?;
        Ok(origins.into_iter().collect())
    }
}

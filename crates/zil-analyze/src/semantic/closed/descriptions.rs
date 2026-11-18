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
        self.source_description_at(instance, function, id, b, depth)?
            .ok_or_else(|| unknown("actual weak description lacks its exact original argument occurrence and owning source"))
    }
    pub(super) fn source_description_at(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Value>> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "implicit description source exceeds its finite depth",
            ));
        }
        self.defining(function, id, b)?;
        let mut selected = None;
        for site in self.inventory.sites_for_value(self.program, instance.id, id, b)? {
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
            self.checked_view_role(&original.root, &expected, &original.role, Some(original.anchor), b)?;
            let Type::Managed(payload) = &source.ty else {
                return Err(fail(
                    "implicit description source is not an original owning value",
                ));
            };
            if let original::LocalAnchor::Environment(ordinal) = original.anchor
                && original.path.is_empty()
                && self.original.captured_description(instance, ordinal, self.program, self.inventory, b, depth + 1)?
                    .is_some()
            {
                let physical = self.environment_capture(instance, function, ordinal, b)?;
                if self.capture_value_origin(function, id, b, depth + 1)? != physical
                    || !self.capture_values_equal(
                        &self.capture_whole_value(instance, function, physical, b, depth + 1)?,
                        &expected, b,
                    )?
                {
                    return Err(fail("actual description observes a different original default capture"));
                }
            } else {
                self.sealed_resource_place(
                    instance, function, id, original.anchor, &original.root,
                    source, &original.path, payload, b,
                )?;
            }
            let same_payload = match &expected.ty {
                Type::Desc(inner) => geometry::same_type(inner, payload, b)?,
                _ => false,
            };
            if !same_payload
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
        if selected.is_some() {
            return Ok(selected);
        }
        if let Some(value) = self.temporary_description_source(instance, function, id, b, depth + 1)? {
            return Ok(Some(value));
        }
        self.creator_description_source(instance, function, id, b)
    }

    fn creator_description_source(
        &self, instance: &SemanticInstance, function: &TypedFunction, id: ValueId, b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let (_, _, producer) = self.defining(function, id, b)?;
        if !matches!(producer.kind, InstructionKind::DescribeLocal { .. } | InstructionKind::DescribeCapture { .. }) {
            return Ok(None);
        }
        let mut selected = None;
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, WORK)?;
                let InstructionKind::MakeClosure { closure, captures } = &instruction.kind else { continue; };
                b.spend(captures.len(), WORK)?;
                if !captures.contains(&id) {
                    continue;
                }
                let closure = self.module.closures.get(closure.0 as usize)
                    .filter(|row| row.id == *closure)
                    .ok_or_else(|| fail("actual description creator loses its closure"))?;
                for child in self.function_worlds(closure.function, b)? {
                    if child.parent != Some(instance.id) {
                        continue;
                    }
                    for binding in self.capture_creator_bindings(child, closure, b)?.values() {
                        if !matches!((&binding.lexical.ty, &binding.stored.ty),
                            (Type::Managed(owner), Type::Desc(view)) if owner == view)
                        {
                            continue;
                        }
                        b.lookup(binding.bound_operands.len(), WORK)?;
                        if binding.bound_operands.contains(&id)
                            && selected.replace(binding.stored.copy(b)?).is_some()
                        {
                            return Err(fail("actual description selects several original capture boundaries"));
                        }
                    }
                }
            }
        }
        Ok(selected)
    }

    pub(super) fn temporary_description_source(
        &self, instance: &SemanticInstance, function: &TypedFunction, id: ValueId,
        b: &mut Budget, depth: usize,
    ) -> CompileResult<Option<Value>> {
        let (block, index, actual) = self.defining(function, id, b)?;
        let InstructionKind::DescribeLocal { local } = actual.kind else { return Ok(None); };
        let mut selected = None;
        for (call, ordinal, original) in self.original.temporary_descriptions(instance.definition, b)? {
            for site in self.inventory.sites_for_expression(self.program, instance, call, b)? {
                b.spend(1, WORK)?;
                if !matches!(site.action,
                    SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. }
                    | SemanticSiteAction::CallableCall { .. }) {
                    continue;
                }
                let SemanticSiteAddress::Typed {
                    block: call_block, instruction: call_at, edge: SemanticSiteEdge::After,
                } = site.at else { return Err(fail("temporary description needs the exact actual call boundary")); };
                let call_at = call_at as usize;
                let caller = function.blocks.get(call_block.0 as usize)
                    .filter(|row| row.id == call_block)
                    .ok_or_else(|| fail("temporary description has no actual call block"))?;
                let call_op = caller.instructions.get(call_at)
                    .ok_or_else(|| fail("temporary description has no actual call"))?;
                let operand = match &call_op.kind {
                    InstructionKind::Call { arguments, .. }
                    | InstructionKind::SuspendCall { arguments, .. }
                    | InstructionKind::ForeignCall { arguments, .. }
                    | InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallIndirect { arguments, .. } => arguments.get(ordinal),
                    InstructionKind::CallDynSlot { arguments, .. } =>
                        ordinal.checked_sub(1).and_then(|index| arguments.get(index)),
                    _ => None,
                }.copied();
                let Some(operand) = operand else {
                    return Err(fail("temporary description has no exact actual argument ordinal"));
                };
                if operand != id {
                    let (_, _, op) = self.defining(function, operand, b)?;
                    if !matches!(op.kind, InstructionKind::Load(_) | InstructionKind::TakeLocal { .. })
                        || self.call_operand_origins(function, operand, b)?.as_slice() != [id]
                    {
                        continue;
                    }
                }
                if call_block != block || index >= call_at || selected.is_some() {
                    return Err(fail("temporary description changes its unique actual call preparation"));
                }
                let original_value = self.original_subject_value(instance, function, original.producer, b)?
                    .ok_or_else(|| fail("temporary description lacks its exact original owning producer"))?;
                let store = index.checked_sub(1)
                    .ok_or_else(|| fail("temporary description lacks actual owner initialization"))?;
                let (source_block, source_index, _) = self.defining(function, original_value, b)?;
                if source_block != block || source_index >= store {
                    return Err(fail("temporary description owner is outside its actual call preparation"));
                }
                let stored = caller.instructions.get(store)
                    .ok_or_else(|| fail("temporary description has no actual owner Store"))?;
                if !matches!(stored.kind, InstructionKind::Store { local: target, value }
                    if target == local && value == original_value)
                {
                    return Err(fail("temporary description selects a different prepared owner"));
                }
                operations::available_before(function, (block, store, SemanticSiteEdge::Before), original_value, b)?;
                operations::initialized_local_before(function, (block, call_at), local, b)?;
                self.original_producer_local(function, local, (block, call_at), original_value, b)?;
                let (owner, view) = self.original.temporary_description_values(
                    instance, original, self.program, self.inventory, b, depth + 1,
                )?;
                if !matches!((&owner.ty, &view.ty), (Type::Managed(actual), Type::Desc(expected))
                    if actual == expected)
                    || function.locals.get(local.0 as usize).filter(|row| row.id == local)
                        .is_none_or(|row| row.ty != owner.ty)
                    || function.values.get(id.0 as usize) != Some(&view.ty)
                    || !self.capture_values_equal(&self.ssa_value(instance, function, original_value, b, depth + 1)?, &owner, b)?
                {
                    return Err(fail("temporary description changes the complete type or domain of its owning source"));
                }
                selected = Some(view);
            }
        }
        Ok(selected)
    }
}

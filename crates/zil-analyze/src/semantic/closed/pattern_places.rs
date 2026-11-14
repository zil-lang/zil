//! Bind original pattern names to actual storage through exact subject SSA,
//! tag/field selection and destination Store operations. Names and equal
//! physical types do not choose a destination.
use super::*;

impl Check<'_, '_> {
    fn original_subject_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        node: zil_base::NodeId,
        b: &mut Budget,
    ) -> CompileResult<Option<ValueId>> {
        let definition = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("pattern subject has no complete original body"));
        };
        let mut result = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.node != SemanticOriginalNode::Expression(node) {
                continue;
            }
            for site in &self.program.sites {
                b.spend(1, WORK)?;
                if site.instance != instance.id || site.recipe != link.site {
                    continue;
                }
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge,
                } = site.at
                else {
                    return Err(fail("actual pattern subject retains a Core site"));
                };
                let selected = match &site.action {
                    SemanticSiteAction::TypeUse {
                        subject: SemanticOperand::Value(value),
                        ..
                    } => Some(*value),
                    SemanticSiteAction::BodyCall { .. }
                    | SemanticSiteAction::Call { .. }
                    | SemanticSiteAction::CallableCall { .. }
                    | SemanticSiteAction::Construct { .. }
                        if edge == SemanticSiteEdge::After =>
                    {
                        function
                            .blocks
                            .get(block.0 as usize)
                            .filter(|item| item.id == block)
                            .and_then(|item| item.instructions.get(instruction as usize))
                            .ok_or_else(|| {
                                fail("pattern subject emission has no actual instruction")
                            })?
                            .result
                    }
                    _ => None,
                };
                if let Some(value) = selected {
                    if result.is_some_and(|old| old != value) {
                        return Err(fail("pattern subject has competing actual SSA emissions"));
                    }
                    result = Some(value);
                }
            }
        }
        Ok(result)
    }

    fn stored_pattern_value(
        &self,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<Option<LocalId>> {
        let mut result = None;
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, WORK)?;
                if let InstructionKind::Store {
                    local,
                    value: stored,
                } = instruction.kind
                    && stored == value
                {
                    if result.is_some_and(|old| old != local) {
                        return Err(unknown(
                            "pattern SSA is stored into several distinct actual heads",
                        ));
                    }
                    result = Some(local);
                }
            }
        }
        Ok(result)
    }

    pub(super) fn pattern_local(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        ordinal: u32,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<LocalId> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("actual pattern origin exceeds its finite ancestry"));
        }
        let origin = self
            .original
            .pattern_origin(ordinal, instance.definition, b)?;
        b.spend(
            origin
                .subject
                .path
                .len()
                .saturating_add(origin.path.len() + 1),
            WORK,
        )?;
        let mut path = Vec::new();
        path.try_reserve(origin.subject.path.len().saturating_add(origin.path.len()))
            .map_err(|_| fail("actual pattern path allocation failed"))?;
        for field in &origin.subject.path {
            path.push(PlaceProjection::Field(*field));
        }
        for step in &origin.path {
            path.push(*step);
        }
        let anchored = origin
            .subject
            .anchor
            .map(|anchor| self.anchored_local_at(instance, function, anchor, b, depth + 1))
            .transpose()?;
        let emitted = self.original_subject_value(instance, function, origin.subject.node, b)?;
        let root = if let Some(value) = emitted {
            let (_, _, producer) = self.defining(function, value, b)?;
            if let Some(anchor) = anchored {
                // A source name/field still has to read the original root;
                // a forged same-typed SSA cannot provide another subject.
                let valid = match &producer.kind {
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        origin.subject.path.is_empty() && *local == anchor
                    }
                    InstructionKind::TakeTupleField {
                        local,
                        path: selected,
                    }
                    | InstructionKind::TakeVariantField {
                        local,
                        path: selected,
                    } => {
                        *local == anchor
                            && selected.iter().copied().eq(origin
                                .subject
                                .path
                                .iter()
                                .copied()
                                .map(PlaceProjection::Field))
                    }
                    _ => false,
                };
                if !valid {
                    return Err(fail(
                        "pattern subject reads another original root or field path",
                    ));
                }
            }
            if let Some(local) = self.stored_pattern_value(function, value, b)? {
                // A stored field-value is a new whole temporary. Its source
                // field prefix has already been checked above.
                if !origin.subject.path.is_empty() {
                    b.spend(origin.subject.path.len(), WORK)?;
                    drop(path.drain(..origin.subject.path.len()));
                }
                local
            } else {
                anchored
                    .ok_or_else(|| unknown("pattern value lacks its actual storage temporary"))?
            }
        } else {
            anchored.ok_or_else(|| unknown("pattern subject has no actual root or emission"))?
        };
        let mut selected = None;
        let mut accept = |local| -> CompileResult<()> {
            if selected.is_some_and(|old| old != local) {
                return Err(unknown(
                    "original pattern has several distinct extraction destinations",
                ));
            }
            selected = Some(local);
            Ok(())
        };
        if path.is_empty()
            && let Some(value) = emitted
            && let Some(local) = self.stored_pattern_value(function, value, b)?
        {
            accept(local)?;
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, WORK)?;
                match &instruction.kind {
                    InstructionKind::TakeVariantField {
                        local,
                        path: actual,
                    }
                    | InstructionKind::TakeTupleField {
                        local,
                        path: actual,
                    } if *local == root => {
                        b.spend(actual.len().saturating_add(path.len() + 1), WORK)?;
                        if *actual == path {
                            let value = instruction.result.ok_or_else(|| {
                                fail("actual pattern extraction has no SSA result")
                            })?;
                            if let Some(local) = self.stored_pattern_value(function, value, b)? {
                                accept(local)?;
                            }
                        }
                    }
                    InstructionKind::UnpackTuple { local, fields } if *local == root => {
                        if let [PlaceProjection::Field(index)] = path.as_slice()
                            && let Some(local) = fields.get(*index as usize)
                        {
                            b.spend(fields.len(), WORK)?;
                            accept(*local)?;
                        }
                    }
                    InstructionKind::UnpackVariant {
                        local,
                        variant,
                        fields,
                    } if *local == root => {
                        let index = match path.as_slice() {
                            [PlaceProjection::Variant(step)] if step.variant == *variant => {
                                Some(step.index)
                            }
                            [PlaceProjection::ErrorArm(error)] if u32::from(*error) == *variant => {
                                Some(0)
                            }
                            _ => None,
                        };
                        if let Some(index) = index
                            && let Some(local) = fields.get(index as usize)
                        {
                            b.spend(fields.len(), WORK)?;
                            accept(*local)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        selected.ok_or_else(|| {
            unknown("pattern binding lacks its exact actual selection and destination store")
        })
    }
}

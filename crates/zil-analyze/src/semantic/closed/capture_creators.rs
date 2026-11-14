//! Original lexical values are bound at each real environment construction.
//! Navigation need not fabricate an executable whole-name read in the child.
use super::*;
use crate::semantic::value::Value;
impl Check<'_, '_> {
    pub(super) fn capture_creator_bindings(
        &self,
        instance: &SemanticInstance,
        closure: &ClosureDefinition,
        b: &mut Budget,
    ) -> CompileResult<BTreeMap<u32, (u32, Value)>> {
        let definition = self.inventory.definition(instance.definition, b)?;
        let parent = self.inventory.instance(
            instance
                .parent
                .ok_or_else(|| fail("captured environment has no real creator parent"))?,
            b,
        )?;
        let SemanticInstanceTarget::Function(target) = &parent.target else {
            return Err(fail(
                "captured environment parent is not an actual function",
            ));
        };
        let function = self.inventory.function(target, b)?;
        let environment = instance
            .environment
            .as_ref()
            .ok_or_else(|| fail("captured environment has no sealed actual values"))?;
        let mut result = BTreeMap::new();
        for (ordinal, formal) in definition.environment.iter().enumerate() {
            b.spend(1, WORK)?;
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| fail("original environment ordinal overflow"))?;
            let Some(anchor) = self.original.capture_origin(definition.id, ordinal, b)? else {
                continue;
            };
            let Some(SemanticActual::Type(actual)) = environment.actuals.get(ordinal as usize)
            else {
                return Err(fail("runtime capture changes its original value kind"));
            };
            crate::dyn_metadata::recipe(&formal.source, b)?;
            let expected = self.resolved(parent, actual, b, 0)?;
            let mut selected = None;
            let mut creators = 0usize;
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    b.spend(1, WORK)?;
                    let InstructionKind::MakeClosure {
                        closure: id,
                        captures,
                    } = &instruction.kind
                    else {
                        continue;
                    };
                    if *id != closure.id {
                        continue;
                    }
                    creators += 1;
                    if captures.len() != closure.captures.len() {
                        return Err(fail(
                            "actual environment construction loses its whole capture vector",
                        ));
                    }
                    let mut this = None;
                    for (capture, operand) in captures.iter().enumerate() {
                        if !self.capture_matches(parent, function, *operand, anchor, b, 0)? {
                            continue;
                        }
                        operations::available_before(
                            function,
                            (block.id, index, SemanticSiteEdge::Before),
                            *operand,
                            b,
                        )?;
                        let value = self.ssa_value(parent, function, *operand, b, 0)?;
                        if value != expected || closure.captures.get(capture) != Some(&expected.ty)
                        {
                            return Err(fail(
                                "actual creator changes its captured lexical value or intrinsic class image",
                            ));
                        }
                        let capture = u32::try_from(capture)
                            .map_err(|_| fail("physical capture ordinal overflow"))?;
                        if this.replace(capture).is_some() {
                            return Err(fail(
                                "original captured binding occurs twice in one actual environment",
                            ));
                        }
                    }
                    let capture = this.ok_or_else(|| {
                        fail("actual creator omits its original captured lexical binding")
                    })?;
                    if selected.is_some_and(|old| old != capture) {
                        return Err(fail(
                            "actual creators disagree on the original environment binding",
                        ));
                    }
                    selected = Some(capture);
                }
            }
            if creators == 0 {
                return Err(fail(
                    "captured value has no actual environment construction in its real parent",
                ));
            }
            b.lookup(result.len(), WORK)?;
            result.insert(
                ordinal,
                (
                    selected
                        .ok_or_else(|| fail("captured value has no physical original operand"))?,
                    expected,
                ),
            );
        }
        Ok(result)
    }

    fn capture_matches(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        value: ValueId,
        anchor: original::LocalAnchor,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "captured lexical source exceeds its finite actual chain",
            ));
        }
        let (_, _, instruction) = self.defining(function, value, b)?;
        if let InstructionKind::ReborrowAddress { address, .. } = instruction.kind {
            return self.capture_matches(instance, function, address, anchor, b, depth + 1);
        }
        match anchor {
            original::LocalAnchor::Environment(ordinal) => {
                let expected = self.environment_capture(instance, function, ordinal, b)?;
                Ok(
                    matches!(instruction.kind,InstructionKind::ReadCapture {index}|InstructionKind::TakeCapture {index}|InstructionKind::BorrowCapture {index,..} if index==expected),
                )
            }
            anchor => {
                let expected = self.anchored_local(instance, function, anchor, b)?;
                Ok(
                    matches!(instruction.kind,InstructionKind::Load(local)|InstructionKind::TakeLocal {local}|InstructionKind::BorrowLocal {local}|InstructionKind::BorrowLocalMut {local} if local==expected),
                )
            }
        }
    }
}

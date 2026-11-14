//! Actual Task operations retain their original worker and owning-head identity.
//! Phase, allocation failure, retained loans and cleanup remain Task-kernel facts.
use super::*;

impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn task_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        intrinsic: original::sealed::Intrinsic,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<Type> {
        if !actuals.is_empty() {
            return Err(fail("actual Task intrinsic invents generic actuals"));
        }
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("original Task call has no actual operation"))?;
        let inputs =
            self.original
                .intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        let arguments: &[ValueId] = match (intrinsic, &instruction.kind) {
            (
                original::sealed::Intrinsic::TaskCreate,
                InstructionKind::TaskCreate {
                    worker, arguments, ..
                },
            ) => {
                let targets = self
                    .original
                    .native_targets(instance, site, self.inventory, b)?;
                if targets.as_slice() != [self.function_instance(*worker, b)?.definition] {
                    return Err(fail(
                        "actual Task creator substitutes another original worker",
                    ));
                }
                // The referenced worker's whole original declaration and its
                // actual header are checked independently, even when unused.
                // Referencing it here does not execute its Suspend body.
                arguments
            }
            (original::sealed::Intrinsic::TaskCreate, InstructionKind::TaskCreateOwned { .. }) => {
                return Err(unknown(
                    "owning Task allocation failure needs the original complete input-return carrier",
                ));
            }
            (original::sealed::Intrinsic::TaskPoll, InstructionKind::TaskPoll { task })
            | (
                original::sealed::Intrinsic::TaskCancel,
                InstructionKind::TaskRequestCancel { task },
            )
            | (
                original::sealed::Intrinsic::TaskPollClose,
                InstructionKind::TaskPollClose { task },
            )
            | (
                original::sealed::Intrinsic::TaskFinish,
                InstructionKind::TaskFinishDestroy { task },
            ) => {
                let anchor = self
                    .original
                    .intrinsic_anchor(instance, site, self.inventory, b)?;
                self.task_head(instance, function, *task, anchor, b, 0)?;
                std::slice::from_ref(task)
            }
            _ => {
                return Err(fail(
                    "actual Task opcode replaces another original sealed operation",
                ));
            }
        };
        if arguments.len() != inputs.len() {
            return Err(fail(
                "actual Task operation changes its complete ordered inputs",
            ));
        }
        for (argument, expected) in arguments.iter().zip(&inputs) {
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                *argument,
                b,
            )?;
            if function.values.get(argument.0 as usize) != Some(&expected.ty) {
                return Err(fail(
                    "actual Task input changes its complete original carrier",
                ));
            }
            if self.uses_full_value(&expected.ty, b)? {
                let value = self.ssa_value(instance, function, *argument, b, 0)?;
                let equal = if matches!(expected.ty, Type::Address { .. }) {
                    self.outer_role(instance, function, &value, expected, b, 0)?
                } else {
                    value == *expected
                };
                if !equal {
                    return Err(fail(
                        "actual Task input changes its original whole class role",
                    ));
                }
            }
            self.actual_type(&expected.ty, b)?;
        }
        let output =
            self.original
                .intrinsic_output(instance, site, self.program, self.inventory, b)?;
        self.actual_type(&output.ty, b)?;
        Ok(output.ty)
    }

    pub(super) fn task_head(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        value: ValueId,
        anchor: original::LocalAnchor,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual Task owning-head chain exceeds its finite depth",
            ));
        }
        let (_, _, instruction) = self.defining(function, value, b)?;
        if let InstructionKind::ReborrowAddress { address, .. } = instruction.kind {
            return self.task_head(instance, function, address, anchor, b, depth + 1);
        }
        match anchor {
            original::LocalAnchor::Environment(ordinal) => {
                let capture = self.environment_capture(instance, function, ordinal, b)?;
                if matches!(instruction.kind,
                    InstructionKind::TakeCapture { index }
                    | InstructionKind::ReadCapture { index }
                    | InstructionKind::BorrowCapture { index, .. } if index == capture)
                {
                    return Ok(());
                }
            }
            anchor => {
                let local = self.anchored_local(instance, function, anchor, b)?;
                if matches!(instruction.kind,
                    InstructionKind::Load(actual)
                    | InstructionKind::TakeLocal { local: actual }
                    | InstructionKind::BorrowLocal { local: actual }
                    | InstructionKind::BorrowLocalMut { local: actual } if actual == local)
                {
                    return Ok(());
                }
            }
        }
        Err(fail(
            "actual Task operation selects another original owning head",
        ))
    }
}

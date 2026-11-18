//! Actual Task operations retain their original worker and owning-head identity.
//! Phase, allocation failure, retained loans and cleanup remain Task-kernel facts.
//! A construction BodyCall carries the worker's entire ordered mixed actual
//! vector, just like an ordinary generic target reference. Poll/Close/Cancel/
//! Finish carry their own intrinsic result-Type projection instead. These are
//! distinct interfaces even when one worker formal happens to equal its R.
use super::*;
use crate::semantic::value;

impl Check<'_, '_> {
    fn task_worker_arguments(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        worker: FunctionId,
        proposed: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<()> {
        let targets = self.original.native_targets(instance, site, self.inventory, b)?;
        let [original] = targets.as_slice() else {
            return Err(fail("Task creation has no unique independently checked original worker"));
        };
        let expected = self.original.task_worker_arguments(instance, site, self.program, self.inventory, b)?;
        let formals = &self.inventory.definition(*original, b)?.parameters;
        let worlds = self.function_worlds(worker, b)?;
        if worlds.is_empty() {
            return Err(fail("Task worker has no complete original target world"));
        }
        for target in worlds {
            if target.definition != *original || proposed.len() != expected.len()
                || target.actuals.len() != expected.len() || formals.len() != expected.len()
            {
                return Err(fail("Task worker changes its original definition or full mixed actual arity"));
            }
            for (ordinal, (((proposed, target_actual), expected), formal)) in proposed.iter()
                .zip(&target.actuals).zip(&expected).zip(formals).enumerate()
            {
                let proposed = self.actual_value(instance, proposed, b, 0)?;
                if !self.capture_arguments_equal(&proposed, expected, b)? {
                    let mut errors = fail("Task worker actual differs from original caller-domain mixed inference");
                    errors[0].notes.push(format!("worker function {}, full ordinal {}, original {:?}, proposed {:?}",
                        worker.0, ordinal, expected, proposed));
                    return Err(errors);
                }
                let actual = self.actual_value(target, target_actual, b, 0)?;
                if !self.body_call_actual(target, ordinal, formal, expected, &actual, b)? {
                    let mut errors = fail("Task worker changes its independent original rigid mixed context");
                    errors[0].notes.push(format!("worker function {}, original world {:?}, full ordinal {}, caller {:?}, target {:?}",
                        worker.0, target.id, ordinal, expected, actual));
                    return Err(errors);
                }
            }
        }
        Ok(())
    }

    pub(super) fn task_result(&self, instance: &SemanticInstance, function: &TypedFunction,
        block: BlockId, index: usize, b: &mut Budget) -> CompileResult<value::Value> {
        let operation = function.blocks.get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|body| body.instructions.get(index))
            .ok_or_else(|| fail("original Task result has no actual instruction"))?;
        let mut selected = None;
        for site in self.inventory.sites_at(self.program, instance.id, block, index, SemanticSiteEdge::After, b)? {
            let SemanticSiteAction::BodyCall { actuals } = &site.action else { continue; };
            let intrinsic = self.original.intrinsic(instance, site.recipe, self.inventory, b)?
                .ok_or_else(|| fail("Task result has no original intrinsic scheme"))?;
            if !matches!(intrinsic, original::sealed::Intrinsic::TaskCreate | original::sealed::Intrinsic::TaskCreateOwned
                | original::sealed::Intrinsic::TaskPoll | original::sealed::Intrinsic::TaskPollClose) {
                return Err(fail("Task result replaces another original intrinsic"));
            }
            let ty = self.task_operation(instance, site.recipe, intrinsic, function, block, index, actuals, b)?;
            self.result(operation, function, &ty)?;
            let output = self.original.intrinsic_output(instance, site.recipe, self.program, self.inventory, b)?;
            if !geometry::same_type(&ty, &output.ty, b)? || selected.replace(output).is_some() {
                return Err(fail("Task result has another complete original carrier or duplicate creator"));
            }
        }
        selected.ok_or_else(|| fail("Task result lacks its exact original creator receipt"))
    }

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
        if !matches!(intrinsic, original::sealed::Intrinsic::TaskCreate | original::sealed::Intrinsic::TaskCreateOwned) {
            let expected_type =
                self.original
                    .intrinsic_type_actual(instance, site, self.program, self.inventory, b)?;
            match actuals {
                [] => {
                    // This intrinsic has no proposed Type projection. Its
                    // full inputs/result are reconstructed independently.
                }
                [actual_type] => {
                    if !matches!(self.actual_value(instance, actual_type, b, 0)?, value::Argument::Type(actual) if actual == expected_type)
                    {
                        return Err(fail(
                            "sealed resource Type actual changes its independently checked original scheme",
                        ));
                    }
                }
                _ => {
                    return Err(fail(
                        "sealed resource operation changes its intrinsic Type actual arity",
                    ));
                }
            }
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
            )
            | (
                original::sealed::Intrinsic::TaskCreateOwned,
                InstructionKind::TaskCreateOwned { worker, arguments, .. },
            ) => {
                self.task_worker_arguments(instance, site, *worker, actuals, b)?;
                // The referenced worker's whole original declaration and its
                // actual header are checked independently, even when unused.
                // Referencing it here does not execute its Suspend body.
                arguments
            }
            (original::sealed::Intrinsic::TaskCreate, InstructionKind::TaskCreateOwned { .. })
            | (original::sealed::Intrinsic::TaskCreateOwned, InstructionKind::TaskCreate { .. }) => {
                return Err(fail("actual Task creation changes its original input-return failure contract"));
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
        for (ordinal, (argument, expected)) in arguments.iter().zip(&inputs).enumerate() {
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
                let equal = if matches!(expected.ty, Type::Address { .. } | Type::Desc(_)) {
                    self.outer_call_role(instance, function, (block, index), ordinal, *argument, &value, expected, b, 0)?
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
        output.into_type(b)
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
        // Captured references are real local occupants: BorrowCapture may be
        // stored, loaded and reborrowed before any Task/Mutex operation. Use
        // the complete all-path capture provenance proof also used by other
        // original receivers, preserving the exact capture and activation.
        // Complete types/classes and the actual loan/cleanup ledger are
        // independent mandatory checks at each caller of this identity query.
        if self.anchored_value(instance, function, anchor, value, b, depth + 1)? {
            return Ok(());
        }
        Err(fail(
            "actual Task operation selects another original owning head",
        ))
    }
}

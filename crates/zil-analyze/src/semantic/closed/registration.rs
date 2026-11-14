//! Bind an original registration operation to its exact actual source head.
//! Class coincidence does not supply object identity or retirement permission.
use super::*;
impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn registration_operation(
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
            return Err(fail(
                "actual registration operation invents generic actuals",
            ));
        }
        if matches!(
            intrinsic,
            original::sealed::Intrinsic::AsyncValid
                | original::sealed::Intrinsic::RegistrationValid
        ) {
            return self
                .registration_observation(instance, site, intrinsic, function, block, index, b);
        }
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("original registration operation has no actual instruction"))?;
        let input = match (&intrinsic, &instruction.kind) {
            (
                original::sealed::Intrinsic::RegistrationClose,
                InstructionKind::RegistrationClose { registration },
            ) => *registration,

            (
                original::sealed::Intrinsic::AsyncRequest,
                InstructionKind::AsyncRegistrationRequestClose { registration },
            )
            | (
                original::sealed::Intrinsic::AsyncPoll,
                InstructionKind::AsyncRegistrationPollClose { registration },
            )
            | (
                original::sealed::Intrinsic::AsyncFinish,
                InstructionKind::AsyncRegistrationFinishDestroy { registration },
            )
            | (
                original::sealed::Intrinsic::AsyncFinishChecked,
                InstructionKind::AsyncRegistrationFinishDestroyChecked { registration },
            ) => *registration,
            _ => {
                return Err(fail(
                    "actual registration operation differs from its original sealed identity",
                ));
            }
        };
        operations::available_before(function, (block, index, SemanticSiteEdge::Before), input, b)?;
        let expected =
            self.original
                .intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        let [expected] = expected.as_slice() else {
            return Err(fail(
                "original registration operation loses its whole input",
            ));
        };
        if function.values.get(input.0 as usize) != Some(&expected.ty) {
            return Err(fail(
                "actual registration operation changes the complete owning head or reference category",
            ));
        }
        let anchor = self
            .original
            .intrinsic_anchor(instance, site, self.inventory, b)?;
        let local = self.anchored_local(instance, function, anchor, b)?;
        if self.receiver_local(function, input, b, 0)? != local {
            return Err(fail(
                "actual registration operation selects another original parameter or binding head",
            ));
        }
        if matches!(expected.ty, Type::Address { .. }) {
            let actual = self.ssa_value(instance, function, input, b, 0)?;
            if !self.outer_role(instance, function, &actual, expected, b, 0)? {
                return Err(fail(
                    "actual registration operation changes its complete original input class role",
                ));
            }
        } else if self.uses_full_value(&expected.ty, b)? {
            if self.ssa_value(instance, function, input, b, 0)? != *expected {
                return Err(fail(
                    "actual registration operation changes its complete input scope",
                ));
            }
        }
        self.actual_type(&expected.ty, b)?;
        let output =
            self.original
                .intrinsic_output(instance, site, self.program, self.inventory, b)?;
        self.actual_type(&output.ty, b)?;
        Ok(output.ty)
    }
}

//! Bind checked original provider roles and the consumed callback to this
//! actual installation. The established registration kernel independently
//! proves raw-entry drain, stable storage, cleanup and thread capabilities.
use super::*;
impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn registration_start_operation(
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
                "actual registration installation adds generic role arguments",
            ));
        }
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("actual registration installation has no instruction"))?;
        let (providers, callback) = match (&intrinsic, &instruction.kind) {
            (
                original::sealed::Intrinsic::Register,
                InstructionKind::RegistrationStart {
                    register,
                    close,
                    callback,
                    ..
                },
            ) => (vec![*register, *close], *callback),
            (
                original::sealed::Intrinsic::RegisterAsync,
                InstructionKind::AsyncRegistrationStart {
                    register,
                    request_close,
                    poll_drain,
                    callback,
                    ..
                },
            ) => (vec![*register, *request_close, *poll_drain], *callback),
            _ => {
                return Err(fail(
                    "actual registration installation changes the original sealed opcode",
                ));
            }
        };
        b.spend(providers.len(), WORK)?;
        let targets = self
            .original
            .native_targets(instance, site, self.inventory, b)?;
        if providers.len() != targets.len() {
            return Err(fail(
                "actual registration installation changes its complete provider vector",
            ));
        }
        for (provider, expected) in providers.into_iter().zip(targets) {
            if self.function_instance(provider, b)?.definition != expected {
                return Err(fail(
                    "actual registration installation substitutes another original foreign provider",
                ));
            }
        }
        operations::available_before(
            function,
            (block, index, SemanticSiteEdge::Before),
            callback,
            b,
        )?;
        let inputs =
            self.original
                .intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        let [expected] = inputs.as_slice() else {
            return Err(fail(
                "original registration installation loses its entire consumed callback",
            ));
        };
        let anchor = self
            .original
            .intrinsic_anchor(instance, site, self.inventory, b)?;
        let local = self.anchored_local(instance, function, anchor, b)?;
        if self.receiver_local(function, callback, b, 0)? != local {
            return Err(fail(
                "actual registration installation consumes another original callback operand",
            ));
        }
        if function.values.get(callback.0 as usize) != Some(&expected.ty)
            || self.ssa_value(instance, function, callback, b, 0)? != *expected
        {
            return Err(fail(
                "actual registration installation changes the complete original callback domain",
            ));
        }
        let Type::Closure(id) = expected.ty else {
            return Err(fail(
                "actual registration installation has no concrete callable environment",
            ));
        };
        self.capture_directory(id, b)?;
        self.actual_type(&expected.ty, b)?;
        let output =
            self.original
                .intrinsic_output(instance, site, self.program, self.inventory, b)?;
        self.actual_type(&output.ty, b)?;
        Ok(output.ty)
    }
}

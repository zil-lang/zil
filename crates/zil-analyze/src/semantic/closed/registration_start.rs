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
            if self.function_definition(provider, b)? != expected {
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
        match actuals {
            [] => {}
            [actual] => {
                if !matches!(self.actual_value(instance, actual, b, 0)?, crate::semantic::value::Argument::Type(actual) if actual == *expected)
                {
                    return Err(fail(
                        "registration installation Type actual changes its independently checked complete callback scheme",
                    ));
                }
            }
            _ => {
                return Err(fail(
                    "registration installation changes its complete callback Type actual arity",
                ));
            }
        }
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
        output.into_type(b)
    }
}

impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn scoped_foreign_callback_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        instruction: &Instruction,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<()> {
        let InstructionKind::WithForeignCallback { register, close, callback, body, .. } =
            instruction.kind
        else {
            return Err(fail("actual scoped callback changes the original sealed opcode"));
        };
        if !actuals.is_empty() {
            return Err(fail("actual scoped callback carries unexpected generic actuals"));
        }
        let targets = self.original.native_targets(instance, site, self.inventory, b)?;
        if targets.as_slice() != [self.function_definition(register, b)?, self.function_definition(close, b)?] {
            return Err(fail("actual scoped callback substitutes an original provider"));
        }
        let nodes = self.original.scoped_foreign_operands(instance, site, self.inventory, b)?;
        let inputs = self.original.intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        let [expected_callback, expected_body] = inputs.as_slice() else {
            return Err(fail("actual scoped callback loses its complete original input scheme"));
        };
        for (node, operand, expected) in [
            (nodes[0], callback, expected_callback),
            (nodes[1], body, expected_body),
        ] {
            operations::available_before(function, (block, index, SemanticSiteEdge::Before), operand, b)?;
            let actual = self.ssa_value(instance, function, operand, b, 0)?;
            if !self.capture_values_equal(&actual, expected, b)? {
                return Err(fail("actual scoped callback changes a complete original operand"));
            }
            let source = self.original_subject_value(instance, function, node, b)?;
            let anchor = self.original.local_read_anchor(instance.definition, node, b)?;
            if let Some(anchor) = anchor
                && !self.anchored_value(instance, function, anchor, source.unwrap_or(operand), b, 0)?
            {
                return Err(fail("actual scoped callback uses a different original lexical binding"));
            }
            if let Some(source) = source {
                self.original_producer_value(function, operand, source, b)?;
            } else if anchor.is_none() {
                return Err(fail("actual scoped callback operand has no exact original producer"));
            }
        }

        // Source identity and complete values are checked above. The scoped
        // kernel independently verifies the actual callback/body ABIs, unsafe
        // region and exclusive live loan during the body.
        let output = self.original.intrinsic_output(instance, site, self.program, self.inventory, b)?;
        self.actual_type(&output.ty, b)?;
        self.result(instruction, function, &output.ty)
    }
}

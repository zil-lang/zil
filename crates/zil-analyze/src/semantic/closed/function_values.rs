//! Original code identities and indirect operand boundaries. A matching
//! pointer ABI alone never selects an original referenced definition.
use super::*;
use crate::semantic::value::{self, Value};
impl Check<'_, '_> {
    pub(super) fn function_reference(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        subject: &SemanticOperand,
        expected: &Type,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let Some(original) = self
            .original
            .function_reference(instance, site, self.inventory, b)?
        else {
            return Ok(());
        };
        let SemanticOperand::Value(value) = subject else {
            return Err(fail(
                "original function reference has no actual emitted code value",
            ));
        };
        let (_, _, instruction) = self.defining(function, *value, b)?;
        let target = match (expected, &instruction.kind) {
            (Type::FunctionPointer { .. }, InstructionKind::FunctionRef { function })
            | (Type::ForeignCallback { .. }, InstructionKind::ForeignFunctionRef { function }) => {
                function
            }
            _ => {
                return Err(fail(
                    "original code reference changes its complete pointer family or actual opcode",
                ));
            }
        };
        let worlds = self.function_worlds(*target, b)?;
        if worlds.is_empty() {
            return Err(fail(
                "actual function reference has no complete original world",
            ));
        }
        for world in worlds {
            if world.definition != original
                || !world.actuals.is_empty()
                || world.parent.is_some()
                || world.environment.is_some()
                || world.self_type.is_some()
                || !world.header_embedding.is_empty()
            {
                return Err(fail(
                    "actual function reference selects another original specialization or environment",
                ));
            }
        }
        let actual = self
            .inventory
            .functions
            .get(target)
            .ok_or_else(|| fail("actual function reference selects no real function"))?;
        let (parameters, result) = match expected {
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => (parameters, result),
            _ => {
                return Err(fail(
                    "original thin code value changes its complete code family",
                ));
            }
        };
        if actual.call_kind != CallKind::Direct
            || actual.foreign.is_some()
            || actual.environment.is_some()
            || actual.return_type != **result
            || actual.parameters.len() != parameters.len()
            || !matches!(actual.result_mode, PassingMode::Infer | PassingMode::Copy)
        {
            return Err(fail(
                "actual function reference changes its original fixed code ABI",
            ));
        }
        for (local, expected) in actual.parameters.iter().zip(parameters) {
            let actual = actual
                .locals
                .get(local.0 as usize)
                .filter(|decl| decl.id == *local)
                .ok_or_else(|| fail("actual code parameter has no real local"))?;
            crate::dyn_metadata::ty(&actual.ty, b)?;
            crate::dyn_metadata::ty(expected, b)?;
            if actual.ty != *expected {
                return Err(fail(
                    "actual function reference changes its complete original parameter or mode",
                ));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn code_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let Some((anchor, code)) =
            self.original
                .code_boundary(instance, site, self.program, self.inventory, b)?
        else {
            return Ok(None);
        };
        if !actuals.is_empty() {
            return Err(fail("original indirect code call invents generic actuals"));
        }
        let Type::FunctionPointer { parameters, result } = &code.ty else {
            return Err(fail("original indirect call loses its complete code type"));
        };
        if !empty(&code.life) || !code.classes.is_empty() {
            return Err(unknown(
                "indirect code call needs its full persistent class boundary",
            ));
        }
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("original indirect call has no actual instruction"))?;
        let InstructionKind::CallIndirect { callee, arguments } = &instruction.kind else {
            return Err(fail(
                "original indirect call is replaced by another actual opcode",
            ));
        };
        let local = self.anchored_local(instance, function, anchor, b)?;
        operations::available_before(
            function,
            (block, index, SemanticSiteEdge::Before),
            *callee,
            b,
        )?;
        if self.receiver_local(function, *callee, b, 0)? != local
            || function.values.get(callee.0 as usize) != Some(&code.ty)
            || parameters.len() != arguments.len()
        {
            return Err(fail(
                "actual indirect call selects another original code operand or arity",
            ));
        }
        for (argument, expected) in arguments.iter().zip(parameters) {
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                *argument,
                b,
            )?;
            crate::dyn_metadata::ty(expected, b)?;
            if function.values.get(argument.0 as usize) != Some(expected) {
                return Err(fail(
                    "actual indirect argument changes its complete original type",
                ));
            }
            self.zero_data(expected, b, 0)?;
        }
        self.zero_data(result, b, 0)?;
        self.result(instruction, function, result)?;
        crate::dyn_metadata::ty(result, b)?;
        Ok(Some(value::scalar(result.as_ref().clone())))
    }
}

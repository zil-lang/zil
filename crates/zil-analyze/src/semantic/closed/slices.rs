//! Bind the original slice-length member scheme to its actual reference read.
//! A same-typed sibling local is not the checked original receiver.
use super::*;
impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn slice_length_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !actuals.is_empty() {
            return Err(fail("actual slice length invents generic arguments"));
        }
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("original slice length has no actual instruction"))?;
        let InstructionKind::SliceLength { address } = instruction.kind else {
            return Err(fail("original slice length changes its actual opcode"));
        };
        operations::available_before(
            function,
            (block, index, SemanticSiteEdge::Before),
            address,
            b,
        )?;
        let expected =
            self.original
                .intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        let [expected] = expected.as_slice() else {
            return Err(fail("original slice length loses its complete receiver"));
        };
        if !matches!(&expected.ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Slice { .. }))
            || function.values.get(address.0 as usize) != Some(&expected.ty)
        {
            return Err(fail(
                "actual slice length changes its whole borrowed-slice type",
            ));
        }
        let anchor = self
            .original
            .intrinsic_anchor(instance, site, self.inventory, b)?;
        let local = self.anchored_local(instance, function, anchor, b)?;
        if self.receiver_local(function, address, b, 0)? != local {
            return Err(fail(
                "actual slice length reads another original reference binder",
            ));
        }
        let actual = self.ssa_value(instance, function, address, b, 0)?;
        if !self.outer_role(instance, function, &actual, expected, b, 0)? {
            return Err(fail(
                "actual slice length changes the original complete reference role",
            ));
        }
        self.result(instruction, function, &Type::I64)
    }
}

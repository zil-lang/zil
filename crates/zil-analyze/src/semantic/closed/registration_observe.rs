//! Readonly registration observation may form a weak description of the exact
//! original owning head. It never upgrades a description to owning authority.
use super::*;
use crate::semantic::value;
impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn registration_observation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        intrinsic: original::sealed::Intrinsic,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Type> {
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("original registration observation has no actual instruction"))?;
        let desc = match (&intrinsic, &instruction.kind) {
            (
                original::sealed::Intrinsic::RegistrationValid,
                InstructionKind::RegistrationValid { registration },
            )
            | (
                original::sealed::Intrinsic::AsyncValid,
                InstructionKind::AsyncRegistrationValid { registration },
            )
            | (
                original::sealed::Intrinsic::TaskValid,
                InstructionKind::TaskValid { task: registration },
            ) => *registration,
            _ => {
                return Err(fail(
                    "actual registration observation changes the original sealed operation",
                ));
            }
        };
        operations::available_before(function, (block, index, SemanticSiteEdge::Before), desc, b)?;
        let inputs =
            self.original
                .intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        let [expected] = inputs.as_slice() else {
            return Err(fail(
                "original registration observation loses its complete head",
            ));
        };
        let anchor = self
            .original
            .intrinsic_anchor(instance, site, self.inventory, b)?;
        let selected = match &expected.ty {
            Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(_)) =>
                value::pointee(expected, self.inventory, b)?,
            _ => expected.copy(b)?,
        };
        let payload = match &selected.ty {
            Type::Managed(payload) | Type::Desc(payload) => payload,
            _ => return Err(fail("original observation loses its exact owning/description category")),
        };
        // This is the same independently checked original place relation used
        // by implicit descriptions. It follows every actual descriptor temp
        // Store/Load and the exact original Capture/Binding/Parameter anchor;
        // equal payload Types never select another owner or physical capture.
        self.sealed_resource_place(instance, function, desc, anchor, expected,
            &selected, &[], payload, b)?;
        crate::dyn_metadata::ty(payload, b)?;
        if function.values.get(desc.0 as usize) != Some(&Type::Desc(payload.clone())) {
            return Err(fail(
                "actual observation changes its exact weak payload identity",
            ));
        }
        self.actual_type(&expected.ty, b)?;
        Ok(Type::Bool)
    }
}

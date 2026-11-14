//! Readonly registration observation may form a weak description of the exact
//! original owning head. It never upgrades a description to owning authority.
use super::*;
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
        let local = self.anchored_local(instance, function, anchor, b)?;
        let (source_block, source_index, source) = self.defining(function, desc, b)?;
        let payload = match &expected.ty {
            Type::Desc(payload) => {
                if !matches!(source.kind, InstructionKind::Load(actual) | InstructionKind::TakeLocal { local: actual } if actual == local)
                    || self.local_value(
                        instance,
                        function,
                        local,
                        (source_block, source_index),
                        b,
                        0,
                    )? != *expected
                {
                    return Err(fail(
                        "actual registration observation loads another complete original descriptor",
                    ));
                }
                payload
            }
            Type::Managed(payload) => {
                let actual = match source.kind {
                    InstructionKind::DescribeLocal { local: actual } => {
                        if actual != local {
                            return Err(fail(
                                "actual observation describes another original owner",
                            ));
                        }
                        self.local_value(
                            instance,
                            function,
                            local,
                            (source_block, source_index),
                            b,
                            0,
                        )?
                    }
                    InstructionKind::DescribeAddress { address } => {
                        operations::available_before(
                            function,
                            (source_block, source_index, SemanticSiteEdge::Before),
                            address,
                            b,
                        )?;
                        let (at_block, at_index, actual, _) =
                            self.borrow_root(function, address, b, 0)?;
                        if actual != local {
                            return Err(fail("actual observation borrows another original owner"));
                        }
                        self.local_value(instance, function, local, (at_block, at_index), b, 0)?
                    }
                    _ => {
                        return Err(fail(
                            "actual observation has no real weak description of its original owning head",
                        ));
                    }
                };
                if actual != *expected {
                    return Err(fail(
                        "actual observation changes its complete original owner domain",
                    ));
                }
                payload
            }
            Type::Address { inner, .. } => {
                let Type::Managed(payload) = inner.as_ref() else {
                    return Err(fail(
                        "original observation address selects a descriptor slot",
                    ));
                };
                let InstructionKind::DescribeAddress { address } = source.kind else {
                    return Err(fail(
                        "actual observation does not describe its original borrowed owning head",
                    ));
                };
                operations::available_before(
                    function,
                    (source_block, source_index, SemanticSiteEdge::Before),
                    address,
                    b,
                )?;
                if self.receiver_local(function, address, b, 0)? != local {
                    return Err(fail(
                        "actual observation selects another original borrowed head",
                    ));
                }
                let actual = self.ssa_value(instance, function, address, b, 0)?;
                if !self.outer_role(instance, function, &actual, expected, b, 0)? {
                    return Err(fail(
                        "actual observation changes its complete original borrowed-head relation",
                    ));
                }
                payload
            }
            _ => {
                return Err(fail(
                    "original observation loses its owning-head or descriptor category",
                ));
            }
        };
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

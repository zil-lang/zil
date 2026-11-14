//! Bind the closed original Store scheme to the actual manager and opcode.
//! The actual Store kernel still proves prefix entries, loans and cleanup.
use super::*;
use crate::semantic::value::{self, Value};
impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn store_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        intrinsic: original::sealed::Intrinsic,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("original Store intrinsic has no actual instruction"))?;
        use original::sealed::Intrinsic as I;
        let (store, extra) = match (intrinsic, &instruction.kind) {
            (I::StoreNew, InstructionKind::OwnedStoreNew { .. }) => (None, None),
            (I::StoreReserve, InstructionKind::OwnedStoreReserve { store, capacity }) => {
                (Some(*store), Some(*capacity))
            }
            (I::StorePush, InstructionKind::OwnedStorePush { store, value }) => {
                (Some(*store), Some(*value))
            }
            (I::StorePop, InstructionKind::OwnedStorePop { store })
            | (I::StoreLen, InstructionKind::OwnedStoreLen { store })
            | (I::StoreCapacity, InstructionKind::OwnedStoreCapacity { store }) => {
                (Some(*store), None)
            }
            (I::StoreAt, InstructionKind::OwnedStoreBorrow { store, index })
            | (I::StoreAtMut, InstructionKind::OwnedStoreBorrowMut { store, index }) => {
                (Some(*store), Some(*index))
            }
            _ => {
                return Err(fail(
                    "actual Store operation replaces its original sealed opcode",
                ));
            }
        };
        let inputs =
            self.original
                .intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        let expected_type =
            self.original
                .intrinsic_type_actual(instance, site, self.program, self.inventory, b)?;
        match actuals {
            [] => {
                // No generic function actual is required by this intrinsic.
                // Its full type is nevertheless independently reconstructed
                // above and checked against the actual operands/result below.
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
        if let Some(store) = store {
            if inputs.len() != 2 + usize::from(extra.is_some()) {
                return Err(fail(
                    "actual Store operation changes its complete original input scheme",
                ));
            }
            let selected = if let Type::Address { inner, .. } = &inputs[1].ty {
                inner.as_ref()
            } else {
                &inputs[1].ty
            };
            let family = match selected {
                Type::Managed(payload) | Type::Desc(payload) => payload.as_ref(),
                _ => {
                    return Err(fail(
                        "original Store source has no manager/description category",
                    ));
                }
            };
            if !matches!(family, Type::OwnedStore { .. }) {
                return Err(fail(
                    "original Store source changes its complete resource family",
                ));
            }
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                store,
                b,
            )?;
            let anchor = self
                .original
                .intrinsic_anchor(instance, site, self.inventory, b)?;
            let path = self
                .original
                .native_path(instance, site, self.inventory, b)?;
            self.sealed_resource_place(
                instance, function, store, anchor, &inputs[0], &inputs[1], &path, family, b,
            )?;
            if let Some(extra) = extra {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    extra,
                    b,
                )?;
                if function.values.get(extra.0 as usize) != Some(&inputs[2].ty) {
                    return Err(fail(
                        "actual Store argument changes its complete original type",
                    ));
                }
                if self.uses_full_value(&inputs[2].ty, b)?
                    && self.ssa_value(instance, function, extra, b, 0)? != inputs[2]
                {
                    return Err(fail(
                        "actual Store argument changes its complete original class occurrence",
                    ));
                }
            }
        } else {
            let InstructionKind::OwnedStoreNew { element } = &instruction.kind else {
                unreachable!()
            };
            if inputs.len() != 1 || *element != inputs[0].ty {
                return Err(fail(
                    "actual Store constructor changes its full original element",
                ));
            }
        }
        let output =
            self.original
                .intrinsic_output(instance, site, self.program, self.inventory, b)?;
        self.actual_type(&output.ty, b)?;
        self.result(instruction, function, &output.ty)?;
        Ok(output)
    }
    pub(super) fn store_result(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let mut result = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at,SemanticSiteAddress::Typed{block:actual,instruction,edge:SemanticSiteEdge::After} if actual==block&&instruction as usize==index)
            {
                continue;
            }
            let SemanticSiteAction::BodyCall { actuals } = &site.action else {
                continue;
            };
            let Some(intrinsic) =
                self.original
                    .intrinsic(instance, site.recipe, self.inventory, b)?
            else {
                continue;
            };
            if !matches!(
                intrinsic,
                original::sealed::Intrinsic::StoreNew
                    | original::sealed::Intrinsic::StoreReserve
                    | original::sealed::Intrinsic::StorePush
                    | original::sealed::Intrinsic::StorePop
                    | original::sealed::Intrinsic::StoreLen
                    | original::sealed::Intrinsic::StoreCapacity
                    | original::sealed::Intrinsic::StoreAt
                    | original::sealed::Intrinsic::StoreAtMut
            ) {
                continue;
            }
            let value = self.store_operation(
                instance,
                site.recipe,
                intrinsic,
                function,
                block,
                index,
                actuals,
                b,
            )?;
            if result.replace(value).is_some() {
                return Err(fail("actual Store result has competing original schemes"));
            }
        }
        result.ok_or_else(|| fail("actual Store result has no complete original sealed scheme"))
    }
}

//! Reconstruct original range borrows from actual local/SSA provenance.
//! AddressSlice preserves an existing authority; only a real Borrow/Reborrow
//! in this very chain can anchor the original fresh outer occurrence.
use super::*;
use crate::semantic::value::{self, Value};

impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn source_slice_borrow(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        anchor: original::LocalAnchor,
        input: &Value,
        projection: original::slice_borrows::SliceProjection,
        expected: &Value,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual range borrow exceeds its bounded provenance depth",
            ));
        }
        let local = self.anchored_local(instance, function, anchor, b)?;
        let Type::Address {
            mutable: output_mutable,
            inner: output_inner,
        } = &expected.ty
        else {
            return Err(fail(
                "original range borrow loses its full reference carrier",
            ));
        };
        let Type::Slice { element } = output_inner.as_ref() else {
            return Err(fail(
                "original range borrow result has another pointee family",
            ));
        };
        self.ordinary_type(element, b)?;
        let mut current = id;
        let mut range = false;
        let mut reborrow = false;
        let mut root = None;
        for _ in depth..=b.partial_depth {
            let (block, index, instruction) = self.defining(function, current, b)?;
            let next = match &instruction.kind {
                InstructionKind::ReborrowAddress { address, mutable } => {
                    let Some(Type::Address {
                        mutable: parent_mutable,
                        inner,
                    }) = function.values.get(address.0 as usize)
                    else {
                        return Err(fail("actual range child has no complete parent reference"));
                    };
                    if (*mutable && !*parent_mutable) || (*output_mutable && !*mutable) {
                        return Err(fail(
                            "actual range child changes its original exclusive mode",
                        ));
                    }
                    crate::dyn_metadata::ty(inner, b)?;
                    if function.values.get(current.0 as usize)
                        != Some(&Type::Address {
                            mutable: *mutable,
                            inner: inner.clone(),
                        })
                    {
                        return Err(fail("actual range reborrow changes its complete pointee"));
                    }
                    reborrow = true;
                    *address
                }
                InstructionKind::AddressSlice {
                    address,
                    start,
                    end,
                } => {
                    if range
                        || start.is_some() != projection.start
                        || end.is_some() != projection.end
                    {
                        return Err(fail(
                            "actual range projection changes its original range structure",
                        ));
                    }
                    range = true;
                    for bound in [start, end].into_iter().flatten() {
                        operations::available_before(
                            function,
                            (block, index, SemanticSiteEdge::Before),
                            *bound,
                            b,
                        )?;
                        if function.values.get(bound.0 as usize) != Some(&Type::I64) {
                            return Err(fail("actual range bound loses its checked index carrier"));
                        }
                    }
                    let Some(Type::Address { mutable, inner }) =
                        function.values.get(address.0 as usize)
                    else {
                        return Err(fail(
                            "actual range projection has no borrowed backing place",
                        ));
                    };
                    let actual_element = match inner.as_ref() {
                        Type::Array { element, .. } | Type::Slice { element } => element,
                        _ => {
                            return Err(fail(
                                "actual range projection crosses another complete storage category",
                            ));
                        }
                    };
                    if actual_element != element || (*output_mutable && !*mutable) {
                        return Err(fail(
                            "actual range projection changes its complete element identity or mode",
                        ));
                    }
                    crate::dyn_metadata::ty(element, b)?;
                    if function.values.get(current.0 as usize)
                        != Some(&Type::Address {
                            mutable: *mutable,
                            inner: Box::new(Type::Slice {
                                element: element.clone(),
                            }),
                        })
                    {
                        return Err(fail(
                            "actual range projection changes its complete slice result",
                        ));
                    }
                    *address
                }
                InstructionKind::Load(actual) | InstructionKind::TakeLocal { local: actual }
                    if projection.address =>
                {
                    if *actual != local {
                        return Err(fail(
                            "actual range borrow selects another original reference binder",
                        ));
                    }
                    if !reborrow {
                        return Err(fail(
                            "actual range projection lacks the original fresh reference reborrow",
                        ));
                    }
                    let actual =
                        self.local_value(instance, function, local, (block, index), b, depth + 1)?;
                    if actual != *input {
                        return Err(fail(
                            "actual range borrow changes the complete original backing-reference classes",
                        ));
                    }
                    root = Some(());
                    break;
                }
                InstructionKind::BorrowLocal { local: actual }
                | InstructionKind::BorrowLocalMut { local: actual }
                    if !projection.address =>
                {
                    if *actual != local {
                        return Err(fail(
                            "actual range borrow selects another original array place",
                        ));
                    }
                    let mutable =
                        matches!(instruction.kind, InstructionKind::BorrowLocalMut { .. });
                    if *output_mutable && !mutable {
                        return Err(fail(
                            "actual range borrow strengthens its original backing-array mode",
                        ));
                    }
                    let actual =
                        self.local_value(instance, function, local, (block, index), b, depth + 1)?;
                    if actual != *input {
                        return Err(fail(
                            "actual range borrow changes its full initialized array occupant",
                        ));
                    }
                    root = Some(());
                    break;
                }
                _ => {
                    return Err(unknown(
                        "actual range borrow lacks its exact original initialized backing-place chain",
                    ));
                }
            };
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                next,
                b,
            )?;
            current = next;
        }
        if root.is_none() {
            return Err(unknown(
                "actual range borrow provenance exceeds its finite depth",
            ));
        }
        if !range || function.values.get(id.0 as usize) != Some(&expected.ty) {
            return Err(fail(
                "actual range borrow lacks its exact complete original projection result",
            ));
        }
        crate::dyn_metadata::ty(&expected.ty, b)?;
        crate::dyn_metadata::shape(&expected.life, b)?;
        b.spend(expected.classes.len(), WORK)?;
        Ok(expected.clone())
    }

    pub(super) fn range_projection_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        address: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual intermediate slice projection exceeds its bounded depth",
            ));
        }
        let (block, index, _) = self.defining(function, id, b)?;
        operations::available_before(
            function,
            (block, index, SemanticSiteEdge::Before),
            address,
            b,
        )?;
        let parent = self.ssa_value(instance, function, address, b, depth + 1)?;
        let Type::Address { mutable, .. } = parent.ty else {
            return Err(fail(
                "actual slice projection loses its full parent reference",
            ));
        };
        let pointee = self.pointee(&parent, b)?;
        if !pointee.classes.is_empty()
            || parent.life.slots.len() != 1
            || !parent.life.slots[0].path.is_empty()
        {
            return Err(unknown(
                "actual slice projection needs its complete element occurrence mapping",
            ));
        }
        let element = match pointee.ty {
            Type::Array { element, .. } | Type::Slice { element } => element,
            _ => return Err(fail("actual slice projection has another backing family")),
        };
        let class = parent
            .classes
            .get(parent.life.slots[0].binder as usize)
            .ok_or_else(|| fail("actual slice projection loses its original parent occurrence"))?;
        b.spend(1, WORK)?;
        let result = value::address(
            mutable,
            class.clone(),
            value::scalar(Type::Slice { element }),
            b,
        )?;
        if function.values.get(id.0 as usize) != Some(&result.ty) {
            return Err(fail(
                "actual slice projection changes its complete element type or outer mode",
            ));
        }
        Ok(result)
    }
}

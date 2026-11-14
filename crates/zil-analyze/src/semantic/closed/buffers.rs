//! Bind sealed ByteBuffer operations to the original manager and exact physical
//! projection. Targets, initialized prefixes, pointer epochs and all aliasing
//! remain reconstructed by the actual ByteBuffer and resource kernels.
use super::*;
use crate::semantic::value::Value;

enum Origin {
    Local(LocalId, BlockId, usize),
    Capture(u32),
}
impl Check<'_, '_> {
    fn buffer_address_origin(
        &self,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(Origin, Vec<u32>)> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual ByteBuffer manager path exceeds its finite depth",
            ));
        }
        let (block, index, instruction) = self.defining(function, id, b)?;
        match &instruction.kind {
            InstructionKind::BorrowLocal { local }
            | InstructionKind::BorrowLocalMut { local }
            | InstructionKind::Load(local)
            | InstructionKind::TakeLocal { local } => {
                Ok((Origin::Local(*local, block, index), Vec::new()))
            }
            InstructionKind::BorrowCapture { index, .. }
            | InstructionKind::ReadCapture { index } => Ok((Origin::Capture(*index), Vec::new())),
            InstructionKind::ReborrowAddress { address, mutable } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                let Some(Type::Address {
                    mutable: parent,
                    inner,
                }) = function.values.get(address.0 as usize)
                else {
                    return Err(fail("actual manager reborrow has no parent address"));
                };
                if *mutable && !*parent {
                    return Err(fail("actual manager reborrow strengthens a shared head"));
                }
                crate::dyn_metadata::ty(inner, b)?;
                if function.values.get(id.0 as usize)
                    != Some(&Type::Address {
                        mutable: *mutable,
                        inner: inner.clone(),
                    })
                {
                    return Err(fail("actual manager reborrow changes its complete pointee"));
                }
                self.buffer_address_origin(function, *address, b, depth + 1)
            }
            InstructionKind::AddressField {
                address,
                index: field,
            } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                let (origin, mut path) =
                    self.buffer_address_origin(function, *address, b, depth + 1)?;
                b.spend(path.len().saturating_add(1), WORK)?;
                path.try_reserve(1)
                    .map_err(|_| original::resource("actual manager path allocation failed"))?;
                path.push(*field);
                Ok((origin, path))
            }
            _ => Err(unknown(
                "actual ByteBuffer descriptor needs its complete initialized head provenance",
            )),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sealed_resource_place(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        descriptor: ValueId,
        anchor: original::LocalAnchor,
        root: &Value,
        selected: &Value,
        expected_path: &[u32],
        family: &Type,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let (block, index, instruction) = self.defining(function, descriptor, b)?;
        let (origin, path) = match &instruction.kind {
            InstructionKind::DescribeLocal { local } => {
                (Origin::Local(*local, block, index), Vec::new())
            }
            InstructionKind::DescribeCapture { index } => (Origin::Capture(*index), Vec::new()),
            InstructionKind::DescribePackageField {
                local,
                index: field,
            } => {
                b.spend(1, WORK)?;
                (Origin::Local(*local, block, index), vec![*field])
            }
            InstructionKind::DescribeAddress { address } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                self.buffer_address_origin(function, *address, b, 0)?
            }
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local }
                if expected_path.is_empty() && matches!(selected.ty, Type::Desc(_)) =>
            {
                (Origin::Local(*local, block, index), Vec::new())
            }
            InstructionKind::ReadCapture { index }
                if expected_path.is_empty() && matches!(selected.ty, Type::Desc(_)) =>
            {
                (Origin::Capture(*index), Vec::new())
            }
            _ => {
                return Err(unknown(
                    "actual ByteBuffer operation lacks its original manager/description place operation",
                ));
            }
        };
        b.spend(path.len().saturating_add(expected_path.len()), WORK)?;
        if path != expected_path {
            return Err(fail(
                "actual ByteBuffer operation changes its original field path",
            ));
        }
        let actual = match origin {
            Origin::Local(actual, at_block, at_index) => {
                if self.anchored_local(instance, function, anchor, b)? != actual {
                    return Err(fail(
                        "actual ByteBuffer operation selects another original parent place",
                    ));
                }
                self.local_value(instance, function, actual, (at_block, at_index), b, 0)?
            }
            Origin::Capture(actual) => {
                let original::LocalAnchor::Environment(ordinal) = anchor else {
                    return Err(fail(
                        "actual ByteBuffer capture replaces a different original binding",
                    ));
                };
                if self.environment_capture(instance, function, ordinal, b)? != actual {
                    return Err(fail(
                        "actual ByteBuffer operation selects another original captured object",
                    ));
                }
                self.capture_whole_value(instance, function, actual, b, 0)?
            }
        };
        if actual != *root {
            return Err(fail(
                "actual ByteBuffer place changes its complete original class domain",
            ));
        }
        let mut leaf = &root.ty;
        if let Type::Address { inner, .. } = leaf {
            leaf = inner;
        }
        for field in &path {
            b.spend(1, WORK)?;
            if let Type::BorrowedStorage { storage, .. } = leaf {
                leaf = storage;
            }
            leaf = match leaf {
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    &fields
                        .get(*field as usize)
                        .ok_or_else(|| {
                            fail("actual ByteBuffer field exceeds the complete nominal layout")
                        })?
                        .1
                }
                Type::Tuple(fields) => fields
                    .get(*field as usize)
                    .ok_or_else(|| fail("actual ByteBuffer field exceeds its complete tuple"))?,
                _ => {
                    return Err(fail(
                        "actual ByteBuffer path crosses another physical storage family",
                    ));
                }
            };
        }
        let expected_leaf = if let Type::Address { inner, .. } = &selected.ty {
            inner.as_ref()
        } else {
            &selected.ty
        };
        if leaf != expected_leaf {
            return Err(fail(
                "actual ByteBuffer field changes the complete original leaf type",
            ));
        }
        let payload = match expected_leaf {
            Type::Managed(payload) | Type::Desc(payload) => payload,
            _ => {
                return Err(fail(
                    "original ByteBuffer place loses its manager/description category",
                ));
            }
        };
        crate::dyn_metadata::ty(payload, b)?;
        crate::dyn_metadata::ty(family, b)?;
        if payload.as_ref() != family
            || function.values.get(descriptor.0 as usize) != Some(&Type::Desc(payload.clone()))
        {
            return Err(fail(
                "actual ByteBuffer operation uses another complete description family",
            ));
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn buffer_operation(
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
                "actual ByteBuffer operation invents generic arguments",
            ));
        }
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(index))
            .ok_or_else(|| fail("actual ByteBuffer operation has no instruction"))?;
        let (descriptor, extra) = match (&intrinsic, &instruction.kind) {
            (original::sealed::Intrinsic::BytesNew, InstructionKind::ByteBufferNew) => (None, None),
            (
                original::sealed::Intrinsic::BytesView,
                InstructionKind::ByteBufferBorrow {
                    buffer,
                    mutable: false,
                },
            ) => (Some(*buffer), None),
            (original::sealed::Intrinsic::BytesLen, InstructionKind::ByteBufferLen { buffer })
            | (
                original::sealed::Intrinsic::BytesCapacity,
                InstructionKind::ByteBufferCapacity { buffer },
            ) => (Some(*buffer), None),
            (
                original::sealed::Intrinsic::BytesReserve,
                InstructionKind::ByteBufferReserve { buffer, capacity },
            ) => (Some(*buffer), Some(*capacity)),
            (
                original::sealed::Intrinsic::BytesPush,
                InstructionKind::ByteBufferPush { buffer, value },
            ) => (Some(*buffer), Some(*value)),
            (
                original::sealed::Intrinsic::BytesTruncate,
                InstructionKind::ByteBufferTruncate { buffer, length },
            ) => (Some(*buffer), Some(*length)),
            _ => {
                return Err(fail(
                    "actual ByteBuffer operation replaces its independently resolved original opcode",
                ));
            }
        };
        let inputs =
            self.original
                .intrinsic_inputs(instance, site, self.program, self.inventory, b)?;
        if let Some(descriptor) = descriptor {
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                descriptor,
                b,
            )?;
            if inputs.len() != 2 + usize::from(extra.is_some()) {
                return Err(fail(
                    "actual ByteBuffer operation changes its original full input arity",
                ));
            }
            let anchor = self
                .original
                .intrinsic_anchor(instance, site, self.inventory, b)?;
            let path = self
                .original
                .native_path(instance, site, self.inventory, b)?;
            self.sealed_resource_place(
                instance,
                function,
                descriptor,
                anchor,
                &inputs[0],
                &inputs[1],
                &path,
                &Type::ByteBuffer,
                b,
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
                        "actual ByteBuffer scalar argument changes its original fixed carrier",
                    ));
                }
            }
        } else if !inputs.is_empty() {
            return Err(fail(
                "actual empty ByteBuffer construction invents external sources",
            ));
        }
        let output =
            self.original
                .intrinsic_output(instance, site, self.program, self.inventory, b)?;
        self.actual_type(&output.ty, b)?;
        Ok(output.ty)
    }
}

impl Check<'_, '_> {
    pub(super) fn buffer_result(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let mut selected = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at,SemanticSiteAddress::Typed{block:actual,instruction,edge:SemanticSiteEdge::After} if actual==block && instruction as usize==index)
            {
                continue;
            }
            let SemanticSiteAction::BodyCall { actuals } = &site.action else {
                continue;
            };
            if self
                .original
                .intrinsic(instance, site.recipe, self.inventory, b)?
                != Some(original::sealed::Intrinsic::BytesView)
            {
                continue;
            }
            let ty = self.buffer_operation(
                instance,
                site.recipe,
                original::sealed::Intrinsic::BytesView,
                function,
                block,
                index,
                actuals,
                b,
            )?;
            let output = self.original.intrinsic_output(
                instance,
                site.recipe,
                self.program,
                self.inventory,
                b,
            )?;
            if output.ty != ty {
                return Err(fail(
                    "ByteBuffer view changes its complete independently checked original output",
                ));
            }
            if selected.replace(output).is_some() {
                return Err(fail(
                    "ByteBuffer view has competing original operation occurrences",
                ));
            }
        }
        selected.ok_or_else(|| {
            unknown("actual ByteBuffer view lacks its exact original manager and output occurrence")
        })
    }
}

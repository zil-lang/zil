//! Actual projected borrows follow physical field operations to the original
//! initialized root. Class claims neither select a root nor cross an owner.
use super::*;

enum Root {
    Local(LocalId, BlockId, usize, bool), // bool: loaded an existing address
    Capture(u32, bool),
    Expression(ValueId),
}
struct Origin {
    root: Root,
    path: Vec<u32>,
    index: Option<ValueId>,
    fresh: bool,
}

impl Check<'_, '_> {
    pub(super) fn checked_owned_place(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        source: &original::borrow_places::Source,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if source.indirect || source.index.is_some() || source.expression.is_some() {
            return Err(fail(
                "owning callable cannot move through an indirect original place",
            ));
        }
        let local = self.anchored_local(
            instance,
            function,
            source
                .anchor
                .ok_or_else(|| fail("owning callable has no original anchor"))?,
            b,
        )?;
        let (block, index, instruction) = self.defining(function, id, b)?;
        b.spend(source.path.len().saturating_add(1), WORK)?;
        let matches = match &instruction.kind {
            InstructionKind::TakeLocal { local: actual } => {
                *actual == local && source.path.is_empty()
            }
            InstructionKind::TakePackageField {
                local: actual,
                index: field,
            } => *actual == local && source.path.as_slice() == [*field],
            InstructionKind::TakeTupleField {
                local: actual,
                path,
            } => {
                *actual == local
                    && path.iter().copied().eq(source
                        .path
                        .iter()
                        .copied()
                        .map(PlaceProjection::Field))
            }
            _ => false,
        };
        if !matches || function.values.get(id.0 as usize) != Some(&source.inner.ty) {
            return Err(fail(
                "owning callable takes another original place, field or full type",
            ));
        }
        let root = self.local_value(instance, function, local, (block, index), b, 0)?;
        if root != source.root
            || self.borrow_field_type(&root.ty, &source.path, b)? != &source.inner.ty
        {
            return Err(fail(
                "owning callable loses its original root and complete child domain",
            ));
        }
        Ok(())
    }

    fn borrow_field_type<'a>(
        &self,
        mut ty: &'a Type,
        path: &[u32],
        b: &mut Budget,
    ) -> CompileResult<&'a Type> {
        for index in path {
            b.spend(1, WORK)?;
            if let Type::BorrowedStorage { storage, .. } = ty {
                ty = storage;
            }
            ty = match ty {
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    &fields
                        .get(*index as usize)
                        .ok_or_else(|| {
                            fail("actual borrowed field exceeds complete nominal arity")
                        })?
                        .1
                }
                Type::Tuple(fields) => fields
                    .get(*index as usize)
                    .ok_or_else(|| fail("actual borrowed field exceeds complete tuple arity"))?,
                _ => {
                    return Err(fail(
                        "actual borrowed field crosses a pointer, owner or opaque boundary",
                    ));
                }
            };
        }
        Ok(ty)
    }

    fn projected_borrow_origin(
        &self,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Origin> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual projected borrow exceeds its finite ancestry",
            ));
        }
        let (block, index, instruction) = self.defining(function, id, b)?;
        let Some(Type::Address {
            mutable: output_mutable,
            inner: output_inner,
        }) = function.values.get(id.0 as usize)
        else {
            return Err(fail(
                "actual projected borrow has no complete address result",
            ));
        };
        match &instruction.kind {
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
                let mutable = matches!(instruction.kind, InstructionKind::BorrowLocalMut { .. });
                let actual = function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| fail("actual borrow root local is absent"))?;
                if mutable != *output_mutable || output_inner.as_ref() != &actual.ty {
                    return Err(fail("actual borrowed root changes its full type or mode"));
                }
                Ok(Origin {
                    root: Root::Local(*local, block, index, false),
                    path: Vec::new(),
                    index: None,
                    fresh: true,
                })
            }
            InstructionKind::BorrowPlaceLocal {
                local,
                path,
                mutable,
            } => {
                let actual = function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| fail("actual projected root local is absent"))?;
                let mut fields = Vec::new();
                for step in path {
                    let PlaceProjection::Field(field) = step else {
                        return Err(unknown(
                            "original field borrow requires its exact active variant projection",
                        ));
                    };
                    self.push_value(&mut fields, *field, b)?;
                }
                if *mutable != *output_mutable
                    || self.borrow_field_type(&actual.ty, &fields, b)? != output_inner.as_ref()
                {
                    return Err(fail(
                        "actual projected root changes its full leaf type or mode",
                    ));
                }
                Ok(Origin {
                    root: Root::Local(*local, block, index, false),
                    path: fields,
                    index: None,
                    fresh: true,
                })
            }
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                let actual = function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| fail("actual borrowed address local is absent"))?;
                if function.values.get(id.0 as usize) != Some(&actual.ty) {
                    return Err(fail("actual borrowed address read changes its full type"));
                }
                Ok(Origin {
                    root: Root::Local(*local, block, index, true),
                    path: Vec::new(),
                    index: None,
                    fresh: false,
                })
            }
            InstructionKind::BorrowCapture { index, mutable } => {
                if *mutable != *output_mutable {
                    return Err(fail("actual capture borrow changes its mode"));
                }
                Ok(Origin {
                    root: Root::Capture(*index, false),
                    path: Vec::new(),
                    index: None,
                    fresh: true,
                })
            }
            InstructionKind::ReadCapture { index } => Ok(Origin {
                root: Root::Capture(*index, true),
                path: Vec::new(),
                index: None,
                fresh: false,
            }),
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
                    return Err(fail("actual projected reborrow has no parent address"));
                };
                if *mutable != *output_mutable || *mutable && !*parent || inner != output_inner {
                    return Err(fail(
                        "actual projected reborrow strengthens a mode or changes its complete pointee",
                    ));
                }
                let mut origin = self.projected_borrow_origin(function, *address, b, depth + 1)?;
                origin.fresh = true;
                Ok(origin)
            }
            InstructionKind::AddressIndex {
                address,
                index: offset,
            } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *offset,
                    b,
                )?;
                if !function
                    .values
                    .get(offset.0 as usize)
                    .is_some_and(Type::is_integer)
                {
                    let mut errors =
                        fail("actual indexed borrow has another checked index carrier");
                    errors[0].notes.push(format!(
                        "function {} SSA {} index {}: {:?}",
                        function.id.0,
                        id.0,
                        offset.0,
                        function.values.get(offset.0 as usize)
                    ));
                    return Err(errors);
                }
                let Some(Type::Address { mutable, inner }) =
                    function.values.get(address.0 as usize)
                else {
                    return Err(fail("actual indexed borrow has no backing reference"));
                };
                let element = match inner.as_ref() {
                    Type::Array { element, .. } | Type::Slice { element } => element,
                    _ => {
                        return Err(fail(
                            "actual indexed borrow crosses another backing category",
                        ));
                    }
                };
                if mutable != output_mutable || element != output_inner {
                    return Err(fail(
                        "actual indexed borrow changes its element type or access mode",
                    ));
                }
                let mut origin = self.projected_borrow_origin(function, *address, b, depth + 1)?;
                if origin.index.replace(*offset).is_some() {
                    return Err(unknown(
                        "actual indexed borrow needs its complete multidimensional source",
                    ));
                }
                origin.fresh = true;
                Ok(origin)
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
                let Some(Type::Address { mutable, inner }) =
                    function.values.get(address.0 as usize)
                else {
                    return Err(fail("actual field borrow has no complete parent address"));
                };
                if mutable != output_mutable
                    || self.borrow_field_type(inner, &[*field], b)? != output_inner.as_ref()
                {
                    return Err(fail(
                        "actual field borrow changes its exact leaf type or mode",
                    ));
                }
                let mut origin = self.projected_borrow_origin(function, *address, b, depth + 1)?;
                if origin.index.is_some() {
                    return Err(unknown(
                        "actual field borrow needs its complete post-index source mapping",
                    ));
                }
                self.push_value(&mut origin.path, *field, b)?;
                origin.fresh = true;
                Ok(origin)
            }
            _ => Ok(Origin {
                root: Root::Expression(id),
                path: Vec::new(),
                index: None,
                fresh: false,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn checked_borrow_place(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        source: &original::borrow_places::Source,
        address_input: bool,
        outer_fresh: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        let origin = self.projected_borrow_origin(function, id, b, depth + 1)?;
        b.spend(origin.path.len().saturating_add(source.path.len()), WORK)?;
        if origin.path != source.path || !(origin.fresh || outer_fresh) {
            return Err(fail(
                "actual borrow changes its original physical field path or omits its fresh borrow",
            ));
        }
        let (root, indirect) = match origin.root {
            Root::Local(local, block, index, indirect) => {
                if self.anchored_local(
                    instance,
                    function,
                    source.anchor.ok_or_else(|| {
                        fail("actual local borrow replaces an original emitted reference")
                    })?,
                    b,
                )? != local
                {
                    return Err(fail("actual field borrow selects another original root"));
                }
                (
                    self.local_value(instance, function, local, (block, index), b, depth + 1)?,
                    indirect,
                )
            }
            Root::Capture(capture, indirect) => {
                let Some(original::LocalAnchor::Environment(ordinal)) = source.anchor else {
                    return Err(fail(
                        "actual field borrow replaces a lexical root by a capture",
                    ));
                };
                if self.environment_capture(instance, function, ordinal, b)? != capture {
                    return Err(fail("actual field borrow selects another captured root"));
                }
                (
                    self.capture_whole_value(instance, function, capture, b, depth + 1)?,
                    indirect,
                )
            }
            Root::Expression(actual) => {
                let node = source.expression.ok_or_else(|| {
                    fail("actual returned reference replaces an original lexical place")
                })?;
                if self.original_subject_value(instance, function, node, b)? != Some(actual) {
                    return Err(fail(
                        "actual indexed borrow selects another original returned reference",
                    ));
                }
                (
                    self.ssa_value(instance, function, actual, b, depth + 1)?,
                    true,
                )
            }
        };
        if root != source.root || indirect != (source.indirect || address_input) {
            return Err(fail(
                "actual borrowed root changes its original whole classes or pointer boundary",
            ));
        }
        let whole = if indirect {
            let Type::Address { inner, .. } = &root.ty else {
                return Err(fail(
                    "actual borrowed root loses its original reference head",
                ));
            };
            inner.as_ref()
        } else {
            &root.ty
        };
        let mut leaf = self.borrow_field_type(whole, &origin.path, b)?;
        match (source.index, origin.index) {
            (None, None) => {}
            (Some(node), Some(actual)) => {
                let observed = self.original_subject_value(instance, function, node, b)?;
                if observed != Some(actual) {
                    let definition = self.inventory.definition(instance.definition, b)?;
                    let original = super::super::original_index::index(definition, b)?;
                    let expression = original.get_expr(node, b)?;
                    let (_, _, operation) = self.defining(function, actual, b)?;
                    // A folded integer literal need not have a separate
                    // runtime observation. Prove its exact original spelling,
                    // carrier and value against the actual Constant instead.
                    let literal = match (&expression.kind, &operation.kind) {
                        (CoreExprKind::Integer(text), InstructionKind::Constant(value))
                            if observed.is_none() && value.ty().is_integer() =>
                        {
                            let expected = self.recipe_value(
                                instance,
                                &SemanticTypeRecipe::OriginalType {
                                    scope: SemanticScopeRef::Ancestor(0),
                                    source: SemanticOriginalTypeSource::Expression(node),
                                },
                                b,
                                0,
                            )?;
                            expected.ty == value.ty()
                                && original::mixed::integer_constant(text, &expected.ty, b)?
                                    == *value
                        }
                        _ => false,
                    };
                    if !literal {
                        let mut errors = unknown(
                            "actual indexed borrow lacks its exact original emitted index operand",
                        );
                        errors[0].notes.push(format!("original index node {}, actual SSA {}, observed {:?}, actual operation {:?}", node.0, actual.0, observed, operation.kind));
                        return Err(errors);
                    }
                }
                leaf = match leaf {
                    Type::Array { element, .. } | Type::Slice { element } => element.as_ref(),
                    _ => {
                        return Err(fail(
                            "original indexed borrow loses its complete backing element",
                        ));
                    }
                };
            }
            _ => {
                return Err(fail(
                    "actual indexed borrow adds or omits an original index projection",
                ));
            }
        }
        let required = if address_input {
            let Type::Address { inner, .. } = &source.inner.ty else {
                return Err(fail(
                    "original range backing loses its complete reference carrier",
                ));
            };
            inner.as_ref()
        } else {
            &source.inner.ty
        };
        let Some(Type::Address { inner, .. }) = function.values.get(id.0 as usize) else {
            return Err(fail("actual borrowed place loses its result reference"));
        };
        if leaf != required || inner.as_ref() != required {
            return Err(fail(
                "actual borrowed place changes its complete original field type",
            ));
        }
        Ok(())
    }
}

//! Actual projected borrows follow physical field operations to the original
//! initialized root. Class claims neither select a root nor cross an owner.
use super::*;

enum Root {
    Local(LocalId, BlockId, usize, bool), // bool: loaded an existing address
    Capture(u32, bool),
}
struct Origin {
    root: Root,
    path: Vec<u32>,
    fresh: bool,
}

impl Check<'_, '_> {
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
                    fresh: true,
                })
            }
            InstructionKind::ReadCapture { index } => Ok(Origin {
                root: Root::Capture(*index, true),
                path: Vec::new(),
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
                self.push_value(&mut origin.path, *field, b)?;
                origin.fresh = true;
                Ok(origin)
            }
            _ => Err(unknown(
                "original field borrow lacks its exact actual physical root operation",
            )),
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
                if self.anchored_local(instance, function, source.anchor, b)? != local {
                    return Err(fail("actual field borrow selects another original root"));
                }
                (
                    self.local_value(instance, function, local, (block, index), b, depth + 1)?,
                    indirect,
                )
            }
            Root::Capture(capture, indirect) => {
                let original::LocalAnchor::Environment(ordinal) = source.anchor else {
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
        let leaf = self.borrow_field_type(whole, &origin.path, b)?;
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

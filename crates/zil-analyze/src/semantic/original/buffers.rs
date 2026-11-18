//! Sealed ByteBuffer operations inspect an original manager/description place.
//! They never copy its containing Package or infer an object from its type.
use super::*;
impl Solver<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn buffer_call(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        intrinsic: sealed::Intrinsic,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let new = intrinsic == sealed::Intrinsic::BytesNew;
        let writing = matches!(
            intrinsic,
            sealed::Intrinsic::BytesReserve
                | sealed::Intrinsic::BytesPush
                | sealed::Intrinsic::BytesTruncate
        );
        let exclusive = writing
            || matches!(intrinsic, sealed::Intrinsic::BytesView { mutable: true });
        let expected = if new {
            0
        } else if writing {
            2
        } else {
            1
        };
        if !written.is_empty() || arguments.len() != expected {
            return Err(fail(
                "original ByteBuffer operation changes its complete sealed scheme arity",
            ));
        }
        let mut inputs = Vec::new();
        let mut manager_life = None;
        if !new {
            let manager = &arguments[0];
            if let Some(place) = self.place(frame, manager, true, b, depth + 1)? {
                if exclusive
                    && matches!(
                        self.state.arena[self.type_root(place.root, b)?],
                        Kind::Address { mutable: false, .. }
                    )
                {
                    return Err(fail(
                        "original ByteBuffer mutation cannot use a shared projected manager",
                    ));
                }
                let anchor = place.anchor.ok_or_else(|| {
                    unknown("original ByteBuffer operation needs its exact binding/projection identity")
                })?;
                let payload = match self.state.arena[self.type_root(place.selected, b)?] {
                    Kind::Wrapped(Wrapper::Managed, payload) | Kind::Desc { inner: payload, .. } => {
                        payload
                    }
                    Kind::Address { mutable, inner, .. } => {
                        if exclusive && !mutable {
                            return Err(fail(
                                "original ByteBuffer mutation cannot derive writable authority from a shared head",
                            ));
                        }
                        let Kind::Wrapped(Wrapper::Managed, payload) =
                            self.state.arena[self.type_root(inner, b)?]
                        else {
                            return Err(fail(
                                "original ByteBuffer address borrows another storage category",
                            ));
                        };
                        payload
                    }
                    _ => {
                        return Err(fail(
                            "original ByteBuffer operation has no actual manager or description",
                        ));
                    }
                };
                if !matches!(
                    self.state.arena[self.type_root(payload, b)?],
                    Kind::Atom(Type::ByteBuffer)
                ) {
                    return Err(fail(
                        "original ByteBuffer operation selects another sealed resource family",
                    ));
                }
                if exclusive && matches!(anchor, LocalAnchor::Environment(_)) {
                    b.lookup(self.mutating_closures.len(), WORK)?;
                    self.mutating_closures.insert(frame.definition);
                }
                let selected = self.type_root(place.selected, b)?;
                let owner = match self.state.arena[selected] {
                    Kind::Address { inner, .. } => self.type_root(inner, b)?,
                    _ => selected,
                };
                let descriptor =
                    if matches!(self.state.arena[owner], Kind::Wrapped(Wrapper::Managed, _)) {
                        self.describe_argument(frame, &arguments[0], &place, owner, b, depth + 1)?
                    } else {
                        owner
                    };
                let descriptor = self.type_root(descriptor, b)?;
                let Kind::Desc { life, .. } = self.state.arena[descriptor] else {
                    return Err(fail(
                        "original ByteBuffer view lacks its exact manager description",
                    ));
                };
                manager_life = Some(life);
                Self::push(&mut inputs, place.root, b)?;
                Self::push(&mut inputs, place.selected, b)?;
                b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
                self.state
                    .intrinsic_anchors
                    .insert((frame.definition, node), anchor);
                b.lookup(self.state.native_paths.len(), WORK)?;
                self.state
                    .native_paths
                    .insert((frame.definition, node), place.path);
            } else {
                if matches!(intrinsic, sealed::Intrinsic::BytesView { .. })
                    || !matches!(manager.kind, CoreExprKind::Call { .. })
                {
                    return Err(unknown(
                        "original ByteBuffer operation needs its full backing-place expression",
                    ));
                }
                let source = self.expression(frame, manager, b, depth + 1)?;
                let Kind::Wrapped(Wrapper::Managed, payload) =
                    self.state.arena[self.type_root(source, b)?]
                else {
                    return Err(fail("original temporary ByteBuffer has no owning result"));
                };
                if !matches!(self.state.arena[self.type_root(payload, b)?], Kind::Atom(Type::ByteBuffer)) {
                    return Err(fail("original temporary descriptor selects another resource family"));
                }
                // This physical temporary retains the earlier owning result
                // until all later operands complete or take their cleanup edge.
                self.prepare_call_value(frame.definition, manager.id, source, b)?;
                Self::push(&mut inputs, source, b)?;
            }
            if writing {
                let ty = if intrinsic == sealed::Intrinsic::BytesPush {
                    Type::U8
                } else {
                    Type::I64
                };
                let expected = self.atom(ty, b)?;
                let actual = self.expression(frame, &arguments[1], b, depth + 1)?;
                self.unify(actual, expected, depth + 1, b)?;
                Self::push(&mut inputs, expected, b)?;
            }
        }
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        if self
            .state
            .intrinsic_inputs
            .insert((frame.definition, node), inputs)
            .is_some()
        {
            return Err(fail(
                "original ByteBuffer operation has competing input schemes",
            ));
        }
        b.lookup(self.state.intrinsics.len(), WORK)?;
        if self
            .state
            .intrinsics
            .insert((frame.definition, node), intrinsic)
            .is_some()
        {
            return Err(fail(
                "original ByteBuffer operation has competing identities",
            ));
        }
        if new {
            let payload = self.atom(Type::ByteBuffer, b)?;
            self.term(Kind::Wrapped(Wrapper::Managed, payload), b)
        } else if let sealed::Intrinsic::BytesView { mutable } = intrinsic {
            let life = manager_life.ok_or_else(|| {
                fail("original ByteBuffer borrow lacks its complete manager source lifetime")
            })?;
            let element = self.atom(Type::U8, b)?;
            let inner = self.term(Kind::Slice(element), b)?;
            let wanted = if let Some(receiving) = self.receiving_for(frame, node, b)? {
                match self.state.arena[self.type_root(receiving, b)?] {
                    Kind::Address {
                        mutable: mode,
                        life: wanted,
                        inner: expected,
                    } if mode == mutable => {
                        self.unify(inner, expected, depth + 1, b)?;
                        Some(wanted)
                    }
                    _ => None,
                }
            } else {
                None
            };
            // The weak argument keeps its own occurrence. Only this view
            // can receive the target after checking the real manager upper.
            let life = self.select_view_life(frame, life, Some(life), wanted, None, b, depth + 1)?;
            self.term(Kind::Address { mutable, life, inner }, b)
        } else {
            self.atom(if writing { Type::Bool } else { Type::I64 }, b)
        }
    }
}
impl Checked {
    pub(in crate::semantic) fn native_path(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Vec<u32>> {
        let definition = inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail(
                "actual native place has no complete original declaration",
            ));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site {
                continue;
            }
            let SemanticOriginalNode::Expression(node) = link.node else {
                return Err(fail("actual native place selects an original statement"));
            };
            if selected.replace(node).is_some() {
                return Err(fail("actual native place has several original nodes"));
            }
        }
        b.lookup(self.native_paths.len(), WORK)?;
        let path = self
            .native_paths
            .get(&(
                instance.definition,
                selected.ok_or_else(|| fail("actual native place loses its original call node"))?,
            ))
            .ok_or_else(|| {
                fail("actual native place loses its independently resolved physical path")
            })?;
        b.spend(path.len(), WORK)?;
        Ok(path.clone())
    }
}

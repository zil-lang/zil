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
        if !new {
            let place = self
                .place(frame, &arguments[0], true, b, depth + 1)?
                .ok_or_else(|| {
                    unknown("original ByteBuffer operation needs its full backing-place expression")
                })?;
            let anchor = place.anchor.ok_or_else(|| {
                unknown("original ByteBuffer operation needs its exact binding/projection identity")
            })?;
            let payload = match self.state.arena[self.type_root(place.selected, b)?] {
                Kind::Wrapped(Wrapper::Managed, payload) | Kind::Desc { inner: payload, .. } => {
                    payload
                }
                Kind::Address { mutable, inner, .. } => {
                    if writing && !mutable {
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
            if writing && matches!(anchor, LocalAnchor::Environment(_)) {
                b.lookup(self.mutating_closures.len(), WORK)?;
                self.mutating_closures.insert(frame.definition);
            }
            let selected = self.type_root(place.selected, b)?;
            let owner = match self.state.arena[selected] {
                Kind::Address { inner, .. } => self.type_root(inner, b)?,
                _ => selected,
            };
            if matches!(self.state.arena[owner], Kind::Wrapped(Wrapper::Managed, _)) {
                self.describe_argument(frame, &arguments[0], &place, owner, b, depth + 1)?;
            }
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
        } else if intrinsic == sealed::Intrinsic::BytesView {
            let ordinal = self.borrow_occurrence(frame.definition, node, b)?;
            let life = self.term(Kind::Life(Life::Implicit(frame.definition, ordinal)), b)?;
            let element = self.atom(Type::U8, b)?;
            let inner = self.term(Kind::Slice(element), b)?;
            self.term(
                Kind::Address {
                    mutable: false,
                    life,
                    inner,
                },
                b,
            )
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

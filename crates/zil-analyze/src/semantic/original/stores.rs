//! Store schemes retain ownership of initialized entries. A borrowed element
//! has its own original occurrence; it is never a copied whole element.
use super::*;
impl Solver<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn store_call(
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
        let new = intrinsic == sealed::Intrinsic::StoreNew;
        let pair = matches!(
            intrinsic,
            sealed::Intrinsic::StoreReserve
                | sealed::Intrinsic::StorePush
                | sealed::Intrinsic::StoreAt
                | sealed::Intrinsic::StoreAtMut
        );
        if arguments.len()
            != if new {
                0
            } else if pair {
                2
            } else {
                1
            }
        {
            return Err(fail(
                "original Store operation changes its complete value arity",
            ));
        }
        let mut inputs = Vec::new();
        let element = if new {
            match written {
                [CoreGenericArgument::Type(element)] => {
                    self.recipe(frame, element, b, depth + 1)?
                }
                [] => {
                    let result = self.type_root(frame.result, b)?;
                    let Kind::Wrapped(Wrapper::Managed, payload) = self.state.arena[result] else {
                        return Err(unknown(
                            "original inferred Store element needs its receiving declaration context",
                        ));
                    };
                    let Kind::Wrapped(Wrapper::Store, element) =
                        self.state.arena[self.type_root(payload, b)?]
                    else {
                        return Err(fail(
                            "original inferred Store result changes its resource family",
                        ));
                    };
                    element
                }
                _ => {
                    return Err(fail(
                        "original Store construction changes its full Type actual arity",
                    ));
                }
            }
        } else {
            if !written.is_empty() {
                return Err(fail("original Store operation invents generic actuals"));
            }
            let place = self
                .place(frame, &arguments[0], true, b, depth + 1)?
                .ok_or_else(|| {
                    unknown("original Store operation needs its complete initialized place")
                })?;
            let anchor = place.anchor.ok_or_else(|| {
                unknown("original Store operation has no exact source place anchor")
            })?;
            if matches!(anchor, LocalAnchor::Environment(_))
                && matches!(
                    intrinsic,
                    sealed::Intrinsic::StoreReserve
                        | sealed::Intrinsic::StorePush
                        | sealed::Intrinsic::StorePop
                        | sealed::Intrinsic::StoreAtMut
                )
            {
                b.lookup(self.mutating_closures.len(), WORK)?;
                self.mutating_closures.insert(frame.definition);
            }
            let mut selected = self.type_root(place.selected, b)?;
            if let Kind::Address { inner, mutable, .. } = self.state.arena[selected] {
                if !mutable
                    && matches!(
                        intrinsic,
                        sealed::Intrinsic::StoreReserve
                            | sealed::Intrinsic::StorePush
                            | sealed::Intrinsic::StorePop
                            | sealed::Intrinsic::StoreAtMut
                    )
                {
                    return Err(fail(
                        "original Store mutation strengthens a shared manager head",
                    ));
                }
                selected = self.type_root(inner, b)?;
            }
            let payload = match self.state.arena[selected] {
                Kind::Wrapped(Wrapper::Managed, payload) | Kind::Desc { inner: payload, .. } => {
                    self.type_root(payload, b)?
                }
                _ => {
                    return Err(fail(
                        "original Store operation has no Managed or Desc storage category",
                    ));
                }
            };
            let Kind::Wrapped(Wrapper::Store, element) = self.state.arena[payload] else {
                return Err(fail(
                    "original Store operation selects another resource family",
                ));
            };
            Self::push(&mut inputs, place.root, b)?;
            Self::push(&mut inputs, place.selected, b)?;
            b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
            if self
                .state
                .intrinsic_anchors
                .insert((frame.definition, node), anchor)
                .is_some()
            {
                return Err(fail("original Store operation repeats its source anchor"));
            }
            b.lookup(self.state.native_paths.len(), WORK)?;
            if self
                .state
                .native_paths
                .insert((frame.definition, node), place.path)
                .is_some()
            {
                return Err(fail(
                    "original Store operation repeats its physical source path",
                ));
            }
            element
        };
        Self::push(
            &mut self.needs,
            Need {
                term: element,
                predicate: Predicate::StoreElement,
            },
            b,
        )?;
        let result = match intrinsic {
            sealed::Intrinsic::StoreNew => {
                let payload = self.term(Kind::Wrapped(Wrapper::Store, element), b)?;
                Self::push(&mut inputs, element, b)?;
                self.term(Kind::Wrapped(Wrapper::Managed, payload), b)?
            }
            sealed::Intrinsic::StoreReserve
            | sealed::Intrinsic::StoreAt
            | sealed::Intrinsic::StoreAtMut => {
                let index = self.atom(Type::I64, b)?;
                let actual = self.expression(frame, &arguments[1], b, depth + 1)?;
                self.unify(actual, index, depth + 1, b)?;
                Self::push(&mut inputs, index, b)?;
                if intrinsic == sealed::Intrinsic::StoreReserve {
                    self.atom(Type::Bool, b)?
                } else {
                    let ordinal = self.native_result_occurrence(frame.definition, node, b)?;
                    let life =
                        self.term(Kind::Life(Life::Implicit(frame.definition, ordinal)), b)?;
                    self.term(
                        Kind::Address {
                            mutable: intrinsic == sealed::Intrinsic::StoreAtMut,
                            life,
                            inner: element,
                        },
                        b,
                    )?
                }
            }
            sealed::Intrinsic::StorePush => {
                let actual = self.expression(frame, &arguments[1], b, depth + 1)?;
                self.unify(actual, element, depth + 1, b)?;
                Self::push(&mut inputs, element, b)?;
                let unit = self.atom(Type::Unit, b)?;
                self.term(
                    Kind::ErrorUnion {
                        error: element,
                        result: unit,
                    },
                    b,
                )?
            }
            sealed::Intrinsic::StorePop => {
                let unit = self.atom(Type::Unit, b)?;
                self.term(
                    Kind::ErrorUnion {
                        error: unit,
                        result: element,
                    },
                    b,
                )?
            }
            sealed::Intrinsic::StoreLen | sealed::Intrinsic::StoreCapacity => {
                self.atom(Type::I64, b)?
            }
            _ => return Err(fail("original Store scheme selects another intrinsic")),
        };
        b.lookup(self.state.intrinsics.len(), WORK)?;
        if self
            .state
            .intrinsics
            .insert((frame.definition, node), intrinsic)
            .is_some()
        {
            return Err(fail("original Store operation repeats its sealed identity"));
        }
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        if self
            .state
            .intrinsic_inputs
            .insert((frame.definition, node), inputs)
            .is_some()
        {
            return Err(fail(
                "original Store operation repeats its complete input scheme",
            ));
        }
        Ok(result)
    }

    fn native_result_occurrence(
        &self,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<u32> {
        let original = self.inventory.definition(definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &original.body
        else {
            return Err(fail("original native result has no source body"));
        };
        let index = self
            .indexes
            .get(&definition)
            .ok_or_else(|| fail("original native result has no complete node index"))?;
        let mut selected = None;
        for link in site_nodes {
            b.spend(sites.len().saturating_add(1), WORK)?;
            let matches = match link.node {
                SemanticOriginalNode::Expression(id) => id == node,
                SemanticOriginalNode::Statement(id) => {
                    matches!(&index.get_stmt(id,b)?.kind,CoreStatementKind::Binding{initializer:Some(value),..} if value.id==node)
                }
            };
            if !matches {
                continue;
            }
            for site in sites {
                if site.id != link.site
                    || !matches!(
                        site.kind,
                        SemanticSiteRecipeKind::TypeUse { .. }
                            | SemanticSiteRecipeKind::DeclareLocal { .. }
                    )
                {
                    continue;
                }
                for row in &original.implicit_lifetimes {
                    b.spend(
                        row.occurrence_path
                            .len()
                            .saturating_add(site.occurrence_path.len() + 1),
                        WORK,
                    )?;
                    if row.occurrence_path == site.occurrence_path {
                        if selected.is_some_and(|old| old != row.ordinal) {
                            return Err(fail(
                                "original native result has competing declaration occurrences",
                            ));
                        }
                        selected = Some(row.ordinal);
                    }
                }
            }
        }
        selected.ok_or_else(||unknown("original borrowed native result lacks its complete original declaration occurrence"))
    }
}

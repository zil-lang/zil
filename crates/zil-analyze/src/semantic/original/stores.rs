//! Store schemes retain ownership of initialized entries. A borrowed element
//! retains its actual manager-view lifetime; a declaration occurrence does not
//! replace that source. It is never a copied whole element.
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
        let mut manager_life = None;
        let element = if new {
            match written {
                [CoreGenericArgument::Type(element)] => {
                    self.recipe(frame, element, b, depth + 1)?
                }
                [] => {
                    let receiving = self.receiving.last().copied();
                    let mut selected = None;
                    if let Some(receiving) = receiving {
                        let mut root = self.type_root(receiving, b)?;
                        for _ in depth..=b.partial_depth {
                            b.spend(1, WORK)?;
                            root = match self.state.arena[root] {
                                Kind::Wrapped(Wrapper::Managed, inner)
                                | Kind::Desc { inner, .. } => self.type_root(inner, b)?,
                                Kind::ErrorUnion { result, .. } => self.type_root(result, b)?,
                                Kind::Wrapped(Wrapper::Store, element) => {
                                    selected = Some(element);
                                    break;
                                }
                                _ => break,
                            };
                        }
                    }
                    // Infer from subsequent original uses if there is no
                    // receiving type. StoreElement remains a pending proof;
                    // an unconstrained variable never becomes Known0.
                    match selected {
                        Some(element) => element,
                        None => self.var(b)?,
                    }
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
            let descriptor = if matches!(
                self.state.arena[selected],
                Kind::Wrapped(Wrapper::Managed, _)
            ) {
                self.describe_argument(frame, &arguments[0], &place, selected, b, depth + 1)?
            } else {
                selected
            };
            let descriptor = self.type_root(descriptor, b)?;
            let Kind::Desc { life, .. } = self.state.arena[descriptor] else {
                return Err(fail(
                    "original Store view lacks its exact descriptor lifetime",
                ));
            };
            manager_life = Some(life);
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
                    let life = manager_life.ok_or_else(|| {
                        fail("original Store borrow lacks its complete manager source lifetime")
                    })?;
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
}

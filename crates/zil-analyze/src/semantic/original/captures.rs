//! Original capture mode and per-return remaining ownership. A move creator
//! permits consuming a capture once; it does not make it Copy or SyncDrop.
use super::*;

#[derive(Default)]
pub(super) struct CaptureState {
    reads: BTreeMap<SemanticDefinitionId, BTreeSet<usize>>,
    exits: BTreeMap<
        SemanticDefinitionId,
        Vec<Vec<(bool, bool, BTreeSet<Vec<u32>>, BTreeSet<Vec<u32>>)>>,
    >,
}
impl Solver<'_, '_> {
    pub(super) fn record_environment_read(
        &mut self,
        frame: &Frame,
        index: usize,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if index >= frame.inherited {
            let local = frame.locals.get(index)
                .ok_or_else(|| fail("original name read loses its lexical binding"))?;
            let Some(anchor) = local.anchor else {
                // A source-only synthetic binder has no physical local
                // receipt. Any actual transport query must still reject it.
                return Ok(());
            };
            b.lookup(self.state.local_reads.len(), WORK)?;
            if self.state.local_reads.insert((frame.definition, node), anchor)
                .is_some_and(|old| old != anchor)
            {
                return Err(fail("one original name reads competing lexical bindings"));
            }
            return Ok(());
        }
        let local = frame
            .locals
            .get(index)
            .ok_or_else(|| fail("original environment read loses its lexical binding"))?;
        let LocalAnchor::Environment(ordinal) = self.environment_anchor(frame, &local.name, b)?
        else {
            return Err(fail(
                "original captured name resolves outside its environment",
            ));
        };
        b.lookup(self.state.environment_reads.len(), WORK)?;
        if let Some(old) = self
            .state
            .environment_reads
            .insert((frame.definition, node), ordinal)
            && old != ordinal
        {
            return Err(fail("one original name reads several environment bindings"));
        }
        Ok(())
    }
    pub(super) fn owns_captures(
        &self,
        definition: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let Some((creator, expression)) = self.creators.get(&definition) else {
            return Ok(false);
        };
        b.lookup(self.indexes.len(), WORK)?;
        let value = self
            .indexes
            .get(creator)
            .ok_or_else(|| fail("original capture mode has no real creator body"))?
            .get_expr(*expression, b)?;
        let CoreExprKind::Closure { move_capture, .. } = value.kind else {
            return Err(fail("original capture mode selects another operation"));
        };
        Ok(move_capture)
    }
    pub(super) fn capture_read(
        &mut self,
        frame: &Frame,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !frame.alive || index >= frame.inherited || !self.owns_captures(frame.definition, b)? {
            return Ok(());
        }
        b.lookup(self.capture_state.reads.len(), WORK)?;
        let rows = self
            .capture_state
            .reads
            .entry(frame.definition)
            .or_default();
        b.lookup(rows.len(), WORK)?;
        rows.insert(index);
        Ok(())
    }
    pub(super) fn capture_exit(&mut self, frame: &Frame, b: &mut Budget) -> CompileResult<()> {
        if !frame.alive || frame.inherited == 0 || !self.owns_captures(frame.definition, b)? {
            return Ok(());
        }
        let mut exit = Vec::new();
        for local in frame.locals.iter().take(frame.inherited) {
            let vacant = self.copy_vacancies(&local.vacant, b)?;
            let possible = self.copy_vacancies(&local.maybe_vacant, b)?;
            Self::push(&mut exit, (local.live, local.used, vacant, possible), b)?;
        }
        b.lookup(self.capture_state.exits.len(), WORK)?;
        let exits = self
            .capture_state
            .exits
            .entry(frame.definition)
            .or_default();
        Self::push(exits, exit, b)
    }
    pub(super) fn capture_role(
        &mut self,
        definition: SemanticDefinitionId,
        kind: CallableKind,
        operation: CallKind,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !self.owns_captures(definition, b)? {
            return Ok(());
        }
        let frame = self
            .state
            .frames
            .get(&definition)
            .ok_or_else(|| fail("original capture role has no completed body frame"))?;
        if matches!(kind, CallableKind::Fn | CallableKind::FnMut) {
            if let Some(exits) = self.capture_state.exits.get(&definition) {
                for exit in exits {
                    for index in frame.captured.keys() {
                        let (_, _, _, possible) = exit.get(*index).ok_or_else(|| {
                            fail("original reusable capture exit omits its full availability state")
                        })?;
                        b.spend(possible.len().saturating_add(1), WORK)?;
                        if !possible.is_empty() {
                            return Err(fail(
                                "original reusable callable leaves a consumed captured field unrestored",
                            ));
                        }
                    }
                }
            }
            if let Some(reads) = self.capture_state.reads.get(&definition) {
                for index in reads {
                    b.spend(1, WORK)?;
                    let local = frame.locals.get(*index).ok_or_else(|| {
                        fail("original captured read leaves its real lexical inventory")
                    })?;
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: local.ty,
                            predicate: Predicate::Copy,
                        },
                        b,
                    )?;
                }
            }
        } else if kind == CallableKind::FnOnce {
            let mut remaining = Vec::new();
            if let Some(exits) = self.capture_state.exits.get(&definition) {
                for exit in exits {
                    for index in frame.captured.keys() {
                        let (live, _, vacant, _) = exit.get(*index).ok_or_else(|| {
                            fail("original capture exit omits a full live capture occurrence")
                        })?;
                        if *live {
                            let term = frame
                                .locals
                                .get(*index)
                                .ok_or_else(|| {
                                    fail("original capture exit has no declared whole type")
                                })?
                                .ty;
                            let vacant = self.copy_vacancies(vacant, b)?;
                            let local = &frame.locals[*index];
                            let anchor = self.environment_anchor(frame, &local.name, b)?;
                            Self::push(&mut remaining, (term, vacant, anchor), b)?;
                        }
                    }
                }
            }
            for (term, vacant, anchor) in remaining {
                let origin = cleanup::Origin::local(definition, Some(anchor), cleanup::Event::CaptureExit(operation));
                self.cleanup_partial(term, &[], &vacant, &origin, b, 0)?;
            }
        }
        Ok(())
    }
    pub(super) fn closure_copy(
        &self, definition: SemanticDefinitionId, b: &mut Budget, depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let declaration = self.inventory.definition(definition, b)?;
        if declaration.kind != SemanticDefinitionKind::Closure { return Ok(false); }
        b.lookup(self.state.frames.len(), WORK)?;
        let frame = self.state.frames.get(&definition)
            .ok_or_else(|| fail("closure Copy needs its completed original capture inventory"))?;
        let mut captured = Vec::new();
        for index in frame.captured.keys() {
            b.spend(1, WORK)?;
            let local = frame.locals.get(*index)
                .ok_or_else(|| fail("closure Copy leaves its complete original lexical capture domain"))?;
            Self::push(&mut captured, local.ty, b)?;
        }
        if self.owns_captures(definition, b)? {
            for term in captured {
                if !self.satisfies(term, Predicate::Copy, b, depth + 1)? { return Ok(false); }
            }
            return Ok(true);
        }
        // A non-move environment copies only independently shared capture
        // accesses. Exclusive capture borrows and writes retain affine state.
        b.lookup(self.mutating_closures.len(), WORK)?;
        if self.mutating_closures.contains(&definition) { return Ok(false); }
        for ((owner, node), source) in &self.state.borrows {
            b.spend(1, WORK)?;
            if *owner != definition || !matches!(source.anchor, Some(LocalAnchor::Environment(_))) { continue; }
            b.lookup(frame.expressions.len(), WORK)?;
            let value = frame.expressions.get(node)
                .ok_or_else(|| fail("closure Copy capture borrow lacks its real original expression"))?;
            if matches!(self.state.arena[self.type_root(*value, b)?], Kind::Address { mutable: true, .. }) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn known_capture_call(
        &mut self,
        frame: &mut Frame,
        callee: &SemanticOriginalExpr,
        definition: SemanticDefinitionId,
        requested_once: bool,
        operation: CallKind,
        borrowed: Option<bool>,
        place: Option<&borrow_places::Place>,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let name = if let CoreExprKind::Name(name) = &callee.kind { Some(name) } else { None };
        let mut once = requested_once;
        if let Some(reads) = self.capture_state.reads.get(&definition) {
            let body =
                self.state.frames.get(&definition).ok_or_else(|| {
                    fail("original callable consumption has no full checked body")
                })?;
            for index in reads {
                let term = body
                    .locals
                    .get(*index)
                    .ok_or_else(|| {
                        fail("original callable consumption omits an inherited occurrence")
                    })?
                    .ty;
                once |= !self.satisfies(term, Predicate::Copy, b, 0)?;
            }
        }
        if let Some(exits) = self.capture_state.exits.get(&definition) {
            let body =
                self.state.frames.get(&definition).ok_or_else(|| {
                    fail("original partial callable consumption has no checked body")
                })?;
            for exit in exits {
                for index in body.captured.keys() {
                    let (_, _, _, possible) = exit.get(*index).ok_or_else(|| {
                        fail("original partial callable consumption omits its capture exit")
                    })?;
                    b.spend(possible.len().saturating_add(1), WORK)?;
                    once |= !possible.is_empty();
                }
            }
        }
        if once {
            b.lookup(self.state.frames.len(), WORK)?;
            let result = self.state.frames.get(&definition)
                .ok_or_else(|| fail("consuming receiver has no completed original result"))?.result;
            let result = self.type_root(result, b)?;
            if let Kind::Address { life, .. } = self.state.arena[result]
                && matches!(self.state.arena[self.root(life, b)?],
                    Kind::Life(Life::ClosureReceiver(owner)) if owner == definition) {
                return Err(fail("consuming invocation cannot preserve its retired receiver result"));
            }
        }
        if let Some(mutable) = borrowed {
            if once || self.mutating_closures.contains(&definition) && !mutable {
                return Err(fail("borrowed original callable cannot consume its environment or mutate a shared head"));
            }
        }
        if !once && borrowed.is_none() && self.mutating_closures.contains(&definition)
            && place.is_some_and(|place| !place.mutable) {
            return Err(fail("original mutable callable requires its actual writable place"));
        }
        if once && !matches!(callee.kind, CoreExprKind::Name(_)) {
            if let Some(place) = place { self.consume_place(frame, place, b)?; }
        }
        if let Some(name) = name {
            let local = frame.locals.iter_mut().rev().find(|local| local.name == *name)
                .ok_or_else(|| fail("original callable invocation has no real local occupant"))?;
            b.spend(name.len(), WORK)?;
            if !local.initialized { return Err(fail("original callable receiver is uninitialized")); }
            if local.used {
                Self::push(&mut self.needs, Need { term: local.ty, predicate: Predicate::Copy }, b)?;
            }
            if once { local.used = true; local.live = false; }
        } else {
            // A genuine expression was evaluated once above and has its own
            // original identity. No fabricated lexical local represents it.
            b.lookup(frame.expressions.len(), WORK)?;
            if !frame.expressions.contains_key(&callee.id) {
                return Err(fail("temporary callable has no checked original expression"));
            }
        }
        self.capture_role(
            definition,
            if once {
                CallableKind::FnOnce
            } else if self.mutating_closures.contains(&definition) {
                CallableKind::FnMut
            } else {
                CallableKind::Fn
            },
            operation,
            b,
        )?;
        Ok(once)
    }
}

impl Checked {
    pub(in crate::semantic) fn capture_origin(
        &self,
        definition: SemanticDefinitionId,
        ordinal: u32,
        b: &mut Budget,
    ) -> CompileResult<Option<LocalAnchor>> {
        b.lookup(self.capture_origins.len(), WORK)?;
        Ok(self.capture_origins.get(&(definition, ordinal)).copied())
    }
    /// The checked original Name resolves to this local binder, even when a
    /// parameter shadows an equally named outer capture. No type/name match
    /// can substitute for that original lexical resolution.
    pub(in crate::semantic) fn local_recipe_anchor(
        &self,
        definition: SemanticDefinitionId,
        recipe: &SemanticSiteRecipe,
        b: &mut Budget,
    ) -> CompileResult<Option<LocalAnchor>> {
        let SemanticSiteRecipeKind::TypeUse {
            expected: SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Expression(node),
            },
        } = &recipe.kind else { return Ok(None); };
        self.local_read_anchor(definition, *node, b)
    }

    pub(in crate::semantic) fn local_read_anchor(
        &self,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<Option<LocalAnchor>> {
        b.lookup(self.local_reads.len(), WORK)?;
        Ok(self.local_reads.get(&(definition, node)).copied())
    }

    /// The original Γ selected this environment slot at the actual Name
    /// operation. A local shadow never enters this map, even with equal Type.
    pub(in crate::semantic) fn capture_recipe_ordinal(
        &self,
        definition: SemanticDefinitionId,
        recipe: &SemanticSiteRecipe,
        b: &mut Budget,
    ) -> CompileResult<Option<u32>> {
        let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind else {
            return Ok(None);
        };
        match expected {
            SemanticTypeRecipe::Environment {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => Ok(Some(*ordinal)),
            SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Expression(node),
            } => {
                b.lookup(self.environment_reads.len(), WORK)?;
                Ok(self.environment_reads.get(&(definition, *node)).copied())
            }
            _ => Ok(None),
        }
    }
}

impl Checked {
    pub(in crate::semantic) fn borrowed_closure_receiver(
        &self, context: &SemanticInstance, site: SemanticSiteId,
        program: &SemanticModule, inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<borrow_places::Source> {
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } =
            &inventory.definition(context.definition, b)?.body else {
                return Err(fail("borrowed closure receiver has no complete original body"));
            };
        let mut node = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site { continue; }
            let SemanticOriginalNode::Expression(id) = link.node else {
                return Err(fail("borrowed closure call selects an original statement"));
            };
            if node.replace(id).is_some() {
                return Err(fail("borrowed closure call repeats its original expression"));
            }
        }
        let node = node.ok_or_else(|| fail("borrowed closure call has no original expression"))?;
        b.lookup(self.calls.len(), WORK)?;
        let call = self.calls.get(&(context.definition, node))
            .ok_or_else(|| fail("callable receiver has no independently resolved original call"))?;
        let source = call.receiver.as_ref()
            .ok_or_else(|| fail("ordinary static call cannot invent a callable receiver place"))?;
        b.spend(source.path.len().saturating_add(8), WORK)?;
        Ok(borrow_places::Source { role: None, anchor: source.anchor, expression: source.expression,
            index: source.index, root: self.value(source.root, context, program, inventory, b, 0)?,
            inner: self.value(source.inner, context, program, inventory, b, 0)?,
            path: source.path.clone(), indirect: source.indirect, slice: source.slice })
    }
}

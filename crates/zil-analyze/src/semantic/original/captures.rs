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
                            Self::push(&mut remaining, (term, vacant), b)?;
                        }
                    }
                }
            }
            for (term, vacant) in remaining {
                self.cleanup_partial(term, &[], &vacant, b, 0)?;
            }
        }
        Ok(())
    }
    pub(super) fn known_capture_call(
        &mut self,
        frame: &mut Frame,
        callee: &SemanticOriginalExpr,
        definition: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let CoreExprKind::Name(name) = &callee.kind else {
            return Err(unknown(
                "owned original callable invocation needs its exact lexical head",
            ));
        };
        let mut once = false;
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
        let local = frame
            .locals
            .iter_mut()
            .rev()
            .find(|local| local.name == *name)
            .ok_or_else(|| fail("original callable invocation has no real local occupant"))?;
        b.spend(name.len(), WORK)?;
        if !local.initialized || local.used {
            return Err(fail(
                "original callable receiver is uninitialized or already consumed",
            ));
        }
        if once {
            local.used = true;
            local.live = false;
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
            b,
        )
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

//! Original capture mode and per-return remaining ownership. A move creator
//! permits consuming a capture once; it does not make it Copy or SyncDrop.
use super::*;

#[derive(Default)]
pub(super) struct CaptureState {
    reads: BTreeMap<SemanticDefinitionId, BTreeSet<usize>>,
    exits: BTreeMap<SemanticDefinitionId, Vec<Vec<(bool, bool)>>>,
}
impl Solver<'_, '_> {
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
            Self::push(&mut exit, (local.initialized, local.used), b)?;
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
            if let Some(exits) = self.capture_state.exits.get(&definition) {
                for exit in exits {
                    for index in frame.captured.keys() {
                        let (initialized, used) = *exit.get(*index).ok_or_else(|| {
                            fail("original capture exit omits a full live capture occurrence")
                        })?;
                        if initialized && !used {
                            let term = frame
                                .locals
                                .get(*index)
                                .ok_or_else(|| {
                                    fail("original capture exit has no declared whole type")
                                })?
                                .ty;
                            Self::push(
                                &mut self.needs,
                                Need {
                                    term,
                                    predicate: Predicate::SyncDrop,
                                },
                                b,
                            )?;
                        }
                    }
                }
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

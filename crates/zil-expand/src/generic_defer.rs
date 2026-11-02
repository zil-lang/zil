//! Ordinary cleanup boundaries in the existing parametric checker. Actual
//! capture layout, loans and executable fulfillment remain independently checked.
use super::*;

pub(super) struct Captures {
    outer: BTreeSet<usize>,
    read: BTreeSet<usize>,
    consumed: BTreeSet<usize>,
}
fn control(span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-DEFER-CONTROL", Some(span), message)
}
impl Check<'_, '_, '_> {
    pub(super) fn defer_control(span: Span, message: &str) -> Diagnostic {
        control(span, message)
    }
    pub(super) fn defer_use(&mut self, id: usize, consumed: bool, span: Span) -> R<()> {
        let Some(captures) = &mut self.defer_captures else {
            return Ok(());
        };
        self.budget.node(span, 0)?;
        if captures.outer.contains(&id) {
            self.budget.node(span, 0)?;
            captures.read.insert(id);
            if consumed {
                self.budget.node(span, 0)?;
                captures.consumed.insert(id);
            }
        }
        Ok(())
    }
    pub(super) fn defer_held(
        &mut self,
        state: &State,
        id: usize,
        writing: bool,
        span: Span,
    ) -> R<()> {
        if !writing || state.defer_holds.is_empty() {
            return Ok(());
        }
        self.budget.node(span, 0)?;
        if state.defer_holds.contains(&id) {
            return Err(error(
                span,
                "a deferred cleanup still holds this generic position",
            ));
        }
        Ok(())
    }
    pub(super) fn defer_statement(&mut self, body: &Block, state: &mut State, span: Span) -> R<()> {
        if self.defer_depth != 0 {
            return Err(Diagnostic::error(
                Phase::Expand,
                "E-DEFER-NESTED",
                Some(span),
                "nested defer is not supported",
            ));
        }
        self.budget.node(span, 0)?;
        let mut child = self.clone_loop_state(state, span)?;
        let mut outer = BTreeSet::new();
        for local in state.locals.values() {
            self.budget.node(span, 0)?;
            outer.insert(local.id);
            if let Some(roots) = &local.description_roots {
                for root in roots.iter() {
                    self.budget.node(span, 0)?;
                    outer.insert(*root);
                }
            }
        }
        let saved_result = std::mem::replace(&mut self.result, unit(span));
        let saved_loops = std::mem::take(&mut self.loops);
        let saved_unsafe = std::mem::replace(&mut self.unsafe_scope, false);
        let saved_body = std::mem::replace(&mut self.defer_body, true);
        self.defer_depth += 1;
        self.defer_captures = Some(Captures {
            outer,
            read: BTreeSet::new(),
            consumed: BTreeSet::new(),
        });
        let checked = self.block(body, &mut child);
        let mut captures = self.defer_captures.take().expect("cleanup capture context");
        self.defer_depth -= 1;
        self.defer_body = saved_body;
        self.result = saved_result;
        self.loops = saved_loops;
        self.unsafe_scope = saved_unsafe;
        checked?;
        // A real owning use transfers at registration, not at eventual cleanup.
        // Include direct Package field positions created while checking the body.
        for ((owner, _), slot) in &self.package_slots {
            self.budget.node(span, 0)?;
            if captures.outer.contains(owner) && child.moved.contains(slot) {
                self.budget.node(span, 0)?;
                captures.consumed.insert(*slot);
            }
        }
        for id in &captures.consumed {
            self.budget.node(span, 0)?;
            self.alias_invalidate(state, *id, None, span)?;
            state.moved.insert(*id);
            if let Some(origins) = state.store_origins.get(id) {
                for _ in origins {
                    self.budget.node(span, 0)?;
                }
                state.invalidated_stores.extend(origins);
            }
        }
        for id in captures.read {
            self.budget.node(span, 0)?;
            if !captures.consumed.contains(&id) {
                state.defer_holds.insert(id);
            }
        }
        Ok(())
    }
}

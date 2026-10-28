//! Parametric loop exits carry real state. A continue is a backedge, a break
//! contributes to the post-loop join, and neither is a function return.
use super::*;

pub(super) struct BlockFlow {
    pub(super) fallthrough: bool,
    pub(super) returns: bool,
}
impl BlockFlow {
    pub(super) const NEXT: Self = Self {
        fallthrough: true,
        returns: false,
    };
}

#[derive(Default)]
pub(super) struct LoopFrame {
    breaks: Vec<State>,
    continues: Vec<State>,
}

impl Check<'_, '_, '_> {
    pub(super) fn clone_loop_state(&mut self, state: &State, span: Span) -> R<State> {
        // clone_state pays the types, origins' contents, invalidated positions,
        // fresh positions and owning success results. Also charge names and
        // the sets which can retain ids hidden by newer lexical bindings.
        for name in state.locals.keys() {
            self.budget.text(name, span)?;
        }
        for _ in state
            .moved
            .iter()
            .chain(&state.store_owners)
            .chain(state.store_origins.keys())
        {
            self.budget.node(span, 0)?;
        }
        self.clone_state(state)
    }

    pub(super) fn loop_control(&mut self, state: &State, continuing: bool, span: Span) -> R<()> {
        if self.loops.is_empty() {
            return Err(Diagnostic::error(
                Phase::Expand,
                "E-LOOP-CONTROL",
                Some(span),
                "loop control outside a loop",
            ));
        }
        // Pay the collection entry and all state components before cloning.
        self.budget.node(span, self.loops.len())?;
        let snapshot = self.clone_loop_state(state, span)?;
        let frame = self.loops.last_mut().expect("loop checked above");
        if continuing {
            frame.continues.push(snapshot);
        } else {
            frame.breaks.push(snapshot);
        }
        Ok(())
    }

    pub(super) fn loop_body(
        &mut self,
        body: &Block,
        state: &mut State,
        mut iteration: State,
        span: Span,
    ) -> R<BlockFlow> {
        let depth = self.loops.len().checked_add(1).ok_or_else(|| {
            Diagnostic::limit(Phase::Expand, "generic loop depth overflow", Some(span))
        })?;
        self.budget.node(span, depth)?;
        self.loops.push(LoopFrame::default());
        let result = self.block(body, &mut iteration);
        let frame = self.loops.pop().expect("loop frame pushed above");
        let body_flow = result?;

        // The original outer state is still in `state`: body-local bindings
        // and shadowed names exist only in iteration/exit snapshots. Compare
        // by the outer ids, including their managed field slots.
        for continuation in &frame.continues {
            self.budget.node(span, 0)?;
            self.loop_invariant(state, continuation, body.span)?;
        }
        if body_flow.fallthrough {
            self.loop_invariant(state, &iteration, body.span)?;
        }

        // A return-only body has no surviving iteration state. Other exits
        // retain the zero-iteration path plus every break and checked backedge.
        // Keeping the latter also preserves may-invalidated Store facts.
        if body_flow.fallthrough || !frame.breaks.is_empty() || !frame.continues.is_empty() {
            let count = frame
                .breaks
                .len()
                .checked_add(frame.continues.len())
                .and_then(|count| count.checked_add(1 + usize::from(body_flow.fallthrough)))
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Expand,
                        "generic loop exit count overflow",
                        Some(span),
                    )
                })?;
            for _ in 0..count {
                self.budget.node(span, 0)?;
            }
            let entry = self.clone_loop_state(state, span)?;
            let mut exits = Vec::with_capacity(count);
            exits.push(&entry);
            if body_flow.fallthrough {
                exits.push(&iteration);
            }
            exits.extend(&frame.continues);
            exits.extend(&frame.breaks);
            self.join_states(state, &exits, span)?;
        }
        // As before, a while/range can execute zero times. A loop-control
        // exit therefore never satisfies the function's return requirement.
        Ok(BlockFlow {
            fallthrough: true,
            returns: body_flow.returns,
        })
    }
}

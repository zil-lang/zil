//! The registration captures a lexical slot by reference or moves its value
//! into this action. The action can affect phases only by running its real
//! child body when the enclosing activation exits.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Capture {
    Slot(usize),
    Owned(Data),
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Action {
    child: SemanticDefinitionId,
    fields: Vec<(String, Capture)>,
}

impl Action {
    pub(super) fn copy(&self, b: &mut Budget) -> CompileResult<Self> {
        let mut fields = Vec::new();
        for (name, field) in &self.fields {
            b.spend(name.len(), WORK)?;
            let value = match field {
                Capture::Slot(slot) => Capture::Slot(*slot),
                Capture::Owned(data) => Capture::Owned(data.copy(b)?),
            };
            Solver::push(&mut fields, (name.clone(), value), b)?;
        }
        Ok(Self { child: self.child, fields })
    }

    pub(super) fn invalidate(&mut self, b: &mut Budget) -> CompileResult<()> {
        b.spend(self.fields.len(), WORK)?;
        for (_, field) in &mut self.fields {
            if let Capture::Owned(data) = field {
                b.spend(data.outcomes.len(), WORK)?;
                data.outcomes.clear();
            }
        }
        Ok(())
    }

    pub(super) fn join(&mut self, other: &Self, b: &mut Budget) -> CompileResult<()> {
        if self.child != other.child || self.fields.len() != other.fields.len() {
            return Err(fail("original branch joins different registered cleanup actions"));
        }
        for ((name, field), (other_name, next)) in self.fields.iter_mut().zip(&other.fields) {
            b.spend(name.len().saturating_add(other_name.len() + 1), WORK)?;
            if name != other_name { return Err(fail("original join changes a registered capture")); }
            match (field, next) {
                (Capture::Slot(a), Capture::Slot(z)) if a == z => {}
                (Capture::Owned(a), Capture::Owned(z)) => a.join_value(z, b)?,
                _ => return Err(fail("original join changes the registered capture's actual source")),
            }
        }
        Ok(())
    }
}

impl Proof<'_, '_> {
    pub(super) fn register_action(&self, state: &mut State, body: &SemanticOriginalBlock, b: &mut Budget) -> CompileResult<()> {
        let (action, _, _) = body.deferred_action().ok_or_else(|| fail("original registration lacks a lexical action"))?;
        let index = self.inventory.original_index(self.definition.id, b)?;
        let site = index.event_site(action.id, crate::semantic::original_index::EventRole::Closure, b)?
            .ok_or_else(|| fail("registration has no original closure creation site"))?;
        let SemanticSiteRecipeKind::Closure { definition, .. } = index.recipe(site, b)?.kind else {
            return Err(fail("defer operand has another original operation"));
        };
        b.lookup(self.checked.frames.len(), WORK)?;
        let frame = self.checked.frames.get(&definition).ok_or_else(|| fail("registered action lacks its checked child frame"))?;
        let mut fields = Vec::new();
        for &position in frame.captured.keys() {
            b.spend(1, WORK)?;
            let binding = frame.locals.get(position).filter(|_| position < frame.inherited)
                .ok_or_else(|| fail("defer capture has no inherited binding at its lexical creation"))?;
            let mut source = None;
            for (slot, (name, _)) in state.locals.iter().enumerate().rev() {
                b.spend(name.len().saturating_add(binding.name.len() + 1), WORK)?;
                if name == &binding.name { source = Some(slot); break; }
            }
            let slot = source.ok_or_else(|| fail("defer capture has no live binder at registration"))?;
            let field = if binding.borrowed { Capture::Slot(slot) }
                else { Capture::Owned(std::mem::take(&mut state.locals[slot].1)) };
            b.spend(binding.name.len(), WORK)?;
            Solver::push(&mut fields, (binding.name.clone(), field), b)?;
        }
        Solver::push(&mut state.deferred, Action { child: definition, fields }, b)
    }

    pub(super) fn unwind(&self, state: &mut State, retained: usize, b: &mut Budget, depth: usize) -> CompileResult<()> {
        if retained > state.deferred.len() { return Err(fail("cleanup loses an enclosing registered action")); }
        while state.deferred.len() > retained {
            self.step(b, depth)?;
            let action = state.deferred.pop().ok_or_else(|| fail("original cleanup registration disappeared"))?;
            let definition = self.inventory.definition(action.child, b)?;
            let SemanticDefinitionBody::FunctionWithBody { body: SemanticOriginalFunctionBody::Defined(body), .. } =
                &definition.body else { return Err(fail("registered cleanup has no executable original body")); };
            let mut locals = Vec::new();
            let mut writeback = Vec::new();
            for (name, field) in action.fields {
                let data = match field {
                    Capture::Owned(data) => data,
                    Capture::Slot(slot) => {
                        let (outer_name, data) = state.locals.get(slot).ok_or_else(||
                            fail("cleanup capture outlives its original slot"))?;
                        b.spend(name.len().saturating_add(outer_name.len()), WORK)?;
                        if outer_name != &name { return Err(fail("cleanup capture binds another lexical slot")); }
                        Solver::push(&mut writeback, (locals.len(), slot), b)?;
                        data.copy(b)?
                    }
                };
                Solver::push(&mut locals, (name, data), b)?;
            }
            let child = Proof { definition, root: self.root, births: self.births.clone(), inventory: self.inventory,
                checked: self.checked, contract: None, input: None, loops: Default::default() };
            let mut exit = State { phases: std::mem::take(&mut state.phases), locals, alive: true,
                cleanup_unknown: false, deferred: Vec::new(), invalidated: false };
            child.block(&mut exit, body, b, depth + 1)?;
            if !exit.alive { return Err(fail("original cleanup does not complete its synchronous action")); }
            if exit.invalidated { state.invalidate(b)?; }
            state.phases = exit.phases;
            state.cleanup_unknown |= exit.cleanup_unknown;
            for (field, outer) in writeback {
                b.spend(1, WORK)?;
                state.locals[outer].1 = std::mem::take(&mut exit.locals[field].1);
            }
        }
        Ok(())
    }
}

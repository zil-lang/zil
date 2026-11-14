//! A declaration chooses an effect upper, never an effect proof. All original
//! branches are checked before this table is consumed. A call executes its
//! proven callee/Callable upper; constructing a Closure does not execute it.
use super::*;

impl CheckedOriginal {
    pub(super) fn execute_effect(&mut self,env:&Env,kind:CallKind,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        let old=self.executing_effects.entry(env.current()?).or_insert(CallKind::Direct);
        if kind==CallKind::Suspend{*old=CallKind::Suspend;}
        Ok(())
    }
    pub(super) fn check_effects(&self,b:&mut Budget)->Check<()> {
        // Every admitted cleanup edge has already proved original SyncDrop.
        // It therefore adds no Suspend edge. Unknown cleanup was rejected,
        // rather than omitted from this upper.
        for (id,prototype) in &self.prototypes {
            b.spend(1)?;
            if !self.checked.contains(id){return Err(bad("original effect graph contains an unchecked body"));}
            if prototype.call_kind==CallKind::Direct&&self.executing_effects.get(id)==Some(&CallKind::Suspend){return Err(bad("original body may suspend beyond its fixed Direct call convention"));}
        }
        // Static recursive edges use fixed callee uppers. Checking every body
        // against that same upper closes the graph without treating an active
        // or missing callee body as a Direct implementation.
        for target in self.call_targets.values(){b.spend(1)?;if !self.checked.contains(target){return Err(bad("original call effect target has no universally checked body"));}}
        Ok(())
    }
}

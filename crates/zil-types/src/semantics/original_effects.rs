//! Original body effects are checked against the fixed Source ABI upper bound.
//! A wider Suspend declaration is retained even for a Direct implementation.
use super::*;
fn spend(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original effect graph work exceeded", None)
    })?;
    Ok(())
}
impl Check<'_, '_> {
    pub(super) fn finish_effects(&mut self) -> R<()> {
        self.pay(self.program.definitions.len(), 0)?;
        let mut effects = BTreeMap::new();
        for definition in &self.program.definitions {
            if let SemanticDefinitionBody::FunctionWithBody {
                signature, body, ..
            } = &definition.body
            {
                let initial = if matches!(body, SemanticOriginalFunctionBody::Defined(_)) {
                    CallKind::Direct
                } else {
                    signature.call_kind
                };
                effects.insert(definition.id, initial);
            }
        }
        let mut changed = true;
        let mut rounds = 0usize;
        while changed {
            self.pay(1, rounds)?;
            if rounds > effects.len() {
                return Err(bad("original effect lattice does not converge"));
            }
            rounds += 1;
            changed = false;
            for (key, target) in &self.proof.call_targets {
                spend(self.left, 1)?;
                if self.proof.compile_time_calls.contains(key) {
                    continue;
                }
                spend(self.left, self.program.definitions.len())?;
                let definition = self
                    .program
                    .definitions
                    .iter()
                    .find(|definition| definition.id == *target)
                    .ok_or_else(|| bad("original effect call edge loses its real definition"))?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
                else {
                    return Err(bad("original effect edge names noncode"));
                };
                let effect = *effects
                    .get(target)
                    .ok_or_else(|| bad("original callee effect remains unresolved"))?;
                if signature.call_kind == CallKind::Suspend || effect == CallKind::Suspend {
                    if self.proof.cleanup_calls.contains(key) {
                        return Err(bad(
                            "original deferred cleanup invokes a Suspend convention",
                        ));
                    }
                    let caller = effects
                        .get_mut(&key.0)
                        .ok_or_else(|| bad("original effect caller is absent"))?;
                    if *caller != CallKind::Suspend {
                        *caller = CallKind::Suspend;
                        changed = true;
                    }
                }
            }
            for (key, call) in &self.proof.intrinsics {
                spend(self.left, 1)?;
                if self.proof.compile_time_calls.contains(key) {
                    continue;
                }
                if call.effect == CallKind::Suspend {
                    if self.proof.cleanup_calls.contains(key) {
                        return Err(bad(
                            "original deferred cleanup executes a suspending intrinsic",
                        ));
                    }
                    let caller = effects
                        .get_mut(&key.0)
                        .ok_or_else(|| bad("original intrinsic effect owner is absent"))?;
                    if *caller != CallKind::Suspend {
                        *caller = CallKind::Suspend;
                        changed = true;
                    }
                }
            }
            for (key, call) in &self.proof.callable_calls {
                spend(self.left, 1)?;
                if self.proof.compile_time_calls.contains(key) {
                    continue;
                }
                if call.signature.call_kind == CallKind::Suspend {
                    if self.proof.cleanup_calls.contains(key) {
                        return Err(bad("original deferred cleanup invokes a Suspend callable"));
                    }
                    let caller = effects
                        .get_mut(&key.0)
                        .ok_or_else(|| bad("original callable effect owner is absent"))?;
                    if *caller != CallKind::Suspend {
                        *caller = CallKind::Suspend;
                        changed = true;
                    }
                }
            }
        }
        for definition in &self.program.definitions {
            self.pay(1, 0)?;
            if let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body {
                let effect = effects
                    .get(&definition.id)
                    .ok_or_else(|| bad("original code has no complete effect judgment"))?;
                if signature.call_kind == CallKind::Direct && *effect == CallKind::Suspend {
                    return Err(bad(
                        "original body exceeds its declared Direct ABI upper bound",
                    ));
                }
            }
        }
        self.proof.effective_call_kind = effects;
        Ok(())
    }
}

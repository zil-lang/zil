//! A propagating return belongs to the current original function. Its failure
//! and normal exits constrain the two independent arms of that same result.
use super::*;

impl CheckedOriginal {
    pub(super) fn injection_literal_context(
        &mut self,
        expression: &SemanticOriginalExpr,
        error: &Ty,
        success: &Ty,
        b: &mut Budget,
    ) -> Check<Option<Ty>> {
        if !Self::contextual_numeric_literal(expression, b)? {
            return Ok(None);
        }
        let integer = Self::untyped_numeric_tree(expression, true, b)?;
        let accepts = |ty: &Ty| matches!(ty,Ty::Atom(atom) if if integer {atom.is_integer()} else {atom.is_float()});
        match (accepts(error), accepts(success)) {
            (true, false) => Ok(Some(error.clone())),
            (false, true) => Ok(Some(success.clone())),
            _ if integer && (matches!(error, Ty::Infer(_)) || matches!(success, Ty::Infer(_))) => {
                Ok(Some(self.fresh_integer(b)?))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn prepare_fallible_result(
        &mut self,
        owner: SemanticDefinitionId,
        result: &Ty,
        b: &mut Budget,
    ) -> Check<()> {
        if self.propagating_bodies.contains(&owner)
            && matches!(self.resolved(result, b)?, Ty::Infer(_))
        {
            let error = self.fresh(b)?;
            let success = self.fresh(b)?;
            self.unify(
                result,
                &Ty::ErrorUnion(Box::new(error), Box::new(success)),
                b,
            )?;
        }
        Ok(())
    }

    pub(super) fn propagate_error(&mut self, value: Ty, env: &Env, b: &mut Budget) -> Check<Ty> {
        let Ty::ErrorUnion(error, success) = self.resolved(&value, b)? else {
            return Err(bad("original propagation has no ErrorUnion operand"));
        };
        let owner = env.current()?;
        if self.in_defer.get(&owner).copied().unwrap_or(0) != 0 {
            return Err(bad(
                "original deferred cleanup cannot propagate an error from its caller",
            ));
        }
        let outgoing = self
            .results
            .get(&owner)
            .ok_or_else(|| bad("original propagation has no enclosing result"))?;
        let Ty::ErrorUnion(expected, _) = self.resolved(outgoing, b)? else {
            return Err(bad(
                "original propagation cannot return an error from its enclosing result",
            ));
        };
        self.unify(&error, &expected, b)?;
        self.queue_cleanup(env, None, b)?;
        Ok(*success)
    }

    // An early return can precede the first propagating call. Keep its
    // original expression and payload until all error alternatives are known.
    pub(super) fn finish_return_injections(
        &mut self,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> Check<()> {
        let Some(pending) = self.pending_injections.remove(&owner) else {
            return Ok(());
        };
        for (_, payload, sum) in &pending {
            let Ty::ErrorUnion(error, success) = self.resolved(sum, b)? else {
                return Err(bad("original pending return lost its ErrorUnion"));
            };
            if matches!(*error, Ty::Infer(_)) {
                return Err(bad(
                    "original propagation leaves its error type unconstrained",
                ));
            }
            let payload = self.resolved(payload, b)?;
            if matches!(payload, Ty::ErrorUnion(..))
                && !self.same(&payload, &error, b)?
                && !self.same(&payload, &success, b)?
            {
                self.unify(&payload, sum, b)?;
            } else if !self.same(&payload, &error, b)?
                && (!matches!(payload, Ty::Infer(_))
                    || self.integer_variable(&payload) && !self.integer_carrier(&error))
            {
                self.unify(&payload, &success, b)?;
            }
        }
        for (node, payload, sum) in pending {
            let Ty::ErrorUnion(error, success) = self.resolved(&sum, b)? else {
                unreachable!()
            };
            if self.same(&payload, &sum, b)? {
                continue;
            }
            let failed = self.same(&payload, &error, b)?;
            let succeeded = self.same(&payload, &success, b)?;
            if failed == succeeded {
                return Err(bad(
                    "original return payload fails to select one proved error or success arm",
                ));
            }
            b.spend(1)?;
            self.injections.insert((owner, node), (failed, payload));
        }
        Ok(())
    }

    // Opaque result requirements describe the public callable. Only the
    // actual closure returned in the defining result position receives its
    // context. Quantified and Receiver roles retain their own binder proofs.
    pub(super) fn contextual_opaque_closure(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        child: SemanticDefinitionId,
        expected: Option<&Ty>,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        let owner = env.current()?;
        let Some(expected) = expected else {
            return Ok(());
        };
        if self.results.get(&owner) != Some(expected) {
            return Ok(());
        }
        let parent = context.definition(module, owner, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &parent.body else {
            return Ok(());
        };
        if !matches!(signature.result,SemanticTypeRecipe::Opaque{definition,..} if definition==owner)
        {
            return Ok(());
        }
        for requirement in &parent.requirements {
            b.spend(1)?;
            let SemanticRequirementRecipe::Callable {
                ty:
                    SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Result,
                    },
                kind,
                signature,
            } = requirement
            else {
                continue;
            };
            if signature.bound_lifetimes != 0
                || signature.invocation_lifetimes != 0
                || Self::receiver_signature_recipe(signature)
            {
                continue;
            }
            let wanted =
                self.callable_signature(module, context, signature, *kind, env, active, b)?;
            if let Some(value) = self.results.get(&child).cloned() {
                self.unify(&value, &wanted.result, b)?;
            } else {
                self.results.insert(child, wanted.result);
            }
            for (i, parameter) in wanted.parameters.into_iter().enumerate() {
                b.spend(1)?;
                let key = (child, i as u32);
                if let Some(value) = self.parameters.get(&key).cloned() {
                    self.unify(&value, &parameter, b)?;
                } else {
                    self.parameters.insert(key, parameter);
                }
            }
        }
        Ok(())
    }
}

//! Structural consequences of already established original caller premises.
//! This query introduces no equations, substitutions, permissions or closed
//! context assumption. Actual layout and cleanup consumers remain separate.
use super::*;

impl CheckedOriginal {
    pub(super) fn structural_premise(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        ty: &Ty,
        bound: DynStructuralBound,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<bool> {
        let caller = env.current()?;
        if self.current != Some(caller) {
            return Err(bad("structural premise query has no actual original caller context"));
        }
        let ty = self.resolved(ty, b)?;
        if has_infer(&ty, &self.substitutions, b)? {
            return Ok(false);
        }
        // The complete lexical chain is checked before any successful lookup.
        // A target's requirements cannot be inserted as a hypothetical frame.
        b.spend(env.lexical.len())?;
        let mut seen = BTreeSet::new();
        for (index, frame) in env.lexical.iter().enumerate() {
            let definition = context.definition(module, frame.definition, b)?;
            b.spend(definition.parameters.len().saturating_add(definition.environment.len()).saturating_add(1))?;
            if !seen.insert(frame.definition)
                || definition.parent != env.lexical.get(index + 1).map(|parent| parent.definition)
                || frame.actuals.len() != definition.parameters.len()
                || frame.environment.len() != definition.environment.len()
            {
                return Err(bad("structural premise frame is not the complete real caller ancestry"));
            }
            for (formal, actual) in definition.parameters.iter().zip(&frame.actuals) {
                if !matches!((formal, actual),
                    (SemanticFormal::Type { .. }, Arg::Type(_))
                    | (SemanticFormal::Const { .. }, Arg::Const(_))
                    | (SemanticFormal::Lifetime, Arg::Life(_)))
                {
                    return Err(bad("structural premise frame changes an original mixed formal kind"));
                }
            }
        }
        for (index, frame) in env.lexical.iter().enumerate() {
            let definition = context.definition(module, frame.definition, b)?;
            // Each requirement is interpreted where it was declared. Keep all
            // mixed actuals in place; filtering only Type formals would shift
            // every following Const/Life/full-ordinal reference.
            let frames = env.lexical[index..].iter().fold(0usize, |total, frame| {
                total.saturating_add(frame.actuals.len()).saturating_add(frame.environment.len()).saturating_add(1)
            });
            let bounds = env.bounds.iter().fold(0usize, |total, frame| total.saturating_add(frame.len()));
            b.spend(frames.saturating_add(bounds).saturating_add(env.invocation.len()))?;
            let selected = Env {
                lexical: env.lexical[index..].to_vec(),
                bounds: env.bounds.clone(),
                invocation: env.invocation.clone(),
            };
            let mut requirements = Vec::new();
            b.spend(definition.requirements.len())?;
            requirements.extend(definition.requirements.iter().rev());
            for formal in &definition.parameters {
                if let SemanticFormal::Type { requirements: rows } = formal {
                    b.spend(rows.len())?;
                    requirements.extend(rows.iter().rev());
                }
            }
            while let Some(requirement) = requirements.pop() {
                b.spend(1)?;
                let (receiver, established) = match requirement {
                    SemanticRequirementRecipe::Structural { ty, bound } => (ty, *bound),
                    SemanticRequirementRecipe::Builtin { ty, trait_kind } => {
                        let established = match trait_kind {
                            SemanticBuiltinTrait::Copy => DynStructuralBound::Copy,
                            SemanticBuiltinTrait::Value => DynStructuralBound::Value,
                            SemanticBuiltinTrait::SyncDrop => DynStructuralBound::SyncDrop,
                            SemanticBuiltinTrait::StoreElement => DynStructuralBound::StoreElement,
                            // A user trait or another sealed operational role
                            // is never structural evidence by its spelling.
                            _ => continue,
                        };
                        (ty, established)
                    }
                    SemanticRequirementRecipe::ForAll { lifetimes: 0, requirements: rows } => {
                        b.spend(rows.len())?;
                        requirements.extend(rows.iter().rev());
                        continue;
                    }
                    // Nonzero quantifiers need a real rigid instantiation.
                    // Do not erase their frame to discover a convenient bound.
                    SemanticRequirementRecipe::ForAll { .. }
                    | SemanticRequirementRecipe::Trait { .. }
                    | SemanticRequirementRecipe::Callable { .. }
                    | SemanticRequirementRecipe::TypeEqual { .. }
                    | SemanticRequirementRecipe::ConstEqual { .. }
                    | SemanticRequirementRecipe::LifeEqual { .. }
                    | SemanticRequirementRecipe::Outlives { .. } => continue,
                };
                if !entails(established, bound) {
                    continue;
                }
                // Recipe interpretation is permitted to read existing solved
                // variables, but this query must never solve a new one. Pay
                // for the full snapshot before cloning, and reject any change
                // even when candidate interpretation itself returns an error.
                b.spend(self.substitutions.len())?;
                for value in self.substitutions.values() {
                    let _ = self.resolved(value, b)?;
                }
                let snapshot = self.substitutions.clone();
                let receiver = self.ty(module, context, receiver, &selected, active, b);
                if self.substitutions != snapshot {
                    return Err(bad("original structural premise query attempted type inference"));
                }
                let receiver = receiver?;
                if has_infer(&receiver, &self.substitutions, b)? {
                    continue;
                }
                if self.same(&ty, &receiver, b)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

fn entails(established: DynStructuralBound, requested: DynStructuralBound) -> bool {
    established == requested
        || matches!((established, requested),
            (DynStructuralBound::Copy | DynStructuralBound::StoreElement,
             DynStructuralBound::Value | DynStructuralBound::SyncDrop))
}

// Include Const carriers and both closure argument/environment vectors. An
// unresolved class of type variables cannot become evidence merely because
// the same placeholder occurs on both sides of a premise query.
enum Term<'a> {
    Type(&'a Ty),
    Argument(&'a Arg),
    Constant(&'a Const),
    Leave(u64),
}
fn has_infer(ty: &Ty, substitutions: &BTreeMap<u64, Ty>, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![Term::Type(ty)];
    let mut active = BTreeSet::new();
    while let Some(term) = pending.pop() {
        b.spend(1)?;
        match term {
            Term::Type(ty) => match ty {
                Ty::Infer(id) => {
                    let Some(value) = substitutions.get(id) else { return Ok(true); };
                    if !active.insert(*id) {
                        return Err(bad("cyclic original type substitution in structural premise query"));
                    }
                    pending.push(Term::Leave(*id));
                    pending.push(Term::Type(value));
                },
                Ty::Tuple(fields) => {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(Term::Type));
                }
                Ty::Address { inner, .. } | Ty::Desc { inner, .. }
                | Ty::Managed(inner) | Ty::Store(inner) | Ty::Slice(inner) => pending.push(Term::Type(inner)),
                Ty::Array(inner, length) => {
                    pending.push(Term::Type(inner));
                    pending.push(Term::Constant(length));
                }
                Ty::ErrorUnion(error, result) => {
                    pending.push(Term::Type(error));
                    pending.push(Term::Type(result));
                }
                Ty::Nominal(_, arguments) => {
                    b.spend(arguments.len())?;
                    pending.extend(arguments.iter().map(Term::Argument));
                }
                Ty::Closure(_, arguments, environment) => {
                    b.spend(arguments.len().saturating_add(environment.len()))?;
                    pending.extend(arguments.iter().chain(environment).map(Term::Argument));
                }
                Ty::Code { parameters, result, .. } => {
                    b.spend(parameters.len().saturating_add(1))?;
                    pending.extend(parameters.iter().map(Term::Type));
                    pending.push(Term::Type(result));
                }
                Ty::Carrier{child,retained,..}=>{pending.push(Term::Type(child));let mut types=Vec::new();retained.types(&mut types,b)?;pending.extend(types.into_iter().map(Term::Type));},
                Ty::SealedError(_) | Ty::Atom(_) | Ty::Variable(..) | Ty::SelfType(_) | Ty::Function(_) => {}
            },
            Term::Argument(Arg::Type(ty)) => pending.push(Term::Type(ty)),
            Term::Argument(Arg::Const(value)) => pending.push(Term::Constant(value)),
            Term::Argument(Arg::Life(_)) => {},
            Term::Leave(id) => { active.remove(&id); },
            Term::Constant(value) => {
                pending.push(Term::Type(&value.carrier));
                match &value.value {
                    ConstValue::Unary(_, value) | ConstValue::Convert(value) => pending.push(Term::Constant(value)),
                    ConstValue::Binary(_, left, right) => {
                        pending.push(Term::Constant(left));
                        pending.push(Term::Constant(right));
                    }
                    ConstValue::Variable(..) | ConstValue::Value(_) => {},
                }
            }
        }
    }
    Ok(false)
}

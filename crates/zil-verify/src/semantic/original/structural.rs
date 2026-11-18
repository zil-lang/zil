//! Structural consequences of already established original caller premises.
//! This query introduces no equations, substitutions, permissions or closed
//! context assumption. Actual layout and cleanup consumers remain separate.
use super::*;

impl CheckedOriginal {
    pub(super) fn proof_environment(&self,fallback:&Env,b:&mut Budget)->Check<Env>{
        let source=self.premise_environment.as_ref().unwrap_or(fallback);
        self.copy_environment(source,b)
    }
    pub(super) fn copy_environment(&self,source:&Env,b:&mut Budget)->Check<Env>{
        b.spend(source.lexical.len().saturating_add(source.bounds.len()).saturating_add(source.invocation.len()))?;
        let empty=Env{lexical:vec![],bounds:vec![],invocation:vec![],self_type:None};
        for frame in &source.lexical{
            // Pay for complete terms before copying; preserve the actual
            // mixed ordinals, declaration identities and lifetime frames.
            self.apply_arguments(&frame.actuals,&empty,0,b)?;
            self.apply_arguments(&frame.environment,&empty,0,b)?;
        }
        for bound in &source.bounds{b.spend(bound.len())?;}
        if let Some((_,ty))=&source.self_type{self.apply_type(ty,&empty,b)?;}
        Ok(source.clone())
    }
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
            return Err(bad(&format!("structural premise query has no actual original caller context [active {:?}, environment {caller:?}, bound {bound:?}, receiver {}]",self.current,type_category(ty))));
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
        if self.associated_structural_premise(module,context,&ty,bound,env,active,b)?{return Ok(true);}
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
                self_type: env.self_type.as_ref().map(|(owner,ty)|self.resolved(ty,b).map(|ty|(*owner,ty))).transpose()?,
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
                    // Every admitted callable receiver can occupy an ordinary Value
                    // position. Copy and cleanup require their own premises.
                    SemanticRequirementRecipe::Callable { ty, .. } => (ty, DynStructuralBound::Value),
                    SemanticRequirementRecipe::ForAll { lifetimes: 0, requirements: rows } => {
                        b.spend(rows.len())?;
                        requirements.extend(rows.iter().rev());
                        continue;
                    }
                    // Nonzero quantifiers need a real rigid instantiation.
                    // Do not erase their frame to discover a convenient bound.
                    SemanticRequirementRecipe::ForAll { .. }
                    | SemanticRequirementRecipe::Trait { .. }
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

pub(super) fn entails(established: DynStructuralBound, requested: DynStructuralBound) -> bool {
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
                Ty::Projection { receiver, arguments, .. } => {
                    pending.push(Term::Type(receiver));
                    b.spend(arguments.len())?;
                    pending.extend(arguments.iter().map(Term::Argument));
                }
                Ty::Nominal(_, arguments) | Ty::Opaque(_, arguments) => {
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
                Ty::Dyn(value)=>{
                    b.spend(value.application.arguments.len().saturating_add(value.associated.len()))?;
                    pending.extend(value.application.arguments.iter().map(Term::Argument));
                    match &value.context{DynContext::Closed=>{},DynContext::Type(ty)=>pending.push(Term::Type(ty)),DynContext::Shape(shape)=>{let mut types=Vec::new();shape.types(&mut types,b)?;pending.extend(types.into_iter().map(Term::Type));}}
                    for row in &value.associated{b.spend(row.application.arguments.len())?;pending.extend(row.application.arguments.iter().map(Term::Argument));pending.push(Term::Argument(&row.value));}
                },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_provider_constraints_are_not_original_caller_premises() {
        let limits=Limits::default();
        let (original,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        for (has_bound,wrong_scope,accept) in [(true,false,true),(false,false,false),(true,true,false)] {
            let mut module=original.clone();
            let program=module.semantic.as_mut().unwrap();
            let provider=program.definitions.iter_mut().find(|row|row.kind==SemanticDefinitionKind::Function&&matches!(row.parameters.as_slice(),[SemanticFormal::Type{..}])).unwrap();
            let provider_id=provider.id;
            let formal=SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0});
            let requirement=SemanticRequirementRecipe::Structural{ty:formal,bound:DynStructuralBound::StoreElement};
            provider.parameters=vec![SemanticFormal::Type{requirements:vec![requirement.clone()]}];
            provider.requirements.clear();
            let mut caller=provider.clone();
            caller.id.index=program.definitions.iter().filter(|row|row.id.module==provider_id.module).map(|row|row.id.index).max().unwrap()+1;
            caller.identity.declaration_path.push("independent_provider_caller".into());
            caller.parameters=vec![SemanticFormal::Type{requirements:if has_bound{vec![requirement.clone()]}else{vec![]}}];
            let caller_id=caller.id;
            program.definitions.push(caller);
            let mut budget=Budget::new(&limits);
            let context=CheckedContext::catalogue(&module,&mut budget).unwrap();
            let mut active=ActiveQueries::new();
            let mut checked=CheckedOriginal::default();
            checked.current=Some(caller_id);
            let caller=checked.environment(&module,&context,caller_id,&mut active,&mut budget).unwrap();
            let mut selected=checked.environment(&module,&context,provider_id,&mut active,&mut budget).unwrap();
            selected.lexical[0].actuals=vec![Arg::Type(Ty::Variable(if wrong_scope{provider_id}else{caller_id},0))];
            for nested in [false,true] {
                checked.premise_environment=if nested{Some(caller.clone())}else{None};
                let result=checked.obligation(&module,&context,&requirement,&selected,if nested{&selected}else{&caller},&mut active,&mut budget);
                assert_eq!(result.is_ok(),accept,"bound={has_bound}, wrong_scope={wrong_scope}, nested={nested}: {result:?}");
                assert_eq!(checked.premise_environment.is_some(),nested);
            }
        }
    }
}

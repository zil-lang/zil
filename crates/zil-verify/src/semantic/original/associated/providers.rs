//! Matching original impl schemes binds only their own formal ordinals. A
//! caller's rigid Type/Life/Const never becomes an inference variable, and a
//! selected impl's premises remain caller obligations.
use super::*;

pub(in crate::semantic::original) struct Provider { pub owner:SemanticDefinitionId, pub declaration:LifeDeclarationId, pub environment:Env }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Applicability {
    Match,
    Mismatch,
    Pending,
}
impl Applicability {
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Mismatch, _) | (_, Self::Mismatch) => Self::Mismatch,
            (Self::Pending, _) | (_, Self::Pending) => Self::Pending,
            (Self::Match, Self::Match) => Self::Match,
        }
    }
    fn then(self, check: impl FnOnce() -> Check<Self>) -> Check<Self> {
        if self == Self::Mismatch {
            return Ok(self);
        }
        Ok(self.and(check()?))
    }
    fn known(self) -> Check<bool> {
        match self {
            Self::Match => Ok(true),
            Self::Mismatch => Ok(false),
            Self::Pending => Err(unsupported("original impl selection cannot infer a caller Type", None)),
        }
    }
}

impl From<bool> for Applicability {
    fn from(matches: bool) -> Self {
        if matches { Self::Match } else { Self::Mismatch }
    }
}
impl CheckedOriginal {
    pub(in crate::semantic::original) fn original_receiver_applications(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,name:&str,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<(LifeDeclarationId,Vec<Arg>)>>{
        let programme=module.semantic.as_ref().ok_or_else(||bad("original concrete method lookup lacks its programme"))?;
        let mut applications=Vec::new();
        for definition in &programme.definitions{
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl{self_type,trait_application,..})=&definition.body else{continue;};
            b.spend(module.dyn_declarations.len().saturating_add(1))?;
            let Some(trait_decl)=module.dyn_declarations.iter().find(|row|row.id==trait_application.declaration)else{return Err(bad("original concrete method trait is absent"));};
            if !trait_decl.members.iter().any(|member|member.name==name&&matches!(member.kind,DynMemberKind::Method(_))){continue;}
            let mut selected=self.environment(module,context,definition.id,active,b)?;
            let pattern=self.ty(module,context,self_type,&selected,active,b)?;let mut bindings=BTreeMap::new();
            if !self.match_original_type(&pattern,receiver,definition.id,&mut bindings,0,b)?.known()?{continue;}
            let mut actuals=Vec::new();let mut complete=true;for ordinal in 0..definition.parameters.len(){if let Some(value)=bindings.remove(&(ordinal as u32)){actuals.push(value);}else{complete=false;break;}}
            if !complete{continue;}selected.lexical[0].actuals=actuals;
            let mut arguments=Vec::new();for recipe in &trait_application.arguments{arguments.push(self.actual(module,context,recipe,&selected,active,b)?);}
            if self.original_provider(module,context,receiver,trait_application.declaration,&arguments,caller,active,b)?.is_some(){applications.push((trait_application.declaration,arguments));}
        }
        Ok(applications)
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::original) fn original_provider(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,declaration:LifeDeclarationId,arguments:&[Arg],caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Provider>> {
        self.original_provider_for_application(module, context, receiver, declaration, arguments, caller, false, active, b)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn original_provider_for_application(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,declaration:LifeDeclarationId,arguments:&[Arg],caller:&Env,defer:bool,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Provider>> {
        let programme=module.semantic.as_ref().ok_or_else(||bad("original impl selection lacks its programme"))?;
        let mut selected=None;
        let mut pending=false;
        for definition in &programme.definitions {
            b.spend(1)?;
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl{declaration:implementation,self_type,trait_application})=&definition.body else{continue;};
            if trait_application.declaration!=declaration {continue;}
            if dynamic_witness::semantic_external::original_scope(module,*implementation,b)?.id!=definition.id{return Err(bad("original impl declaration selects another lexical owner"));}
            b.spend(module.dyn_declarations.len())?;
            let mut sources=module.dyn_declarations.iter().filter(|row|row.id==*implementation);
            let source=sources.next().ok_or_else(||bad("original impl has no complete raw declaration"))?;
            if sources.next().is_some()||source.kind!=DynDeclarationKind::Impl{return Err(bad("original impl raw identity or kind differs"));}
            dynamic_metadata::scoped_declaration(module,source,b)?;
            let raw_self=source.self_pattern.as_ref().ok_or_else(||unsupported("original impl Self recipe is Unknown",None))?;
            let raw_trait=source.implemented_trait.as_ref().ok_or_else(||unsupported("original impl full trait application is Unknown",None))?;
            let symbolic=self.environment(module,context,definition.id,active,b)?;
            let pattern=self.ty(module,context,self_type,&symbolic,active,b)?;
            // The whole declaration's static bounds may already be indexed;
            // these actual copied header recipes still pay their full trees.
            dynamic_metadata::recipe(raw_self,b)?;dynamic_metadata::recipe(raw_trait,b)?;
            let claims=dynamic_witness::semantic_external::original_requirement_recipes(module,*implementation,&[DynObligation::Trait{receiver:raw_self.clone(),application:raw_trait.clone()}],b)?;
            let [SemanticRequirementRecipe::Trait{ty:raw_pattern,application:raw_application}]=claims.as_slice() else{return Err(bad("original impl header conversion has another kind"));};
            let checked_pattern=self.ty(module,context,raw_pattern,&symbolic,active,b)?;
            if checked_pattern!=pattern||raw_application.declaration!=trait_application.declaration||raw_application.arguments.len()!=trait_application.arguments.len(){return Err(bad("raw original impl header differs from its complete lexical Self/trait recipe"));}
            for(a,c)in raw_application.arguments.iter().zip(&trait_application.arguments){let a=self.actual(module,context,a,&symbolic,active,b)?;let c=self.actual(module,context,c,&symbolic,active,b)?;if a!=c{return Err(bad("raw original impl application changes a full-ordinal source actual"));}}
            let mut bindings=BTreeMap::new();
            let mut applicable = self.match_original_type(&pattern, receiver, definition.id, &mut bindings, 0, b)?;
            if applicable == Applicability::Mismatch { continue; }
            if trait_application.arguments.len()!=arguments.len(){return Err(bad("original impl trait application omits complete mixed arguments"));}
            for (pattern, actual) in trait_application.arguments.iter().zip(arguments) {
                let pattern = self.actual(module, context, pattern, &symbolic, active, b)?;
                applicable = applicable.and(self.match_original_argument(&pattern, actual, definition.id, &mut bindings, 0, b)?);
                if applicable == Applicability::Mismatch { break; }
            }
            match applicable {
                Applicability::Mismatch => continue,
                Applicability::Pending => { pending = true; continue; }
                Applicability::Match => {}
            }
            let mut environment=symbolic;
            let mut actuals=Vec::new();
            for(ordinal,formal)in definition.parameters.iter().enumerate(){
                let value=bindings.remove(&(ordinal as u32)).ok_or_else(||unsupported("original impl has an unconstrained full-ordinal actual",None))?;
                if !matches!((formal,&value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original impl matching changes a formal kind"));}
                actuals.push(value);
            }
            if !bindings.is_empty(){return Err(bad("original impl matching bound an absent formal ordinal"));}
            environment.lexical[0].actuals=actuals;
            for(formal,value)in definition.parameters.iter().zip(&environment.lexical[0].actuals){if let(SemanticFormal::Const{carrier},Arg::Const(value))=(formal,value){let carrier=self.ty(module,context,carrier,&environment,active,b)?;self.scheme_carrier(&carrier,b)?;if !self.same(&carrier,&value.carrier,b)?{return Err(bad("matched original impl Const has another complete declared carrier"));}}}
            if selected.is_some(){return Err(bad("original receiver/application selects multiple complete impl witnesses"));}
            selected=Some((definition,Provider{owner:definition.id,declaration:*implementation,environment}));
        }
        if pending {
            if defer { return Ok(None); }
            return Err(unsupported("original impl selection cannot infer a caller Type", None));
        }
        let Some((definition, selected)) = selected else { return Ok(None); };
        for requirement in &definition.requirements {
            self.obligation(module, context, requirement, &selected.environment, caller, active, b)?;
        }
        for formal in &definition.parameters {
            if let SemanticFormal::Type { requirements } = formal {
                for requirement in requirements {
                    self.obligation(module, context, requirement, &selected.environment, caller, active, b)?;
                }
            }
        }
        Ok(Some(selected))
    }

    fn bind_original_actual(&self,ordinal:u32,actual:Arg,bindings:&mut BTreeMap<u32,Arg>,b:&mut Budget)->Check<Applicability>{
        b.spend(bindings.len().saturating_add(1))?;
        if let Some(previous) = bindings.get(&ordinal) {
            if self.same_arguments_readonly(std::slice::from_ref(previous), std::slice::from_ref(&actual), b)? {
                return Ok(Applicability::Match);
            }
            if let (Arg::Type(left), Arg::Type(right)) = (previous, &actual)
                && (usage::has_inference(left, &self.substitutions, b)?
                    || usage::has_inference(right, &self.substitutions, b)?)
            {
                return Ok(Applicability::Pending);
            }
            return Ok(Applicability::Mismatch);
        }
        bindings.insert(ordinal, actual);
        Ok(Applicability::Match)
    }
    fn match_original_argument(&self, pattern: &Arg, actual: &Arg, owner: SemanticDefinitionId, bindings: &mut BTreeMap<u32, Arg>, depth: usize, b: &mut Budget) -> Check<Applicability> {
        b.spend(1)?;
        match (pattern, actual) {
            (Arg::Type(pattern), Arg::Type(actual)) => self.match_original_type(pattern, actual, owner, bindings, depth + 1, b),
            (Arg::Life(Life::Formal(definition, ordinal)), Arg::Life(actual)) if *definition == owner => {
                let actual = self.resolved_life(actual, b)?;
                if matches!(actual, Life::Infer(_)) {
                    return Err(unsupported("original impl matching has an unresolved caller lifetime", None));
                }
                self.bind_original_actual(*ordinal, Arg::Life(actual), bindings, b)
            }
            (Arg::Const(Const { value: ConstValue::Variable(definition, ordinal), .. }), Arg::Const(actual)) if *definition == owner => {
                if usage::has_inference(&actual.carrier, &self.substitutions, b)? {
                    return Err(unsupported("original impl Const carrier is unresolved", None));
                }
                self.resolved(&actual.carrier, b)?;
                self.bind_original_actual(*ordinal, Arg::Const(actual.clone()), bindings, b)
            }
            _ => self.same_arguments_readonly(std::slice::from_ref(pattern), std::slice::from_ref(actual), b).map(Into::into),
        }
    }

    fn match_original_type(&self, pattern: &Ty, actual: &Ty, owner: SemanticDefinitionId, bindings: &mut BTreeMap<u32, Arg>, depth: usize, b: &mut Budget) -> Check<Applicability> {
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("original impl pattern depth exceeded"));
        }
        let pattern = self.resolved(pattern, b)?;
        let actual = self.resolved(actual, b)?;
        if let Ty::Variable(definition, ordinal) = &pattern && *definition == owner {
            // The impl's quantified slot preserves a caller's integer
            // constraint without choosing a width. Its bounds are checked
            // again after original integer inference settles.
            if matches!(actual, Ty::Infer(_)) {
                b.spend(self.integer_variables.len().saturating_add(1).ilog2() as usize + 1)?;
                if !self.integer_variable(&actual) {
                    return Ok(Applicability::Pending);
                }
            } else if usage::has_inference(&actual, &self.substitutions, b)? {
                return Ok(Applicability::Pending);
            }
            return self.bind_original_actual(*ordinal, Arg::Type(actual), bindings, b);
        }
        match (&pattern, &actual) {
            (Ty::Nominal(a, aa), Ty::Nominal(c, ca)) | (Ty::Opaque(a, aa), Ty::Opaque(c, ca)) if a == c && aa.len() == ca.len() => {
                let mut result = Applicability::Match;
                for (a, c) in aa.iter().zip(ca) {
                    result = result.and(self.match_original_argument(a, c, owner, bindings, depth + 1, b)?);
                    if result == Applicability::Mismatch { break; }
                }
                Ok(result)
            }
            (Ty::Nominal(..) | Ty::Opaque(..), Ty::Nominal(..) | Ty::Opaque(..)) => Ok(Applicability::Mismatch),
            (Ty::Tuple(a), Ty::Tuple(c)) if a.len() == c.len() => {
                let mut result = Applicability::Match;
                for (a, c) in a.iter().zip(c) {
                    result = result.and(self.match_original_type(a, c, owner, bindings, depth + 1, b)?);
                    if result == Applicability::Mismatch { break; }
                }
                Ok(result)
            }
            (Ty::Address { mutable: a, life: al, inner: ai }, Ty::Address { mutable: c, life: cl, inner: ci }) if a == c =>
                self.match_original_argument(&Arg::Life(al.clone()), &Arg::Life(cl.clone()), owner, bindings, depth + 1, b)?
                    .then(|| self.match_original_type(ai, ci, owner, bindings, depth + 1, b)),
            (Ty::Desc { life: al, inner: ai }, Ty::Desc { life: cl, inner: ci }) =>
                self.match_original_argument(&Arg::Life(al.clone()), &Arg::Life(cl.clone()), owner, bindings, depth + 1, b)?
                    .then(|| self.match_original_type(ai, ci, owner, bindings, depth + 1, b)),
            (Ty::Managed(a), Ty::Managed(c)) | (Ty::Store(a), Ty::Store(c)) | (Ty::Slice(a), Ty::Slice(c)) =>
                self.match_original_type(a, c, owner, bindings, depth + 1, b),
            (Ty::ErrorUnion(a, r), Ty::ErrorUnion(c, s)) =>
                self.match_original_type(a, c, owner, bindings, depth + 1, b)?
                    .then(|| self.match_original_type(r, s, owner, bindings, depth + 1, b)),
            (Ty::Array(a, n), Ty::Array(c, m)) =>
                self.match_original_type(a, c, owner, bindings, depth + 1, b)?
                    .then(|| self.match_original_argument(&Arg::Const(*n.clone()), &Arg::Const(*m.clone()), owner, bindings, depth + 1, b)),
            _ => {
                if usage::has_inference(&actual, &self.substitutions, b)? {
                    return Ok(Applicability::Pending);
                }
                self.same(&pattern, &actual, b).map(Into::into)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::ModuleId;

    #[test]
    fn nominal_impl_matching_preserves_the_callers_integer_constraint() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut checker = CheckedOriginal::default();
        let definition = |index| SemanticDefinitionId { module: ModuleId(0), index };
        let host = definition(1);
        let implementation = definition(2);
        let nominal = |host, item| Ty::Nominal(host, vec![Arg::Type(item)]);
        let integer = checker.fresh_integer(&mut budget).unwrap();
        let arbitrary = checker.fresh(&mut budget).unwrap();
        let quantified = nominal(host, Ty::Variable(implementation, 0));
        let fixed = nominal(host, Ty::Atom(Type::I64));
        let integer_host = nominal(host, integer.clone());
        let arbitrary_host = nominal(host, arbitrary);
        let sibling = nominal(definition(3), integer.clone());
        for (pattern, actual, expected) in [
            (&quantified, &integer_host, Applicability::Match),
            (&fixed, &integer_host, Applicability::Pending),
            (&quantified, &arbitrary_host, Applicability::Pending),
            (&quantified, &sibling, Applicability::Mismatch),
        ] {
            let mut bindings = BTreeMap::new();
            let matched = checker
                .match_original_type(pattern, actual, implementation, &mut bindings, 0, &mut budget)
                .unwrap();
            assert_eq!(matched, expected);
            let expected_binding = (expected == Applicability::Match).then(|| Arg::Type(integer.clone()));
            assert_eq!(bindings.get(&0), expected_binding.as_ref());
        }
        assert_eq!(checker.resolved(&integer, &mut budget).unwrap(), integer);
    }
}

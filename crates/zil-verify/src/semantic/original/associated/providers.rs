//! Matching original impl schemes binds only their own formal ordinals. A
//! caller's rigid Type/Life/Const never becomes an inference variable, and a
//! selected impl's premises remain caller obligations.
use super::*;

pub(super) struct Provider { pub owner:SemanticDefinitionId, pub declaration:LifeDeclarationId, pub environment:Env }
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
            if !self.match_original_type(&pattern,receiver,definition.id,&mut bindings,0,b)?{continue;}
            let mut actuals=Vec::new();let mut complete=true;for ordinal in 0..definition.parameters.len(){if let Some(value)=bindings.remove(&(ordinal as u32)){actuals.push(value);}else{complete=false;break;}}
            if !complete{continue;}selected.lexical[0].actuals=actuals;
            let mut arguments=Vec::new();for recipe in &trait_application.arguments{arguments.push(self.actual(module,context,recipe,&selected,active,b)?);}
            if self.original_provider(module,context,receiver,trait_application.declaration,&arguments,caller,active,b)?.is_some(){applications.push((trait_application.declaration,arguments));}
        }
        Ok(applications)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn original_provider(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,declaration:LifeDeclarationId,arguments:&[Arg],caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Provider>> {
        let programme=module.semantic.as_ref().ok_or_else(||bad("original impl selection lacks its programme"))?;
        let mut selected=None;
        for definition in &programme.definitions {
            b.spend(1)?;
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl{declaration:implementation,self_type,trait_application})=&definition.body else{continue;};
            if trait_application.declaration!=declaration {continue;}
            if dynamic_witness::semantic_external::original_scope(module,*implementation,b)?.id!=definition.id{return Err(bad("original impl declaration selects another lexical owner"));}
            b.spend(module.dyn_declarations.len())?;
            let mut sources=module.dyn_declarations.iter().filter(|row|row.id==*implementation);
            let source=sources.next().ok_or_else(||bad("original impl has no complete raw declaration"))?;
            if sources.next().is_some()||source.kind!=DynDeclarationKind::Impl{return Err(bad("original impl raw identity or kind differs"));}
            dynamic_metadata::declaration(source,b)?;
            let raw_self=source.self_pattern.as_ref().ok_or_else(||unsupported("original impl Self recipe is Unknown",None))?;
            let raw_trait=source.implemented_trait.as_ref().ok_or_else(||unsupported("original impl full trait application is Unknown",None))?;
            let symbolic=self.environment(module,context,definition.id,active,b)?;
            let pattern=self.ty(module,context,self_type,&symbolic,active,b)?;
            let claims=dynamic_witness::semantic_external::original_requirement_recipes(module,*implementation,&[DynObligation::Trait{receiver:raw_self.clone(),application:raw_trait.clone()}],b)?;
            let [SemanticRequirementRecipe::Trait{ty:raw_pattern,application:raw_application}]=claims.as_slice() else{return Err(bad("original impl header conversion has another kind"));};
            let checked_pattern=self.ty(module,context,raw_pattern,&symbolic,active,b)?;
            if checked_pattern!=pattern||raw_application.declaration!=trait_application.declaration||raw_application.arguments.len()!=trait_application.arguments.len(){return Err(bad("raw original impl header differs from its complete lexical Self/trait recipe"));}
            for(a,c)in raw_application.arguments.iter().zip(&trait_application.arguments){let a=self.actual(module,context,a,&symbolic,active,b)?;let c=self.actual(module,context,c,&symbolic,active,b)?;if a!=c{return Err(bad("raw original impl application changes a full-ordinal source actual"));}}
            let mut bindings=BTreeMap::new();
            if !self.match_original_type(&pattern,receiver,definition.id,&mut bindings,0,b)?{continue;}
            if trait_application.arguments.len()!=arguments.len(){return Err(bad("original impl trait application omits complete mixed arguments"));}
            let mut matches=true;
            for(pattern,actual)in trait_application.arguments.iter().zip(arguments){
                let pattern=self.actual(module,context,pattern,&symbolic,active,b)?;
                if !self.match_original_argument(&pattern,actual,definition.id,&mut bindings,0,b)?{matches=false;break;}
            }
            if !matches{continue;}
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
            for requirement in &definition.requirements{self.obligation(module,context,requirement,&environment,caller,active,b)?;}
            for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&environment,caller,active,b)?;}}}
            if selected.is_some(){return Err(bad("original receiver/application selects multiple complete impl witnesses"));}
            selected=Some(Provider{owner:definition.id,declaration:*implementation,environment});
        }
        Ok(selected)
    }

    fn bind_original_actual(&self,ordinal:u32,actual:Arg,bindings:&mut BTreeMap<u32,Arg>,b:&mut Budget)->Check<bool>{
        b.spend(bindings.len().saturating_add(1))?;
        if let Some(previous)=bindings.get(&ordinal){return self.same_arguments_readonly(std::slice::from_ref(previous),std::slice::from_ref(&actual),b);}
        bindings.insert(ordinal,actual);Ok(true)
    }
    fn match_original_argument(&self,pattern:&Arg,actual:&Arg,owner:SemanticDefinitionId,bindings:&mut BTreeMap<u32,Arg>,depth:usize,b:&mut Budget)->Check<bool>{
        b.spend(1)?;
        match(pattern,actual){
            (Arg::Type(pattern),Arg::Type(actual))=>self.match_original_type(pattern,actual,owner,bindings,depth+1,b),
            (Arg::Life(Life::Formal(definition,ordinal)),Arg::Life(actual))if *definition==owner=>{
                let actual=self.resolved_life(actual,b)?;if matches!(actual,Life::Infer(_)){return Err(unsupported("original impl matching has an unresolved caller lifetime",None));}
                self.bind_original_actual(*ordinal,Arg::Life(actual),bindings,b)
            },
            (Arg::Const(Const{value:ConstValue::Variable(definition,ordinal),..}),Arg::Const(actual))if *definition==owner=>{
                if usage::has_inference(&actual.carrier,&self.substitutions,b)?{return Err(unsupported("original impl Const carrier is unresolved",None));}
                self.resolved(&actual.carrier,b)?;self.bind_original_actual(*ordinal,Arg::Const(actual.clone()),bindings,b)
            },
            _=>self.same_arguments_readonly(std::slice::from_ref(pattern),std::slice::from_ref(actual),b),
        }
    }
    fn match_original_type(&self,pattern:&Ty,actual:&Ty,owner:SemanticDefinitionId,bindings:&mut BTreeMap<u32,Arg>,depth:usize,b:&mut Budget)->Check<bool>{
        if depth>b.limits.max_call_depth.min(128){return Err(limit("original impl pattern depth exceeded"));}
        let pattern=self.resolved(pattern,b)?;let actual=self.resolved(actual,b)?;
        if usage::has_inference(&actual,&self.substitutions,b)?{return Err(unsupported("original impl selection cannot infer a caller Type",None));}
        if let Ty::Variable(definition,ordinal)=&pattern{if *definition==owner{return self.bind_original_actual(*ordinal,Arg::Type(actual),bindings,b);}}
        match(&pattern,&actual){
            (Ty::Nominal(a,aa),Ty::Nominal(c,ca))|(Ty::Opaque(a,aa),Ty::Opaque(c,ca))if a==c&&aa.len()==ca.len()=>{
                for(a,c)in aa.iter().zip(ca){if !self.match_original_argument(a,c,owner,bindings,depth+1,b)?{return Ok(false);}}Ok(true)
            },
            (Ty::Tuple(a),Ty::Tuple(c))if a.len()==c.len()=>{for(a,c)in a.iter().zip(c){if !self.match_original_type(a,c,owner,bindings,depth+1,b)?{return Ok(false);}}Ok(true)},
            (Ty::Address{mutable:a,life:al,inner:ai},Ty::Address{mutable:c,life:cl,inner:ci})if a==c=>Ok(self.match_original_argument(&Arg::Life(al.clone()),&Arg::Life(cl.clone()),owner,bindings,depth+1,b)?&&self.match_original_type(ai,ci,owner,bindings,depth+1,b)?),
            (Ty::Desc{life:al,inner:ai},Ty::Desc{life:cl,inner:ci})=>Ok(self.match_original_argument(&Arg::Life(al.clone()),&Arg::Life(cl.clone()),owner,bindings,depth+1,b)?&&self.match_original_type(ai,ci,owner,bindings,depth+1,b)?),
            (Ty::Managed(a),Ty::Managed(c))|(Ty::Store(a),Ty::Store(c))|(Ty::Slice(a),Ty::Slice(c))=>self.match_original_type(a,c,owner,bindings,depth+1,b),
            (Ty::ErrorUnion(a,r),Ty::ErrorUnion(c,s))=>Ok(self.match_original_type(a,c,owner,bindings,depth+1,b)?&&self.match_original_type(r,s,owner,bindings,depth+1,b)?),
            (Ty::Array(a,n),Ty::Array(c,m))=>Ok(self.match_original_type(a,c,owner,bindings,depth+1,b)?&&self.match_original_argument(&Arg::Const(*n.clone()),&Arg::Const(*m.clone()),owner,bindings,depth+1,b)?),
            _=>self.same(&pattern,&actual,b),
        }
    }
}

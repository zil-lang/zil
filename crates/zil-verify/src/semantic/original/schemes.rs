//! Universal source-scheme WF for frozen background S. Parsing a recipe or
//! finding a catalogue row is not a proof of its original formal/kind domain.
//! This pass adds no premise or runtime capability to the executable module.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic) fn frozen_schemes(&mut self,module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let program=module.semantic.as_ref().ok_or_else(||bad("frozen original schemes have no programme"))?;
        let previous=self.current;
        let inference_before=(self.substitutions.len(),self.life_substitutions.len());
        let result=(||{
            // Environments are always real declaration/parent frames. There is
            // no invented caller frame and no concrete E instance substitution.
            for definition in &program.definitions {
                self.current=Some(definition.id);
                let env=if let Some(prototype)=self.prototypes.get(&definition.id){prototype.environment.clone()}else{self.environment(module,context,definition.id,active,b)?};
                b.spend(env.lexical.iter().map(|frame|frame.actuals.len()+frame.environment.len()+1).sum())?;
                if !self.prototypes.contains_key(&definition.id){
                    let mut equations=Vec::new();for requirement in &definition.requirements{self.collect_equalities(module,context,requirement,&env,&mut equations,active,b)?;}
                    self.equalities.insert(definition.id,equations);
                }
                for (ordinal,formal) in definition.parameters.iter().enumerate(){
                    self.scheme_formal(module,context,formal,&env,active,b)?;
                    let actual=env.lexical[0].actuals.get(ordinal).ok_or_else(||bad("original scheme environment omits a full mixed formal"))?;
                    if !matches!((formal,actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original scheme environment changes a full formal kind"));}
                }
                if !definition.environment.is_empty(){
                    let parent_id=definition.parent.ok_or_else(||bad("sealed original environment has no real declaration parent"))?;
                    let parent=if let Some(prototype)=self.prototypes.get(&parent_id){prototype.environment.clone()}else{let mut parent=env.clone();parent.lexical.remove(0);parent};
                    self.current=Some(parent.current()?);
                    for formal in &definition.environment {
                        self.scheme_formal(module,context,&formal.kind,&parent,active,b)?;
                        let actual=self.actual(module,context,&formal.source,&parent,active,b)?;
                        match(&formal.kind,actual){
                            (SemanticFormal::Type{..},Arg::Type(ty))=>self.scheme_term(module,context,&ty,&parent,active,b)?,
                            (SemanticFormal::Lifetime,Arg::Life(_))=>{},
                            (SemanticFormal::Const{carrier},Arg::Const(value))=>{let expected=self.ty(module,context,carrier,&parent,active,b)?;self.scheme_carrier(&expected,b)?;if !self.same(&expected,&value.carrier,b)?{return Err(bad("original sealed Const source has a different parent-domain carrier"));}},
                            _=>return Err(bad("original sealed source changes its formal parent-domain kind")),
                        }
                    }
                    self.current=Some(definition.id);
                }
                for requirement in &definition.requirements{self.scheme_requirement(module,context,requirement,&env,active,b)?;}
                match &definition.body {
                    SemanticDefinitionBody::Nominal{body,..}=>match body {
                        SemanticNominalBody::Product{fields}=>for field in fields{let ty=self.ty(module,context,&field.ty,&env,active,b)?;self.scheme_term(module,context,&ty,&env,active,b)?;},
                        SemanticNominalBody::Enum{variants}=>for variant in variants{for field in &variant.fields{let ty=self.ty(module,context,field,&env,active,b)?;self.scheme_term(module,context,&ty,&env,active,b)?;}},
                    },
                    SemanticDefinitionBody::Scope(recipe)=>match recipe {
                        SemanticScopeRecipe::Trait{declaration}=>{if declaration.module!=definition.id.module{return Err(bad("original trait declaration belongs to another module"));}},
                        SemanticScopeRecipe::InherentImpl{declaration,self_type}=>{if declaration.module!=definition.id.module{return Err(bad("original impl declaration belongs to another module"));}if matches!(self_type,SemanticTypeRecipe::SelfType{scope:SemanticScopeRef::Ancestor(0)}){return Err(bad("original impl Self pattern recursively defines itself"));}let ty=self.ty(module,context,self_type,&env,active,b)?;self.scheme_term(module,context,&ty,&env,active,b)?;},
                        SemanticScopeRecipe::TraitImpl{declaration,self_type,trait_application}=>{if declaration.module!=definition.id.module{return Err(bad("original trait impl declaration belongs to another module"));}if matches!(self_type,SemanticTypeRecipe::SelfType{scope:SemanticScopeRef::Ancestor(0)}){return Err(bad("original impl Self pattern recursively defines itself"));}let ty=self.ty(module,context,self_type,&env,active,b)?;self.scheme_term(module,context,&ty,&env,active,b)?;self.scheme_application(module,context,trait_application,&env,active,b)?;},
                        SemanticScopeRecipe::BuiltinImpl{self_type,..}=>{if matches!(self_type,SemanticTypeRecipe::SelfType{scope:SemanticScopeRef::Ancestor(0)}){return Err(bad("original impl Self pattern recursively defines itself"));}let ty=self.ty(module,context,self_type,&env,active,b)?;self.scheme_term(module,context,&ty,&env,active,b)?;},
                    },
                    SemanticDefinitionBody::FunctionWithBody{..}=>{
                        // Reinterpret all complete P/R/quantifier annotations in
                        // their real lexical scope, including unused positions.
                        let prototype=self.prototypes.get(&definition.id).cloned().ok_or_else(||bad("frozen function scheme has no completely checked original signature"))?;
                        for ty in &prototype.parameters{self.scheme_term(module,context,ty,&prototype.environment,active,b)?;}
                        self.scheme_term(module,context,&prototype.result,&prototype.environment,active,b)?;
                    }
                    SemanticDefinitionBody::Function{..}=>return Err(bad("frozen original scheme omits its complete body")),
                }
            }
            Ok(())
        })();
        self.current=previous;
        if inference_before!=(self.substitutions.len(),self.life_substitutions.len()){return Err(bad("frozen original scheme WF attempted to infer a missing type/lifetime"));}
        result
    }
    fn scheme_formal(&mut self,module:&TypedModule,context:&CheckedContext,formal:&SemanticFormal,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        match formal {
            SemanticFormal::Lifetime=>Ok(()),
            SemanticFormal::Type{requirements}=>{for requirement in requirements{self.scheme_requirement(module,context,requirement,env,active,b)?;}Ok(())},
            SemanticFormal::Const{carrier}=>{let ty=self.ty(module,context,carrier,env,active,b)?;self.scheme_term(module,context,&ty,env,active,b)?;self.scheme_carrier(&ty,b)},
        }
    }
    fn scheme_carrier(&self,ty:&Ty,b:&mut Budget)->Check<()> {
        let ty=self.resolved(ty,b)?;
        if matches!(&ty,Ty::Atom(ty)if ty.is_integer()||ty.is_float()||matches!(ty,Type::Unit|Type::Bool)){return Ok(());}
        for scalar in [Type::Unit,Type::Bool,Type::I8,Type::I16,Type::I32,Type::I64,Type::Isize,Type::U8,Type::U16,Type::U32,Type::U64,Type::Usize,Type::F32,Type::F64]{if self.same(&ty,&Ty::Atom(scalar),b)?{return Ok(());}}
        match ty {
            Ty::Variable(..)|Ty::SelfType(_)|Ty::Infer(_)=>Err(unsupported("original Const carrier is not independently a complete scalar in its declaration context",None)),
            _=>Err(bad("original Const carrier is a non-scalar type")),
        }
    }
    fn scheme_requirement(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticRequirementRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Requirement,token},b,|active,b|{
            dynamic_metadata::recipe(recipe,b)?;
            match recipe {
                SemanticRequirementRecipe::Structural{ty,..}|SemanticRequirementRecipe::Builtin{ty,..}=>{let ty=self.ty(module,context,ty,env,active,b)?;self.scheme_term(module,context,&ty,env,active,b)?;},
                SemanticRequirementRecipe::Trait{ty,application}=>{let ty=self.ty(module,context,ty,env,active,b)?;self.scheme_term(module,context,&ty,env,active,b)?;self.scheme_application(module,context,application,env,active,b)?;},
                SemanticRequirementRecipe::Callable{ty,signature,..}=>{let ty=self.ty(module,context,ty,env,active,b)?;self.scheme_term(module,context,&ty,env,active,b)?;let signature=self.signature(module,context,signature,env,active,b)?;for ty in &signature.parameters{self.scheme_term(module,context,ty,&signature.environment,active,b)?;}self.scheme_term(module,context,&signature.result,&signature.environment,active,b)?;},
                SemanticRequirementRecipe::TypeEqual{left,right}=>{let left=self.ty(module,context,left,env,active,b)?;let right=self.ty(module,context,right,env,active,b)?;self.scheme_term(module,context,&left,env,active,b)?;self.scheme_term(module,context,&right,env,active,b)?;},
                SemanticRequirementRecipe::LifeEqual{left,right}=>{self.life(module,context,left,env,active,b)?;self.life(module,context,right,env,active,b)?;},
                SemanticRequirementRecipe::Outlives{longer,shorter}=>{self.life(module,context,longer,env,active,b)?;self.life(module,context,shorter,env,active,b)?;},
                SemanticRequirementRecipe::ConstEqual{carrier,left,right}=>{let carrier=self.ty(module,context,carrier,env,active,b)?;self.scheme_carrier(&carrier,b)?;self.constant(module,context,left,&carrier,env,active,b)?;self.constant(module,context,right,&carrier,env,active,b)?;},
                SemanticRequirementRecipe::ForAll{lifetimes,requirements}=>{
                    let mut selected=env.clone();if *lifetimes!=0{b.spend(*lifetimes as usize)?;let token=active.fresh(b)?;selected.bounds.insert(0,(0..*lifetimes).map(|index|Life::Bound(token,index)).collect());}
                    for requirement in requirements{self.scheme_requirement(module,context,requirement,&selected,active,b)?;}
                }
            }Ok(())
        })
    }
    fn scheme_application(&mut self,module:&TypedModule,context:&CheckedContext,application:&SemanticTraitRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let program=module.semantic.as_ref().ok_or_else(||bad("original trait application has no declaration programme"))?;
        b.spend(program.definitions.len())?;
        let mut rows=program.definitions.iter().filter(|row|matches!(row.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration})if declaration==application.declaration));
        let definition=rows.next().ok_or_else(||bad("original trait application has no real trait declaration"))?;
        if rows.next().is_some()||definition.kind!=SemanticDefinitionKind::Scope||application.arguments.len()!=definition.parameters.len(){return Err(bad("original trait application has an ambiguous declaration or incomplete mixed order"));}
        let mut actuals=Vec::new();
        for (formal,recipe) in definition.parameters.iter().zip(&application.arguments){
            let actual=self.actual(module,context,recipe,env,active,b)?;
            if !matches!((formal,&actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original trait argument changes its full-ordinal kind"));}
            actuals.push(actual);
        }
        let mut target=self.environment(module,context,definition.id,active,b)?;target.lexical[0].actuals=actuals;
        for(formal,actual)in definition.parameters.iter().zip(&target.lexical[0].actuals){match(formal,actual){
            (SemanticFormal::Type{requirements},Arg::Type(ty))=>{self.scheme_term(module,context,ty,env,active,b)?;for requirement in requirements{self.obligation(module,context,requirement,&target,env,active,b)?;}},
            (SemanticFormal::Lifetime,Arg::Life(_))=>{},
            (SemanticFormal::Const{carrier},Arg::Const(value))=>{let expected=self.ty(module,context,carrier,&target,active,b)?;self.scheme_carrier(&expected,b)?;if !self.same(&expected,&value.carrier,b)?{return Err(bad("original trait Const argument has another complete declared carrier"));}},
            _=>return Err(bad("original trait actual has another full formal kind")),
        }}
        for requirement in &definition.requirements{self.obligation(module,context,requirement,&target,env,active,b)?;}
        Ok(())
    }
    fn scheme_term(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            let ty=self.resolved(ty,b)?;
            match &ty {
                Ty::Nominal(id,arguments)=>{
                    let declaration=context.definition(module,*id,b)?;
                    if !matches!(declaration.body,SemanticDefinitionBody::Nominal{..})||arguments.len()!=declaration.parameters.len(){return Err(bad("original nominal scheme application loses its real declaration or mixed arity"));}
                    let mut target=self.environment(module,context,*id,active,b)?;target.lexical[0].actuals=arguments.clone();
                    for(formal,actual)in declaration.parameters.iter().zip(arguments){match(formal,actual){
                        (SemanticFormal::Type{requirements},Arg::Type(ty))=>{self.scheme_term(module,context,ty,caller,active,b)?;for requirement in requirements{self.obligation(module,context,requirement,&target,caller,active,b)?;}},
                        (SemanticFormal::Lifetime,Arg::Life(_))=>{},
                        (SemanticFormal::Const{carrier},Arg::Const(value))=>{let expected=self.ty(module,context,carrier,&target,active,b)?;self.scheme_carrier(&expected,b)?;if !self.same(&expected,&value.carrier,b)?{return Err(bad("original nominal Const argument has another declared carrier"));}},
                        _=>return Err(bad("original nominal application changes a full formal kind")),
                    }}
                    for requirement in &declaration.requirements{self.obligation(module,context,requirement,&target,caller,active,b)?;}
                }
                Ty::Tuple(fields)=>for field in fields{self.scheme_term(module,context,field,caller,active,b)?;},
                Ty::Array(field,value)=>{self.scheme_term(module,context,field,caller,active,b)?;self.scheme_carrier(&value.carrier,b)?;if !self.same(&value.carrier,&Ty::Atom(Type::Usize),b)?{return Err(bad("original array length carrier is not usize"));}},
                Ty::Address{inner,..}|Ty::Desc{inner,..}|Ty::Slice(inner)|Ty::Managed(inner)|Ty::Store(inner)=>self.scheme_term(module,context,inner,caller,active,b)?,
                Ty::ErrorUnion(error,result)=>{self.scheme_term(module,context,error,caller,active,b)?;self.scheme_term(module,context,result,caller,active,b)?;},
                Ty::Code{parameters,result,..}=>{for parameter in parameters{self.scheme_term(module,context,parameter,caller,active,b)?;}self.scheme_term(module,context,result,caller,active,b)?;},
                Ty::Carrier{child,retained,..}=>{self.scheme_term(module,context,child,caller,active,b)?;let mut types=Vec::new();retained.types(&mut types,b)?;for ty in types{self.scheme_term(module,context,ty,caller,active,b)?;}},
                Ty::Closure(..)=>return Err(unsupported("original closure scheme needs its complete creator/environment role-domain inventory",None)),
                Ty::Infer(_)=>return Err(bad("frozen original scheme retains an unresolved inferred type")),
                Ty::Variable(..)|Ty::SelfType(_)|Ty::Atom(_)|Ty::SealedError(_)|Ty::Function(_)=>{},
            }Ok(())
        })
    }
}

//! Original dynamic types retain the full trait application and erased Context.
//! No runtime table or current specialization determines this symbolic term.
use super::*;
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct DynType {pub(super) owned:bool,pub(super) application:DynApplication,pub(super) context:DynContext,pub(super) associated:Vec<DynAssociation>}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct DynApplication {pub(super) declaration:LifeDeclarationId,pub(super) arguments:Vec<Arg>}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum DynContext {Closed,Type(Box<Ty>),Shape(Box<Retained>)}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct DynAssociation {pub(super) application:DynApplication,pub(super) member:u32,pub(super) value:Arg}
impl DynType {
    pub(super) fn map(&self,mut f:impl FnMut(&Arg,&mut Budget)->Check<Arg>,b:&mut Budget)->Check<Self>{
        fn app(value:&DynApplication,f:&mut impl FnMut(&Arg,&mut Budget)->Check<Arg>,b:&mut Budget)->Check<DynApplication>{let mut arguments=Vec::new();for arg in &value.arguments{b.spend(1)?;arguments.push(f(arg,b)?);}Ok(DynApplication{declaration:value.declaration,arguments})}
        let application=app(&self.application,&mut f,b)?;
        let context=match &self.context{
            DynContext::Closed=>DynContext::Closed,
            DynContext::Type(ty)=>match f(&Arg::Type((**ty).clone()),b)?{Arg::Type(ty)=>DynContext::Type(Box::new(ty)),_=>return Err(bad("dynamic Context substitution changes Type kind"))},
            DynContext::Shape(shape)=>{
                let mut shape=shape.map(|ty,b|match f(&Arg::Type(ty.clone()),b)?{Arg::Type(ty)=>Ok(ty),_=>Err(bad("dynamic Context child changes Type kind"))},|life,b|{b.spend(1)?;Ok(life.clone())},b)?;
                shape=shape.map(|ty,b|{b.spend(1)?;Ok(ty.clone())},|life,b|match f(&Arg::Life(life.clone()),b)?{Arg::Life(life)=>Ok(life),_=>Err(bad("dynamic Context class changes Lifetime kind"))},b)?;
                DynContext::Shape(Box::new(shape))
            },
        };
        let mut associated=Vec::new();for row in &self.associated{b.spend(1)?;associated.push(DynAssociation{application:app(&row.application,&mut f,b)?,member:row.member,value:f(&row.value,b)?});}
        Ok(Self{owned:self.owned,application,context,associated})
    }
    pub(super) fn types<'a>(&'a self,output:&mut Vec<&'a Ty>,b:&mut Budget)->Check<()>{
        fn arg<'a>(value:&'a Arg,output:&mut Vec<&'a Ty>){match value{Arg::Type(ty)=>output.push(ty),Arg::Const(value)=>output.push(&value.carrier),Arg::Life(_)=>{}}}
        b.spend(self.application.arguments.len().saturating_add(self.associated.len()))?;
        for value in &self.application.arguments{arg(value,output);}
        match &self.context{DynContext::Closed=>{},DynContext::Type(ty)=>output.push(ty),DynContext::Shape(shape)=>shape.types(output,b)?,}
        for row in &self.associated{b.spend(row.application.arguments.len())?;for value in &row.application.arguments{arg(value,output);}arg(&row.value,output);}
        Ok(())
    }
}
impl CheckedOriginal {
    pub(super) fn dyn_type(&mut self,module:&TypedModule,context:&CheckedContext,owned:bool,recipe:&SemanticDynInterfaceRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<DynType>{
        let application=self.dyn_application(module,context,&recipe.application,env,active,b)?;
        let retained=match &recipe.context{SemanticDynContextRecipe::Closed=>DynContext::Closed,SemanticDynContextRecipe::Type(ty)=>DynContext::Type(Box::new(self.ty(module,context,ty,env,active,b)?)),SemanticDynContextRecipe::Shape(shape)=>DynContext::Shape(Box::new(self.retained(module,context,shape,env,active,b)?))};
        let mut associated=Vec::<DynAssociation>::new();for row in &recipe.associated{
            b.spend(associated.len().saturating_add(1))?;
            let application=self.dyn_application(module,context,&row.application,env,active,b)?;
            if associated.iter().any(|old|old.application==application&&old.member==row.member){return Err(bad("original dynamic interface repeats an associated binding"));}
            let value=match &row.value{SemanticAssociatedRecipe::Type(ty)=>Arg::Type(self.ty(module,context,ty,env,active,b)?),SemanticAssociatedRecipe::Const{carrier,value}=>{let carrier=self.ty(module,context,carrier,env,active,b)?;Arg::Const(self.constant(module,context,value,&carrier,env,active,b)?)} };
            associated.push(DynAssociation{application,member:row.member,value});
        }
        Ok(DynType{owned,application,context:retained,associated})
    }
    fn dyn_application(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticTraitRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<DynApplication>{
        let scope=dynamic_witness::semantic_external::original_scope(module,recipe.declaration,b)?;
        if !matches!(scope.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration})if declaration==recipe.declaration)||scope.parameters.len()!=recipe.arguments.len(){return Err(bad("original dynamic trait application has another identity or mixed arity"));}
        let mut arguments=Vec::new();for(formal,recipe)in scope.parameters.iter().zip(&recipe.arguments){let value=self.actual(module,context,recipe,env,active,b)?;if !matches!((formal,&value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original dynamic trait application changes a full-ordinal kind"));}arguments.push(value);}
        Ok(DynApplication{declaration:recipe.declaration,arguments})
    }
    pub(super) fn reify_dyn(&self,module:&TypedModule,value:&DynType,env:&EvalEnv,b:&mut Budget)->Check<SemanticDynInterfaceRecipe>{
        let application=self.reify_dyn_application(module,&value.application,env,b)?;
        let context=match &value.context{DynContext::Closed=>SemanticDynContextRecipe::Closed,DynContext::Type(ty)=>SemanticDynContextRecipe::Type(Box::new(self.reify_type(module,ty,env,b)?)),DynContext::Shape(shape)=>SemanticDynContextRecipe::Shape(Box::new(self.reify_retained(module,shape,env,b)?))};
        let mut associated=Vec::new();for row in &value.associated{let application=self.reify_dyn_application(module,&row.application,env,b)?;let value=match self.reify_argument(module,&row.value,env,b)?{SemanticActualRecipe::Type(ty)=>SemanticAssociatedRecipe::Type(ty),SemanticActualRecipe::Const{carrier,value}=>SemanticAssociatedRecipe::Const{carrier,value},_=>return Err(bad("original dynamic associated value changes member kind"))};associated.push(SemanticAssociatedBindingRecipe{application,member:row.member,value});}
        Ok(SemanticDynInterfaceRecipe{application,context,associated})
    }
    fn reify_dyn_application(&self,module:&TypedModule,value:&DynApplication,env:&EvalEnv,b:&mut Budget)->Check<SemanticTraitRecipe>{let mut arguments=Vec::new();for value in &value.arguments{arguments.push(self.reify_argument(module,value,env,b)?);}Ok(SemanticTraitRecipe{declaration:value.declaration,arguments})}
    pub(super) fn dyn_scheme(&mut self,module:&TypedModule,context:&CheckedContext,value:&DynType,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let applications=self.dyn_applications(module,context,value,caller,active,b)?;
        let mut types=Vec::new();value.types(&mut types,b)?;for ty in types{self.scheme_term(module,context,ty,caller,active,b)?;}
        for binding in &value.associated{
            let mut valid=false;for application in &applications{if application.declaration==binding.application.declaration&&self.same_arguments_readonly(&application.arguments,&binding.application.arguments,b)?{valid=true;}}
            if !valid{return Err(bad("dynamic associated binding selects no original interface trait"));}
            b.spend(module.dyn_declarations.len())?;
            let declaration=module.dyn_declarations.iter().find(|row|row.id==binding.application.declaration).ok_or_else(||bad("dynamic associated declaration is absent"))?;
            if !matches!((declaration.members.get(binding.member as usize),&binding.value),(Some(DynMemberDeclaration{kind:DynMemberKind::Type,..}),Arg::Type(_))|(Some(DynMemberDeclaration{kind:DynMemberKind::Const(_),..}),Arg::Const(_))){return Err(bad("dynamic associated binding changes exact member kind"));}
        }
        Ok(())
    }
    pub(super) fn dyn_applications(&mut self,module:&TypedModule,context:&CheckedContext,value:&DynType,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<DynApplication>>{
        let mut pending=vec![value.application.clone()];let mut checked=Vec::<DynApplication>::new();
        while let Some(application)=pending.pop(){
            b.spend(checked.len().saturating_add(1))?;let mut duplicate=false;for old in &checked{if old.declaration==application.declaration&&self.same_arguments_readonly(&old.arguments,&application.arguments,b)?{duplicate=true;break;}}if duplicate{continue;}
            if checked.len()>b.limits.max_nodes{return Err(limit("original dynamic supertrait closure exceeded"));}
            let scope=dynamic_witness::semantic_external::original_scope(module,application.declaration,b)?;
            if scope.parameters.len()!=application.arguments.len(){return Err(bad("dynamic supertrait application omits ordered mixed arguments"));}
            let mut selected=self.environment(module,context,scope.id,active,b)?;selected.lexical[0].actuals=application.arguments.clone();let mut erased=value.clone();erased.owned=false;selected.self_type=Some((scope.id,Ty::Dyn(Box::new(erased))));
            let mut requirements=scope.requirements.iter().collect::<Vec<_>>();
            while let Some(requirement)=requirements.pop(){b.spend(1)?;match requirement{
                SemanticRequirementRecipe::Trait{ty,application}=>{let ty=self.ty(module,context,ty,&selected,active,b)?;if !self.same(&ty,&selected.self_type.as_ref().unwrap().1,b)?{let target=self.dyn_application(module,context,application,&selected,active,b)?;self.original_trait_obligation(module,context,&ty,target.declaration,&target.arguments,caller,active,b)?;}else{pending.push(self.dyn_application(module,context,application,&selected,active,b)?);}},
                SemanticRequirementRecipe::ForAll{lifetimes:0,requirements:rows}=>requirements.extend(rows),
                _=>self.obligation(module,context,requirement,&selected,caller,active,b)?,
            }}
            for(formal,actual)in scope.parameters.iter().zip(&application.arguments){match(formal,actual){(SemanticFormal::Type{requirements},Arg::Type(ty))=>{self.scheme_term(module,context,ty,caller,active,b)?;for requirement in requirements{self.obligation(module,context,requirement,&selected,caller,active,b)?;}},(SemanticFormal::Lifetime,Arg::Life(_))=>{},(SemanticFormal::Const{carrier},Arg::Const(value))=>{let carrier=self.ty(module,context,carrier,&selected,active,b)?;if !self.same(&carrier,&value.carrier,b)?{return Err(bad("dynamic trait Const actual changes declared carrier"));}},_=>return Err(bad("dynamic trait actual changes original mixed kind"))}}
            checked.push(application);
        }
        Ok(checked)
    }
}

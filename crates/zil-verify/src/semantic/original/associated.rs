//! Symbolic original associated types keep their complete receiver/application
//! identity. Neither a monomorphized result nor a vtable supplies this meaning.
use super::*;
mod providers;
mod normalize;

pub(super) fn has_projection(ty:&Ty,b:&mut Budget)->Check<bool>{
    let mut pending=vec![ty];
    while let Some(ty)=pending.pop(){b.spend(1)?;match ty{
        Ty::Projection{..}=>return Ok(true),
        Ty::Nominal(_,args)|Ty::Opaque(_,args)=>{for value in args{match value{Arg::Type(ty)=>pending.push(ty),Arg::Const(value)=>pending.push(&value.carrier),_=>{}}}},
        Ty::Closure(_,args,sealed)=>{for value in args.iter().chain(sealed){match value{Arg::Type(ty)=>pending.push(ty),Arg::Const(value)=>pending.push(&value.carrier),_=>{}}}},
        Ty::Tuple(fields)=>pending.extend(fields),
        Ty::Address{inner,..}|Ty::Desc{inner,..}|Ty::Managed(inner)|Ty::Store(inner)|Ty::Slice(inner)=>pending.push(inner),
        Ty::Array(inner,length)=>{pending.push(inner);pending.push(&length.carrier);},
        Ty::ErrorUnion(error,result)=>{pending.push(error);pending.push(result);},
        Ty::Carrier{child,retained,..}=>{pending.push(child);retained.types(&mut pending,b)?;},
        Ty::Code{parameters,result,..}=>{pending.extend(parameters);pending.push(result);},
        _=>{},
    }}Ok(false)
}

impl CheckedOriginal{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn associated_type(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,declaration:LifeDeclarationId,arguments:&[Arg],member:u32,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let receiver=self.resolved(receiver,b)?;let arguments=self.resolve_arguments(arguments,0,b)?;
        let scope=dynamic_witness::semantic_external::original_scope(module,declaration,b)?;
        if !matches!(scope.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration:actual})if actual==declaration){return Err(bad("original associated type application does not name a trait scope"));}
        b.spend(module.dyn_declarations.len())?;let mut declarations=module.dyn_declarations.iter().filter(|row|row.id==declaration);let original=declarations.next().ok_or_else(||bad("original associated type declaration is absent"))?;
        if declarations.next().is_some()||original.kind!=DynDeclarationKind::Trait||arguments.len()!=scope.parameters.len(){return Err(bad("original associated type has an ambiguous trait or incomplete mixed application"));}
        if !matches!(original.members.get(member as usize),Some(DynMemberDeclaration{kind:DynMemberKind::Type,..})){return Err(bad("original associated type selects another original member kind/ordinal"));}
        let rows=original.associated_definitions.as_ref().ok_or_else(||unsupported("original associated member inventory is Unknown",None))?;b.spend(rows.len())?;
        let mut rows=rows.iter().filter(|row|row.member==member);let row=rows.next().ok_or_else(||bad("original associated type has no complete member definition/bounds row"))?;
        if rows.next().is_some()||row.carrier.is_some()||matches!(row.value,Some(DynAssociatedValueRecipe::Constant{..})){return Err(bad("original associated type row changes its member kind or is duplicated"));}
        for(formal,value)in scope.parameters.iter().zip(&arguments){if !matches!((formal,value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original trait application changes its ordered formal kind"));}}
        b.spend(self.projection_terms.len())?;
        if let Some((_,_,_,_,ty))=self.projection_terms.iter().rev().find(|(old,decl,args,ordinal,_)|old==&receiver&&*decl==declaration&&args==&arguments&&*ordinal==member){return self.resolved(ty,b);}
        if !matches!(receiver,Ty::Variable(..)|Ty::SelfType(_)|Ty::Projection{..}|Ty::Infer(_)) {
            if let Some(provider)=self.original_provider(module,context,&receiver,declaration,&arguments,env,active,b)?{
                let selected=module.dyn_declarations.iter().find(|row|row.id==provider.declaration).ok_or_else(||bad("selected original impl has no complete declaration"))?;
                b.spend(selected.members.len())?;
                let name=&original.members[member as usize].name;
                let mut members=selected.members.iter().enumerate().filter(|(_,row)|row.name==*name&&row.kind==DynMemberKind::Type);
                let implementation=members.next().map(|(ordinal,_)|ordinal as u32);
                if members.next().is_some(){return Err(bad("selected original impl duplicates an associated member identity"));}
                let rows=selected.associated_definitions.as_ref().ok_or_else(||unsupported("selected original impl associated inventory is Unknown",None))?;
                b.spend(rows.len())?;
                let override_row=implementation.map(|ordinal|{let mut rows=rows.iter().filter(|row|row.member==ordinal);let row=rows.next().ok_or_else(||bad("selected original associated override lacks its RHS"))?;if rows.next().is_some(){return Err(bad("selected original associated override is duplicated"));}Ok(row)}).transpose()?;
                let (rhs,owner,environment)=if let Some(override_row)=override_row {
                    let Some(DynAssociatedValueRecipe::Type(rhs))=&override_row.value else{return Err(bad("selected original associated override has another kind or missing RHS"));};
                    (rhs,provider.declaration,provider.environment)
                }else{
                    let Some(DynAssociatedValueRecipe::Type(rhs))=&row.value else{return Err(bad("original impl omits an associated definition with no trait default"));};
                    let mut environment=self.environment(module,context,scope.id,active,b)?;
                    environment.lexical[0].actuals=self.resolve_arguments(&arguments,0,b)?;
                    environment.self_type=Some((scope.id,self.resolved(&receiver,b)?));
                    (rhs,declaration,environment)
                };
                let recipe=dynamic_witness::semantic_external::original_type_recipe(module,owner,rhs,b)?;
                let result=self.ty(module,context,&recipe,&environment,active,b)?;
                self.scheme_term(module,context,&result,env,active,b)?;
                b.spend(arguments.len().saturating_add(1))?;
                let key_receiver=self.resolved(&receiver,b)?;let key_arguments=self.resolve_arguments(&arguments,0,b)?;let key_result=self.resolved(&result,b)?;
                self.projection_terms.push((key_receiver,declaration,key_arguments,member,key_result));
                let proved=(||{
                    if let Some(override_row)=override_row{for requirement in dynamic_witness::semantic_external::original_requirement_recipes(module,provider.declaration,&override_row.requirements,b)?{self.obligation(module,context,&requirement,&environment,env,active,b)?;}}
                    let mut trait_env=self.environment(module,context,scope.id,active,b)?;
                    trait_env.lexical[0].actuals=self.resolve_arguments(&arguments,0,b)?;
                    trait_env.self_type=Some((scope.id,self.resolved(&receiver,b)?));
                    for requirement in dynamic_witness::semantic_external::original_requirement_recipes(module,declaration,&row.requirements,b)?{self.obligation(module,context,&requirement,&trait_env,env,active,b)?;}
                    Ok(result)
                })();
                self.projection_terms.pop();
                return proved;
            }
        }
        if !self.trait_premise(module,context,&receiver,declaration,&arguments,env,active,b)?{
            return Err(unsupported("original associated type has neither a caller trait premise nor a unique complete impl/RHS",None));
        }
        // This is a type term with an unknown intrinsic cardinality, not an
        // empty-domain type or a runtime field/address. Concrete substitution
        // must still solve its actual associated definition independently.
        Ok(Ty::Projection{receiver:Box::new(receiver),declaration,arguments,member})
    }
    pub(super) fn trait_premise(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,declaration:LifeDeclarationId,arguments:&[Arg],env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        for(index,frame)in env.lexical.iter().enumerate(){
            let definition=context.definition(module,frame.definition,b)?;
            if matches!(receiver,Ty::SelfType(owner)if *owner==definition.id)&&matches!(definition.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration:actual})if actual==declaration)&&self.same_arguments_readonly(&frame.actuals,arguments,b)?{return Ok(true);}
            b.spend(env.lexical.len().saturating_sub(index))?;let mut lexical=env.clone();lexical.lexical=lexical.lexical[index..].to_vec();
            for requirement in &definition.requirements{if self.trait_premise_row(module,context,requirement,receiver,declaration,arguments,&lexical,active,b)?{return Ok(true);}}
            for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{if self.trait_premise_row(module,context,requirement,receiver,declaration,arguments,&lexical,active,b)?{return Ok(true);}}}}
        }Ok(false)
    }
    #[allow(clippy::too_many_arguments)]
    fn trait_premise_row(&mut self,module:&TypedModule,context:&CheckedContext,row:&SemanticRequirementRecipe,receiver:&Ty,declaration:LifeDeclarationId,arguments:&[Arg],env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        b.spend(1)?;match row{
            SemanticRequirementRecipe::Trait{ty,application}if application.declaration==declaration=>{
                let actual=self.ty(module,context,ty,env,active,b)?;if !self.same(&actual,receiver,b)?{return Ok(false);}
                let mut values=Vec::new();for argument in &application.arguments{values.push(self.actual(module,context,argument,env,active,b)?);}self.same_arguments_readonly(&values,arguments,b)
            },
            SemanticRequirementRecipe::ForAll{lifetimes:0,requirements}=>{for row in requirements{if self.trait_premise_row(module,context,row,receiver,declaration,arguments,env,active,b)?{return Ok(true);}}Ok(false)},
            _=>Ok(false),
        }
    }
    fn same_arguments_readonly(&self,left:&[Arg],right:&[Arg],b:&mut Budget)->Check<bool>{
        b.spend(left.len().saturating_add(right.len()))?;if left.len()!=right.len(){return Ok(false);}
        for(a,c)in left.iter().zip(right){let equal=match(a,c){(Arg::Type(a),Arg::Type(c))=>self.same(a,c,b)?,(Arg::Life(a),Arg::Life(c))=>self.same_life(a,c,b)?,(Arg::Const(a),Arg::Const(c))=>self.same(&a.carrier,&c.carrier,b)?&&a.value==c.value,_=>false};if !equal{return Ok(false);}}Ok(true)
    }
}

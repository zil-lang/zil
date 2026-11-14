//! Symbolic original associated types keep their complete receiver/application
//! identity. Neither a monomorphized result nor a vtable supplies this meaning.
use super::*;

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
        if !self.trait_premise(module,context,&receiver,declaration,&arguments,env,active,b)?{
            return Err(unsupported("concrete original associated type requires its uniquely matched complete impl/RHS proof",None));
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

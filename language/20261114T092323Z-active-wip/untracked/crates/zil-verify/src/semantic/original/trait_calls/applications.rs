use super::*;
type Application=(LifeDeclarationId,Vec<Arg>,u32,SemanticDefinitionId);
impl CheckedOriginal {
    pub(super) fn original_method_applications(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,name:&str,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<Application>>{
        let mut applications=Vec::<(LifeDeclarationId,Vec<Arg>)>::new();
        for(index,frame)in caller.lexical.iter().enumerate(){
            let definition=context.definition(module,frame.definition,b)?;
            let mut lexical=caller.clone();lexical.lexical=lexical.lexical[index..].to_vec();
            let mut pending=definition.requirements.iter().collect::<Vec<_>>();for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{pending.extend(requirements);}}
            while let Some(row)=pending.pop(){b.spend(1)?;match row{
                SemanticRequirementRecipe::Trait{ty,application}=>{let ty=self.ty(module,context,ty,&lexical,active,b)?;if !self.same(&ty,receiver,b)?{continue;}let mut args=Vec::new();for recipe in &application.arguments{args.push(self.actual(module,context,recipe,&lexical,active,b)?);}applications.push((application.declaration,args));},
                SemanticRequirementRecipe::ForAll{lifetimes:0,requirements}=>pending.extend(requirements),
                _=>{},
            }}
            if matches!(receiver,Ty::SelfType(owner)if *owner==definition.id){if let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration})=&definition.body{applications.push((*declaration,self.resolve_arguments(&frame.actuals,0,b)?));}}
        }
        // A source-visible concrete nominal may also select its original impl
        // scheme. An abstract I is never decomposed using a current instance.
        if !matches!(receiver,Ty::Variable(..)|Ty::SelfType(_)|Ty::Projection{..}|Ty::Infer(_)){
            for application in self.original_receiver_applications(module,context,receiver,name,caller,active,b)?{applications.push(application);}
        }
        let programme=module.semantic.as_ref().ok_or_else(||bad("original method selection programme is absent"))?;
        let mut selected=Vec::<Application>::new();
        for(declaration,args)in applications{
            b.spend(module.dyn_declarations.len().saturating_add(selected.len()))?;
            let mut originals=module.dyn_declarations.iter().filter(|row|row.id==declaration);let original=originals.next().ok_or_else(||bad("original method bound trait is absent"))?;
            if originals.next().is_some()||original.kind!=DynDeclarationKind::Trait{return Err(bad("original method bound trait identity is ambiguous"));}
            for(ordinal,member)in original.members.iter().enumerate(){if member.name==name&&matches!(member.kind,DynMemberKind::Method(_)){
                let mut scopes=programme.dyn_scopes.iter().filter(|row|row.owner==SemanticDynOwner::MethodSignature{declaration,member:ordinal as u32});let scope=scopes.next().ok_or_else(||bad("original method member has no exact declaration scope"))?;
                if scopes.next().is_some(){return Err(bad("original method declaration scope is ambiguous"));}
                let mut duplicate=false;for(old,actuals,member,_)in &selected{if *old==declaration&&*member==ordinal as u32&&self.same_arguments_readonly(actuals,&args,b)?{duplicate=true;break;}}
                if !duplicate{selected.push((declaration,self.resolve_arguments(&args,0,b)?,ordinal as u32,scope.scope));}
            }}
        }
        Ok(selected)
    }
}

//! Completed ordinary static-member execution, separate from a dynamic table
//! or contract upper. Records remain private until the whole Some programme,
//! every actual world and all original method obligations have succeeded.
use super::*;
use dynamic_witness::semantic_external::OriginalMemberReceipt;

pub(super) struct StaticMember {
    origin:StaticOrigin,
    function:FunctionId,
    specialization:DynFunctionInstance,
    source:OriginalMemberReceipt,
    worlds:Vec<SemanticInstanceId>,
}
enum StaticOrigin{
    Call{caller:SemanticInstanceId,site:SemanticSiteId},
    Definition{world:SemanticInstanceId},
}
impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_static_member(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,function:&TypedFunction,source:OriginalMemberReceipt,worlds:Vec<SemanticInstanceId>,b:&mut Budget)->Check<()>{
        let Some(specialization)=&function.dyn_instance else{return Ok(());};
        if worlds.is_empty()||site.instance!=caller.id||specialization.definition!=source.definition{return Err(bad("static member execution loses its exact caller/body/world identity"));}
        b.spend(worlds.len().saturating_add(1))?;
        for id in &worlds{
            let world=self.instance(module,*id,b)?;
            if world.definition!=source.body||!matches!(world.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function.id){return Err(bad("static member execution selects another actual body world"));}
        }
        let mut records=self.static_members.borrow_mut();
        if records.len()>=b.limits.max_nodes{return Err(limit("static member execution directory exceeds selected bound"));}
        // Both the raw specialization snapshot and its source proof are kept.
        // The already-owned provider/environment/signature proof moves here;
        // no Type/class directory is reconstructed from raw instance metadata.
        dynamic_metadata::recipe(specialization,b)?;b.spend(1)?;
        records.push(StaticMember{origin:StaticOrigin::Call{caller:caller.id,site:site.recipe},function:function.id,specialization:specialization.clone(),source,worlds});Ok(())
    }
    /// An unused emitted definition still has its own complete executable
    /// world. Prove its original member selection instead of borrowing the
    /// receipt of a called/exported copy with the same machine signature.
    pub(super) fn prepare_static_definitions(&self,module:&TypedModule,worlds:&[(SemanticInstanceId,EvalEnv)],external:&mut dynamic_witness::semantic_external::Proofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let programme=module.semantic.as_ref().ok_or_else(||bad("static definitions lack their complete original programme"))?;
        for(id,environment)in worlds{
            b.spend(1)?;
            let world=self.instance(module,*id,b)?;
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=world.target else{continue;};
            let function=module.functions.get(function.0 as usize).filter(|row|row.id==function).ok_or_else(||bad("static definition actual entry is absent"))?;
            let Some(specialization)=&function.dyn_instance else{continue;};
            if function.foreign.is_some()||function.environment.is_some(){return Err(bad("static method definition substitutes foreign or closure authority"));}
            dynamic_metadata::recipe(specialization,b)?;
            b.spend(programme.dyn_scopes.len())?;
            let mut scopes=programme.dyn_scopes.iter().filter(|row|row.owner==SemanticDynOwner::MethodDefinition(specialization.definition));
            let scope=scopes.next().ok_or_else(||bad("static definition has no exact original method scope"))?;
            if scopes.next().is_some()||scope.scope!=world.definition{return Err(bad("static definition world selects another original method"));}
            let definition=self.definition(module,world.definition,b)?;
            let current=environment.lexical.first().ok_or_else(||bad("static definition has no current lexical world"))?;
            if current.key!=ScopeKey::Instance(*id)||current.definition!=definition.id{return Err(bad("static definition world changes its exact lexical identity"));}
            let parent=environment.lexical.get(1).ok_or_else(||bad("static method definition omits its original parent"))?;
            if definition.parent!=Some(parent.definition){return Err(bad("static definition moves its original parent scope"));}
            let owner=self.definition(module,parent.definition,b)?;
            let receiver=current.self_type.as_ref().or(parent.self_type.as_ref()).ok_or_else(||bad("static definition complete Self is Unknown"))?;
            if let Some(actual)=&parent.self_type{if !self.same_type(environment,receiver,actual,b)?{return Err(bad("static method and parent disagree on complete Self"));}}
            environment_work(environment,b)?;
            let mut parent_environment=environment.clone();parent_environment.lexical.remove(0);
            let application=match &owner.body{
                SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl{self_type,trait_application,..})=>{
                    let expected=self.evaluate_type(module,self_type,&parent_environment,external,active,b)?;
                    if !self.same_type(environment,receiver,&expected,b)?{return Err(bad("static definition Self differs from its complete original impl"));}
                    self.application(module,trait_application,&parent_environment,external,active,b)?
                },
                SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration})=>{
                    for actual in &parent.actuals{actual_work(actual,b)?;}
                    CheckedApplication{declaration:*declaration,arguments:parent.actuals.clone()}
                },
                _=>return Err(bad("static method definition has another original parent role")),
            };
            b.spend(module.dyn_declarations.iter().fold(module.dyn_declarations.len(),|fee,owner|fee.saturating_add(owner.method_definitions.as_ref().map_or(0,Vec::len))))?;
            let mut definitions=module.dyn_declarations.iter().flat_map(|owner|owner.method_definitions.iter().flatten().map(move|row|(owner,row))).filter(|(_,row)|row.id==specialization.definition);
            let(body_owner,body)=definitions.next().ok_or_else(||bad("static method original definition inventory is absent"))?;
            if definitions.next().is_some(){return Err(bad("static method original definition is ambiguous"));}
            if !matches!((&owner.body,body.scope),
                (SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl{declaration,..}),DynDefinitionScope::ImplOverride)if *declaration==body_owner.id)
                && !matches!((&owner.body,body.scope),
                (SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration}),DynDefinitionScope::TraitDefault)if *declaration==body_owner.id)
            {return Err(bad("static method definition changes its original impl/default domain"));}
            let member=body_owner.members.get(body.member as usize).ok_or_else(||bad("static definition original member ordinal is absent"))?;
            let mut traits=module.dyn_declarations.iter().filter(|row|row.id==application.declaration);
            let declaration=traits.next().ok_or_else(||bad("static definition original trait is absent"))?;
            if traits.next().is_some(){return Err(bad("static definition original trait is ambiguous"));}
            b.spend(declaration.members.iter().fold(declaration.members.len(),|fee,row|fee.saturating_add(row.name.len()).saturating_add(member.name.len())))?;
            let mut members=declaration.members.iter().enumerate().filter(|(_,row)|row.name==member.name&&row.kind==member.kind);
            let(member,_)=members.next().ok_or_else(||bad("static definition has no exact original trait member"))?;
            if members.next().is_some(){return Err(bad("static definition original trait member is ambiguous"));}
            let source=dynamic_witness::semantic_external::select_definition_member(external,module,self,environment,receiver,&application,member as u32,&current.actuals,&parent_environment,active,b)?;
            if source.definition!=specialization.definition||source.body!=world.definition||source.body_owner!=body_owner.id||source.body_member!=body.member||source.source!=body.scope{return Err(bad("static definition selection changes its exact original body/domain"));}
            dynamic_witness::semantic_external::check_static_member_instance(module,self,environment,function,&source,active,b)?;
            let requests=dynamic_witness::semantic_external::static_member_requests(external,module,self,function,&source,active,b)?;
            if b.pending_dyn_contracts.methods.len().saturating_add(requests.len())>b.limits.max_nodes{return Err(limit("static definition contracts exceed selected node bound"));}
            b.spend(requests.len())?;b.pending_dyn_contracts.methods.extend(requests);
            let mut records=self.static_members.borrow_mut();
            if records.len()>=b.limits.max_nodes{return Err(limit("static definition world directory exceeds selected bound"));}
            dynamic_metadata::recipe(specialization,b)?;b.spend(2)?;
            records.push(StaticMember{origin:StaticOrigin::Definition{world:*id},function:function.id,specialization:specialization.clone(),source,worlds:vec![*id]});
        }Ok(())
    }
    pub(super) fn complete_static_members(&self,module:&TypedModule,checked:&std::collections::BTreeSet<SemanticInstanceId>,worlds:&[(SemanticInstanceId,EvalEnv)],b:&mut Budget)->Check<()>{
        for record in self.static_members.borrow().iter(){
            b.spend(record.worlds.len().saturating_add(1))?;
            if record.worlds.iter().any(|id|!checked.contains(id)){return Err(bad("static member execution has an unchecked actual body world"));}
            match record.origin{
                StaticOrigin::Call{caller,site}=>{
                    if !checked.contains(&caller){return Err(bad("static member execution has an unchecked actual caller world"));}
                    let caller=self.instance(module,caller,b)?;let definition=self.definition(module,caller.definition,b)?;
                    let SemanticDefinitionBody::FunctionWithBody{sites,..}=&definition.body else{return Err(bad("static member caller lacks its original executable body"));};
                    b.spend(sites.len())?;
                    if !sites.iter().any(|row|row.id==site&&matches!(row.kind,SemanticSiteRecipeKind::OriginalCall|SemanticSiteRecipeKind::Call{..})){return Err(bad("static member execution has no exact original call occurrence"));}
                },
                StaticOrigin::Definition{world}=>{
                    if !checked.contains(&world){return Err(bad("static definition has an unchecked actual world"));}
                    let world=self.instance(module,world,b)?;
                    if world.definition!=record.source.body||!matches!(world.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==record.function){return Err(bad("static definition receipt selects another actual body world"));}
                },
            }
            if record.source.definition!=record.specialization.definition{return Err(bad("static member execution changes its full original specialization identity"));}
            let function=module.functions.get(record.function.0 as usize).filter(|row|row.id==record.function).ok_or_else(||bad("static member result actual entry is absent"))?;
            self.complete_nominal_result(module,function,&record.source,worlds,&record.worlds,b)?;
        }Ok(())
    }
    pub(crate) fn checked_static_specialization(&self,module:&TypedModule,function:&TypedFunction,b:&mut Budget)->Check<bool>{
        let Some(instance)=&function.dyn_instance else{return Ok(false);};
        for record in self.static_members.borrow().iter().filter(|record|record.function==function.id){
            b.spend(1)?;
            // Full raw equality is paid and exact; a matching machine header,
            // provider body or outside image alone can never select a record.
            dynamic_metadata::recipe(instance,b)?;dynamic_metadata::recipe(&record.specialization,b)?;
            if *instance!=record.specialization{continue;}
            let body=self.definition(module,record.source.body,b)?;
            if !matches!(body.body,SemanticDefinitionBody::FunctionWithBody{..}){return Err(bad("completed static specialization has no original executable body"));}
            return Ok(true);
        }Ok(false)
    }
}

fn actual_work(actual:&CheckedActual,b:&mut Budget)->Check<()>{
    match actual{CheckedActual::Type(value)=>value.charge_clone(b),CheckedActual::Const{carrier,value}=>{carrier.charge_clone(b)?;dynamic_metadata::constant(value,b)},CheckedActual::Life(key)=>key_work(key,b)}
}
fn key_work(key:&ClassKey,b:&mut Budget)->Check<()>{b.spend(1)?;match key{ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=>b.spend(path.len()),_=>Ok(())}}
fn environment_work(environment:&EvalEnv,b:&mut Budget)->Check<()>{
    b.spend(environment.lexical.len().saturating_mul(2).saturating_add(environment.assumptions.len()).saturating_add(environment.images.len()).saturating_add(environment.bounds.len()).saturating_add(environment.invocations.len()))?;
    for frame in &environment.lexical{for actual in frame.actuals.iter().chain(&frame.environment){actual_work(actual,b)?;}if let Some(value)=&frame.self_type{value.charge_clone(b)?;}}
    for(a,z)in &environment.images{key_work(a,b)?;key_work(z,b)?;}
    for key in environment.bounds.iter().flat_map(|frame|&frame.classes).chain(&environment.invocations){key_work(key,b)?;}Ok(())
}

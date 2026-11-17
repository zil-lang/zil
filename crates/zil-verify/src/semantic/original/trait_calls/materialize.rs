use super::*;
pub(in crate::semantic) enum MethodActualRecipe{Recipe(SemanticActualRecipe),ReceiverRole(SemanticClassExpr),SourceType(Vec<(SemanticTypeRecipe,bool)>)}
pub(in crate::semantic) struct TraitCallRecipes{pub receiver:SemanticTypeRecipe,pub application:SemanticTraitRecipe,pub member:u32,pub actuals:Vec<MethodActualRecipe>}
impl CheckedOriginal {
    pub(in crate::semantic) fn trait_call_recipes(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,environment:&EvalEnv,b:&mut Budget)->Check<Option<TraitCallRecipes>>{
        let Some(call)=self.trait_calls.get(&(definition,site))else{return Ok(None);};
        let mut arguments=Vec::new();for actual in &call.arguments{arguments.push(self.reify_argument(module,actual,environment,b)?);}
        let mut actuals=Vec::new();for(ordinal,actual)in call.actuals.iter().enumerate(){
            let sources=self.call_type_sources(definition,site,ordinal as u32,b)?;
            if !sources.is_empty(){if !matches!(actual,Arg::Type(_)){return Err(bad("method whole Type source selects another original actual kind"));}actuals.push(MethodActualRecipe::SourceType(sources));continue;}
            let role=match actual{Arg::Life(life)=>match self.resolved_life(life,b)?{Life::ReceiverRole(definition,node,mode)=>Some(self.receiver_role(module,definition,node,mode,environment,b)?),_=>None},_=>None};
            actuals.push(match role{Some(role)=>MethodActualRecipe::ReceiverRole(role),None=>MethodActualRecipe::Recipe(self.reify_argument(module,actual,environment,b)?)});
        }
        Ok(Some(TraitCallRecipes{receiver:self.reify_type(module,&call.receiver,environment,b)?,application:SemanticTraitRecipe{declaration:call.declaration,arguments},member:call.member,actuals}))
    }
    pub(in crate::semantic) fn receiver_role(&self,module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,mode:StoredBorrowMode,env:&EvalEnv,b:&mut Budget)->Check<SemanticClassExpr>{
        let Some(frame)=env.lexical.first().filter(|row|row.definition==definition)else{return Err(bad("original receiver role escapes its actual caller"));};
        let ScopeKey::Instance(instance)=frame.key else{return Err(unsupported("receiver role requires its exact actual caller instance",None));};
        let programme=module.semantic.as_ref().ok_or_else(||bad("receiver role lacks its original programme"))?;
        b.spend(programme.sites.len().saturating_add(programme.instances.len()).saturating_add(self.trait_calls.len()))?;
        if self.callable_nodes.contains_key(&(definition,node))&&!self.trait_calls.iter().any(|((owner,_),call)|*owner==definition&&call.node==node){
            if mode!=StoredBorrowMode::SharedAddress{return Err(bad("Closure Receiver role grants no exclusive synthetic access"));}
            let recipe=self.original_call_site(definition,node,b)?.1;
            let mut sites=programme.sites.iter().filter(|row|row.instance==instance&&row.recipe==recipe&&matches!(row.action,SemanticSiteAction::BodyCall{..}));
            let site=sites.next().ok_or_else(||bad("Closure Receiver role has no actual original Call occurrence"))?;if sites.next().is_some(){return Err(bad("Closure Receiver role has ambiguous actual emission"));}
            let SemanticSiteAddress::Typed{block,instruction,..}=site.at else{return Err(bad("Closure Receiver retains Core emission"));};
            let target=programme.instances.iter().find(|row|row.id==instance&&row.definition==definition).ok_or_else(||bad("Closure Receiver caller instance is absent"))?;
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=target.target else{return Err(bad("Closure Receiver caller has no executable function"));};
            let function=module.functions.get(function.0 as usize).filter(|row|row.id==function).ok_or_else(||bad("Closure Receiver caller function is absent"))?;
            let instruction=function.blocks.get(block.0 as usize).filter(|row|row.id==block).and_then(|row|row.instructions.get(instruction as usize)).ok_or_else(||bad("Closure Receiver Call instruction is absent"))?;
            if !matches!(instruction.kind,InstructionKind::CallClosureShared{..}|InstructionKind::CallClosureMut{..}|InstructionKind::CallClosureBorrowed{kind:CallableKind::Fn|CallableKind::FnMut,..}){return Err(bad("Closure Receiver access is not its actual nonconsuming Call"));}
            let(called,_)=crate::stored_callables::called(module,function,&instruction.kind).ok_or_else(||bad("Closure Receiver Call has no exact complete callee/mode"))?;
            let entry=module.functions.get(called.function.0 as usize).filter(|row|row.id==called.function).ok_or_else(||bad("Closure Receiver callee entry is absent"))?;
            if entry.call_kind!=CallKind::Direct||called.storage_life.as_ref().is_none_or(|header|crate::signature_life::outer(header,StorageBase::Receiver).is_none()){return Err(bad("Closure Receiver Call lacks its explicit Direct access-head contract"));}
            let Life::Implicit(owner,ordinal)=self.receiver_call_life(module,definition,node,b)?else{unreachable!()};
            return Ok(SemanticClassExpr::Implicit{scope:self.scope(owner,env,b)?,ordinal});
        }
        let mut calls=self.trait_calls.iter().filter(|((owner,_),call)|*owner==definition&&call.node==node&&call.receiver_mode==Some(mode));
        let((_,recipe),_)=calls.next().ok_or_else(||bad("receiver role has no original checked method-call occurrence"))?;
        if calls.next().is_some(){return Err(bad("receiver role names multiple original method calls"));}
        let mut sites=programme.sites.iter().filter(|row|row.instance==instance&&row.recipe==*recipe&&matches!(row.action,SemanticSiteAction::BodyCall{..}));
        let site=sites.next().ok_or_else(||bad("receiver role has no exact actual method-call site"))?;if sites.next().is_some(){return Err(unsupported("receiver role needs an exact disambiguated actual emission",None));}
        let SemanticSiteAddress::Typed{block,instruction,..}=site.at else{return Err(bad("receiver role retains a Core emission"));};
        let mut instances=programme.instances.iter().filter(|row|row.id==instance&&row.definition==definition);
        let target=instances.next().ok_or_else(||bad("receiver role actual caller instance is absent"))?;if instances.next().is_some(){return Err(bad("receiver role actual caller instance is ambiguous"));}
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=target.target else{return Err(bad("receiver role caller retains a nonexecutable target"));};
        let function=module.functions.get(function.0 as usize).filter(|row|row.id==function).ok_or_else(||bad("receiver role actual function is absent"))?;
        let instruction=function.blocks.get(block.0 as usize).filter(|row|row.id==block).and_then(|row|row.instructions.get(instruction as usize)).ok_or_else(||bad("receiver role actual instruction is absent"))?;
        let value=match &instruction.kind{InstructionKind::Call{arguments,..}|InstructionKind::SuspendCall{arguments,..}=>*arguments.first().ok_or_else(||bad("receiver role actual Call omits its receiver"))?,InstructionKind::CallDynSlot{callee,..}=>*callee,_=>return Err(bad("receiver role actual site is not a checked method Call"))};
        let ty=function.values.get(value.0 as usize).ok_or_else(||bad("receiver role actual operand is absent"))?;
        if !matches!((mode,ty),(StoredBorrowMode::SharedAddress,Type::Address{mutable:false,..})|(StoredBorrowMode::ExclusiveAddress,Type::Address{mutable:true,..})|(StoredBorrowMode::ReadOnlyDescription|StoredBorrowMode::WritableDescription,Type::Desc(_))){return Err(bad("receiver role actual outer borrowing mode differs"));}
        Ok(SemanticClassExpr::OuterOccurrence{operand:SemanticOperand::Value(value),path:vec![],mode})
    }
}

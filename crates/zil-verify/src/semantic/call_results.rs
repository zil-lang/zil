//! Existing call results keep their completed callee domain. A later lexical
//! type use must not reconstruct it as a fresh nominal from outside images.
use super::*;
impl CheckedContext{
    pub(super) fn original_call_result(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,scope:ScopeKey,environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let Some(at)=environment.at else{return Ok(None);};
        let ScopeKey::Instance(instance)=scope else{return Ok(None);};
        let owner=self.instance(module,instance,b)?;
        if owner.definition!=definition||!matches!(owner.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==at.function){return Ok(None);}
        let site=self.original.borrow().as_ref().ok_or_else(||bad("original Call result has no complete symbolic body"))?.call_result_source(module,definition,source,environment,b)?;
        let Some((site,dynamic,outer))=site else{return Ok(None);};
        let program=module.semantic.as_ref().ok_or_else(||bad("original Call result has no actual site directory"))?;
        let function=module.functions.get(at.function.0 as usize).filter(|function|function.id==at.function).ok_or_else(||bad("Call result caller is absent"))?;
        b.spend(program.sites.len())?;let mut selected=None;
        for row in program.sites.iter().filter(|row|row.instance==instance&&row.recipe==site){
            let SemanticSiteAddress::Typed{block,instruction,..}=row.at else{return Err(bad("Call result retains a Core occurrence"));};
            if block==at.block{if instruction>at.instruction{continue;}}
            else if !closure_boundaries::creation_dominates(function,SitePoint{function:at.function,block,instruction,after:true},at,b)?{continue;}
            if !matches!(row.action,SemanticSiteAction::Call{..}|SemanticSiteAction::BodyCall{..}|SemanticSiteAction::CallableCall{..}){return Err(bad("Call result is linked to another actual emission role"));}
            let operation=function.blocks.get(block.0 as usize).and_then(|block|block.instructions.get(instruction as usize)).ok_or_else(||bad("Call result actual operation is absent"))?;
            if dynamic{
                if !matches!(row.action,SemanticSiteAction::BodyCall{..})||!matches!(operation.kind,InstructionKind::CallDynSlot{..}){return Err(bad("dynamic Call result changes its exact original dispatch or emission role"));}
            }else if !matches!(operation.kind,InstructionKind::Call{..}|InstructionKind::SuspendCall{..}|InstructionKind::ForeignCall{..}|InstructionKind::CallIndirect{..}|InstructionKind::CallClosure{..}|InstructionKind::CallClosureOnce{..}|InstructionKind::CallClosureBorrowed{..}|InstructionKind::CallClosureShared{..}|InstructionKind::CallClosureMut{..}){return Err(bad("Call result site names another actual opcode"));}
            let Some(value)=operation.result else{return Ok(None);};
            // This map is populated only after the actual dispatcher replayed
            // this exact original operation. A Dyn slot has already checked
            // its full interface/member, Context κ, P/R and receiver boundary;
            // it is never interpreted as an ordinary function contract here.
            let values=self.occurrences.borrow();
            let Some(value)=values.get(&(scope,OperandKey::Value(value)))else{continue;};
            value.charge_clone(b)?;
            if selected.replace(value.clone()).is_some(){return Err(unsupported("original Call result has several unresolved actual source occurrences",None));}
        }
        let Some(value)=selected else{return Ok(None);};
        if !matches!(value.ty,Type::Address{..}|Type::Desc(_)){return Ok(Some(value));}
        // A completed call preserves its whole child domain, while its root
        // borrow may be viewed at the independently inferred caller role. The
        // two occurrences are not equal, and the source SSA remains unchanged.
        let Some((mode,SemanticOuterLifeRecipe::Class(life)))=outer else{return Ok(Some(value));};
        if !matches!((&value.ty,mode),(Type::Address{mutable:true,..},StoredBorrowMode::ExclusiveAddress)|(Type::Address{mutable:false,..},StoredBorrowMode::SharedAddress)|(Type::Desc(_),StoredBorrowMode::WritableDescription)){return Err(bad("completed Call result changes its original root borrow kind or mode"));}
        let target=self.life(module,&life,environment,b)?;
        let mut heads=value.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==mode);
        let Some(head)=heads.next()else{return Ok(Some(value));};
        if heads.next().is_some(){return Err(bad("completed Call result has ambiguous actual outer borrow occurrences"));}
        let actual=value.classes.get(head.binder as usize).ok_or_else(||bad("completed Call result outer class is absent"))?;
        if self.same(environment,actual,&target,b)?{return Ok(Some(value));}
        if !self.outlives(environment,actual,&target,b)?{return Err(bad("completed Call result has no proved directed relation to its original caller borrow role"));}
        let child=self.child_value(module,&value,&[LifeTypeProjection::Pointee],b)?;
        let view=dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(target)},&[child],active,b)?;
        if !self.compatible(environment,&value,&view,b)?{return Err(bad("completed Call result view changes its complete original child domain"));}
        Ok(Some(view))
    }
}

impl CheckedContext {
    /// Reuse the actual Return receipt and its original full result in the
    /// original checked world, including all nonzero quantified frame tokens.
    /// Some(false) means exact root equality; None supplies no scoped view.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn return_borrow_view(&self,module:&TypedModule,function:&TypedFunction,instance:&SemanticInstance,block:BlockId,value:ValueId,result:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<Option<bool>>{
        if instance.target!=SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id))||!matches!(result.ty,Type::Address{..}|Type::Desc(_)){return Ok(None);}
        let actual_block=function.blocks.get(block.0 as usize).filter(|body|body.id==block).ok_or_else(||bad("checked Return view selects an absent block"))?;
        if actual_block.terminator!=Terminator::Return(Some(value)){return Err(bad("checked Return view changes its exact actual terminator operand"));}
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        if frame.key!=ScopeKey::Instance(instance.id)||frame.definition!=instance.definition{return Err(bad("checked Return view changes its exact original world"));}
        let occurrences=self.occurrences.borrow();
        let actual=occurrences.get(&(frame.key,OperandKey::Value(value))).ok_or_else(||bad("checked Return view lacks its completed actual SSA receipt"))?;
        if !self.compatible(environment,actual,result,b)?{return Err(bad("checked Return view changes the original complete result domain"));}
        let mut sources=actual.life.slots.iter().filter(|row|row.path.is_empty());
        let mut targets=result.life.slots.iter().filter(|row|row.path.is_empty());
        let(Some(source),Some(target))=(sources.next(),targets.next())else{return Ok(None);};
        if sources.next().is_some()||targets.next().is_some()||source.mode!=target.mode{return Ok(None);}
        let source=actual.classes.get(source.binder as usize).ok_or_else(||bad("checked Return view actual outer class is absent"))?;
        let target=result.classes.get(target.binder as usize).ok_or_else(||bad("checked Return view original outer class is absent"))?;
        if !self.outlives(environment,source,target,b)?{return Err(bad("checked Return view lacks its original directed outer relation"));}
        Ok(Some(!self.same(environment,source,target,b)?))
    }
}

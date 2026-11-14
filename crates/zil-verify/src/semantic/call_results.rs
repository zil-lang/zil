//! Existing call results keep their completed callee domain. A later lexical
//! type use must not reconstruct it as a fresh nominal from outside images.
use super::*;
impl CheckedContext{
    pub(super) fn original_call_result(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,scope:ScopeKey,environment:&EvalEnv,b:&mut Budget)->Check<Option<CheckedType>>{
        let Some(at)=environment.at else{return Ok(None);};
        let ScopeKey::Instance(instance)=scope else{return Ok(None);};
        let owner=self.instance(module,instance,b)?;
        if owner.definition!=definition||!matches!(owner.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==at.function){return Ok(None);}
        let site=self.original.borrow().as_ref().ok_or_else(||bad("original Call result has no complete symbolic body"))?.call_result_source(module,definition,source,b)?;
        let Some(site)=site else{return Ok(None);};
        let program=module.semantic.as_ref().ok_or_else(||bad("original Call result has no actual site directory"))?;
        b.spend(program.sites.len())?;let mut selected=None;
        for row in program.sites.iter().filter(|row|row.instance==instance&&row.recipe==site){
            let SemanticSiteAddress::Typed{block,instruction,..}=row.at else{return Err(bad("Call result retains a Core occurrence"));};
            if block!=at.block||instruction>at.instruction{continue;}
            if !matches!(row.action,SemanticSiteAction::Call{..}|SemanticSiteAction::BodyCall{..}){return Err(bad("Call result is linked to another actual emission role"));}
            let function=module.functions.get(at.function.0 as usize).ok_or_else(||bad("Call result caller is absent"))?;
            let operation=function.blocks.get(block.0 as usize).and_then(|block|block.instructions.get(instruction as usize)).ok_or_else(||bad("Call result actual operation is absent"))?;
            if !matches!(operation.kind,InstructionKind::Call{..}|InstructionKind::SuspendCall{..}|InstructionKind::ForeignCall{..}){return Err(bad("Call result site names another actual opcode"));}
            let Some(value)=operation.result else{return Ok(None);};
            // This map is populated only after the exact original Call,
            // complete world signature, actuals and operands were replayed.
            let values=self.occurrences.borrow();
            let Some(value)=values.get(&(scope,OperandKey::Value(value)))else{continue;};
            dynamic_metadata::ty(&value.ty,b)?;dynamic_metadata::life_shape(&value.life,b)?;
            b.spend(value.classes.len().saturating_add(value.origins.len()).saturating_add(value.statics.len()))?;
            for key in &value.classes{if let ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=key{b.spend(path.len())?;}}
            for origin in &value.origins{charge_intrinsic_origin(origin,b)?;}
            for row in &value.statics{match row{StaticOccurrence::Direct{path,leaf,..}=>{b.spend(path.len())?;dynamic_metadata::ty(leaf,b)?;},StaticOccurrence::Logical{position,leaf,..}=>{dynamic_metadata::position(position,b)?;dynamic_metadata::ty(leaf,b)?;}}}
            if selected.replace(value.clone()).is_some(){return Err(unsupported("original Call result has several unresolved actual source occurrences",None));}
        }
        Ok(selected)
    }
}

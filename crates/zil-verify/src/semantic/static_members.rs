//! Completed ordinary static-member execution, separate from a dynamic table
//! or contract upper. Records remain private until the whole Some programme,
//! every actual world and all original method obligations have succeeded.
use super::*;
use dynamic_witness::semantic_external::OriginalMemberReceipt;

pub(super) struct StaticMember {
    caller:SemanticInstanceId,
    site:SemanticSiteId,
    function:FunctionId,
    specialization:DynFunctionInstance,
    source:OriginalMemberReceipt,
    worlds:Vec<SemanticInstanceId>,
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
        records.push(StaticMember{caller:caller.id,site:site.recipe,function:function.id,specialization:specialization.clone(),source,worlds});Ok(())
    }
    pub(super) fn complete_static_members(&self,module:&TypedModule,checked:&std::collections::BTreeSet<SemanticInstanceId>,b:&mut Budget)->Check<()>{
        for record in self.static_members.borrow().iter(){
            b.spend(record.worlds.len().saturating_add(1))?;
            if !checked.contains(&record.caller)||record.worlds.iter().any(|id|!checked.contains(id)){return Err(bad("static member execution has an unchecked actual caller or body world"));}
            let caller=self.instance(module,record.caller,b)?;
            let definition=self.definition(module,caller.definition,b)?;
            let SemanticDefinitionBody::FunctionWithBody{sites,..}=&definition.body else{return Err(bad("static member caller lacks its original executable body"));};
            b.spend(sites.len())?;
            if !sites.iter().any(|site|site.id==record.site&&matches!(site.kind,SemanticSiteRecipeKind::OriginalCall|SemanticSiteRecipeKind::Call{..})){return Err(bad("static member execution has no exact original call occurrence"));}
            if record.source.definition!=record.specialization.definition{return Err(bad("static member execution changes its full original specialization identity"));}
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

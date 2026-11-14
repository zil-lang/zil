//! A logical constructor image, after the complete original and actual world
//! checks. This bridges equations only: it carries no address/loan/static root.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(crate) struct Key {
    pub(crate) function:FunctionId,
    pub(crate) block:BlockId,
    pub(crate) instruction:u32,
    pub(crate) result:ValueId,
}
pub(crate) struct Source {
    pub(crate) ty:Type,
    pub(crate) header:StorageLifeContract,
    pub(crate) classes:Vec<StorageBinder>,
}
impl CheckedContext {
    pub(super) fn publish_constructor_classes(&self,module:&TypedModule,worlds:&[(SemanticInstanceId,EvalEnv)],b:&mut Budget)->Check<()> {
        let programme=module.semantic.as_ref().ok_or_else(||bad("constructor class sources have no original programme"))?;
        let mut ready=BTreeMap::new();
        for function in &module.functions {
            let Some(header)=&function.storage_life else{continue;};
            b.spend(worlds.len())?;
            let mut selected=Vec::new();
            for(id,environment)in worlds {
                let instance=self.instance(module,*id,b)?;
                if instance.target==SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id)){selected.push((instance,environment));}
            }
            if selected.is_empty(){continue;}
            // Only actual input occurrences ground a constructor image. A
            // result-only binder or raw header count cannot ground itself.
            let input=|base:StorageBase|matches!(base,StorageBase::Parameter(_)|StorageBase::Capture(_)|StorageBase::Receiver);
            let mut inputs=BTreeSet::new();
            b.spend(header.bindings.len().saturating_add(header.classes.len()))?;
            for row in &header.bindings {if input(row.position.base){inputs.insert(row.binder);}}
            for row in &header.classes {if input(row.position.base){inputs.insert(row.binder);}}
            for(block_index,block)in function.blocks.iter().enumerate(){
                for(index,instruction)in block.instructions.iter().enumerate(){
                    b.spend(1)?;
                    if !matches!(instruction.kind,InstructionKind::MakeAggregate{..}|InstructionKind::MakeOwnedTuple{..}){continue;}
                    let Some(result)=instruction.result else{continue;};
                    let ty=function.values.get(result.0 as usize).ok_or_else(||bad("constructor class result is absent"))?;
                    // Physical data proof is independent of the logical N.
                    // Pointers/owners/opaque carriers never use this channel.
                    if !crate::stored_callables::inline_qualified_capture(module,ty,b)?{continue;}
                    let Type::BorrowedStorage{life,..}=ty else{continue;};
                    if life.binder_count==0{continue;}
                    let key=Key{function:function.id,block:BlockId(block_index as u32),instruction:index as u32,result};
                    let mut common:Option<Vec<BTreeSet<StorageBinder>>>=None;
                    let mut complete=true;
                    for(instance,environment)in &selected {
                        b.spend(programme.sites.len())?;
                        let count=programme.sites.iter().filter(|site|site.instance==instance.id
                            &&matches!(&site.at,SemanticSiteAddress::Typed{block,instruction,..}if *block==key.block&&*instruction==key.instruction)
                            &&matches!(&site.action,SemanticSiteAction::Construct{result:SemanticOperand::Value(value),..}if *value==result)).count();
                        if count!=1{complete=false;break;}
                        let occurrences=self.occurrences.borrow();
                        let Some(value)=occurrences.get(&(ScopeKey::Instance(instance.id),OperandKey::Value(result)))else{complete=false;break;};
                        if value.ty!=*ty||value.life!=*life||value.classes.len()!=life.binder_count as usize{ return Err(bad("checked constructor logical domain disagrees with its actual complete type")); }
                        b.spend(value.classes.len())?;
                        let mut candidates=Vec::new();
                        for class in &value.classes {
                            let mut matches=BTreeSet::new();
                            for row in &instance.header_embedding {
                                b.spend(1)?;
                                if inputs.contains(&row.header)&&self.same(environment,class,&self.class(module,&row.actual,environment,b)?,b)?{matches.insert(row.header);}
                            }
                            candidates.push(matches);
                        }
                        if let Some(common)=&mut common {
                            for(old,new)in common.iter_mut().zip(candidates){b.spend(old.len().saturating_add(new.len()))?;old.retain(|binder|new.contains(binder));}
                        }else{common=Some(candidates);}
                    }
                    if !complete{continue;}
                    let Some(common)=common else{continue;};
                    if common.iter().any(|set|set.len()!=1){continue;}
                    dynamic_metadata::ty(ty,b)?;signature_life::charge(header,b)?;b.spend(common.len())?;
                    ready.insert(key,Source{ty:ty.clone(),header:header.clone(),classes:common.into_iter().map(|set|*set.iter().next().unwrap()).collect()});
                }
            }
        }
        // Publish atomically only after every original world, actual CFG,
        // creator seal and ordinary semantic obligation has succeeded.
        b.semantic_constructor_classes=ready;
        Ok(())
    }
}

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
        let mut capture_classes=BTreeMap::new();
        for function in &module.functions {
            let Some(header)=&function.storage_life else{continue;};
            b.spend(worlds.len())?;
            let mut selected=Vec::new();
            for(id,environment)in worlds {
                let instance=self.instance(module,*id,b)?;
                if instance.target==SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id)){selected.push((instance,environment));}
            }
            if selected.is_empty(){continue;}
            // All checked creator worlds must agree before a closure-body
            // write can use two distinct Fixed occurrences of the same Life.
            // Partition by original term, never by capture type or slot index.
            if function.environment.is_some() && header.fixed_binder_count > 1 {
                let n=header.fixed_binder_count as usize;
                b.spend(n)?;
                let mut groups=vec![0u32;n];
                for (instance,environment) in &selected {
                    b.spend(n.saturating_add(instance.header_embedding.len()))?;
                    let mut images=vec![None;n];
                    for row in &instance.header_embedding {
                        let StorageBinder::Fixed(index)=row.header else {continue;};
                        if matches!(row.actual,SemanticClassExpr::Environment{..}|SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(_),..}) {
                            let slot=images.get_mut(index as usize).ok_or_else(||bad("capture image exceeds its fixed scope"))?;
                            if slot.is_some(){return Err(bad("capture occurrence has multiple fixed images"));}
                            *slot=Some(self.class(module,&row.actual,environment,b)?);
                        }
                    }
                    let mut partitions=BTreeMap::new();
                    for (index,(group,image)) in groups.iter_mut().zip(images).enumerate() {
                        b.spend(partitions.len().checked_ilog2().unwrap_or(0) as usize+2)?;
                        *group=if let Some(image)=image {
                            *partitions.entry((*group,image)).or_insert(index as u32)
                        } else {index as u32};
                    }
                }
                if groups.iter().enumerate().any(|(i,g)|i!=*g as usize) {
                    capture_classes.insert(function.id,groups);
                }
            }

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
        b.semantic_capture_classes=capture_classes;
        b.semantic_constructor_classes=ready;
        Ok(())
    }
}

/// A result's outside sources are a separate descriptor. This is never used as
/// a value, a replacement intrinsic domain, or an equality premise.
impl CheckedContext {
    pub(crate) fn nominal_result_sources(&self,environment:&EvalEnv,signature:&CheckedSignature,b:&mut Budget)->Check<Option<(CheckedType,Vec<bool>)>>{
        let result=&signature.result;
        if !matches!(result.ty,Type::BorrowedStorage{..}){return Ok(None);}
        result.charge_clone(b)?;
        let mut sources=result.clone();let mut fresh=vec![false;result.origins.len()];
        for(index,origin)in result.origins.iter().enumerate(){
            let IntrinsicOrigin::Nominal{source,class}=origin else{continue;};
            let mut input=false;
            for(parameter,_)in &signature.parameters{for known in &parameter.origins{if self.same_origin(environment,origin,known,b)?{input=true;}}}
            if input{continue;}
            if source.domain!=result.life||*class as usize!=index||source.class_sources.len()!=result.origins.len(){return Err(bad("fresh result nominal omits its complete original class source domain"));}
            let outside=source.class_sources.get(*class as usize).ok_or_else(||bad("fresh result nominal outside source is absent"))?;
            nominal_origins::charge_origin(outside,b)?;
            sources.origins[index]=outside.clone();fresh[index]=true;
        }
        if fresh.iter().any(|row|*row){Ok(Some((sources,fresh)))}else{Ok(None)}
    }

    /// Enumerate all real Return operations, including nullary/phantom arms.
    /// Only constructors independently proved by the original body may use the
    /// pending result-source correspondence. Existing values keep exact κ.
    pub(crate) fn nominal_result_constructors(&self,module:&TypedModule,function:&TypedFunction,b:&mut Budget)->Check<Option<Vec<Key>>>{
        let programme=module.semantic.as_ref().ok_or_else(||bad("nominal result has no original programme"))?;
        let mut returns=BTreeSet::new();
        for block in &function.blocks{
            b.spend(1)?;
            let Terminator::Return(Some(value))=&block.terminator else{continue;};
            let use_=SitePoint{function:function.id,block:block.id,instruction:block.instructions.len() as u32,after:false};
            let mut work=vec![(*value,use_,false)];let mut active=BTreeSet::new();let mut proven=BTreeSet::new();
            while let Some((value,use_,exit))=work.pop(){
                b.spend(1)?;
                if exit{active.remove(&value);proven.insert(value);continue;}
                let mut producer=None;
                for body in &function.blocks{b.spend(body.instructions.len())?;for(index,instruction)in body.instructions.iter().enumerate(){if instruction.result==Some(value){if producer.replace((body.id,index as u32,instruction)).is_some(){return Err(bad("nominal result repeats its actual SSA producer"));}}}}
                let Some((block,index,instruction))=producer else{return Ok(None);};
                if !closure_boundaries::creation_dominates(function,SitePoint{function:function.id,block,instruction:index,after:true},use_,b)?{return Ok(None);}
                if proven.contains(&value){continue;}
                if !active.insert(value){return Ok(None);}
                work.push((value,use_,true));
                let variant=match instruction.kind{
                    InstructionKind::MakeVariant{variant,..}=>Some(variant),
                    InstructionKind::MakeAggregate{..}|InstructionKind::MakeOwnedTuple{..}=>None,
                    InstructionKind::Load(local)|InstructionKind::TakeLocal{local}=>{
                        if function.parameters.contains(&local){return Ok(None);}
                        let mut stores=0usize;
                        // Every actual local source contributes. A convenient
                        // last store or a same-shaped value is never selected.
                        for body in &function.blocks{b.spend(body.instructions.len())?;for(at,row)in body.instructions.iter().enumerate(){if let InstructionKind::Store{local:target,value:source}=row.kind{if target==local{stores+=1;work.push((source,SitePoint{function:function.id,block:body.id,instruction:at as u32,after:false},false));}}}}
                        if stores==0{return Ok(None);}continue;
                    },
                    _=>return Ok(None),
                };
                let key=Key{function:function.id,block,instruction:index,result:value};
            let mut worlds=0usize;
            for world in &programme.instances{
                b.spend(1)?;
                if world.target!=SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id)){continue;}
                worlds+=1;
                b.spend(programme.sites.len())?;
                let mut sites=programme.sites.iter().filter(|site|site.instance==world.id
                    &&matches!(site.at,SemanticSiteAddress::Typed{block,instruction,..}if block==key.block&&instruction==key.instruction)
                    &&matches!(site.action,SemanticSiteAction::BodyCall{..}|SemanticSiteAction::Construct{..}));
                let mut matched=false;
                for site in &mut sites{
                    if let SemanticSiteAction::Construct{result,..}=&site.action{if *result!=SemanticOperand::Value(key.result){continue;}}
                    let original=self.original.borrow();let original=original.as_ref().ok_or_else(||bad("nominal result constructor lacks its complete original operation proof"))?;
                    if original.checked_result_constructor(module,world.definition,site.recipe,variant,b)?{if matched{return Err(bad("nominal result constructor has ambiguous original operations"));}matched=true;}
                }
                if !matched{return Ok(None);}
            }
            if worlds==0{return Ok(None);}
                returns.insert(key);
            }
        }
        if returns.is_empty(){Ok(None)}else{Ok(Some(returns.into_iter().collect()))}
    }

}

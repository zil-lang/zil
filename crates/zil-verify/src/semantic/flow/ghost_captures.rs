//! Whole logical environments keep every intrinsic occurrence, including
//! physical borrows and ghosts. Every physical capture must still select the
//! exact original free binder at its real creator; no class creates a root.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn environment_capture(&self,module:&TypedModule,creator:&SemanticInstance,child:&SemanticInstance,node:zil_base::NodeId,value:ValueId,actual:&CheckedType,sealed:&[CheckedActual],environment:&EvalEnv,b:&mut Budget)->Check<usize>{
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("logical capture creator is not an actual function"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("logical capture creator function is absent"))?;
        let mut selected=None;
        for(ordinal,expected)in sealed.iter().enumerate(){
            let CheckedActual::Type(expected)=expected else{continue;};
            if expected.ty!=actual.ty{continue;}
            complete_environment(module,expected,b)?;
            let source=self.original.borrow().as_ref().ok_or_else(||bad("logical capture has no checked original body"))?.ghost_capture_source(module,creator.definition,child.definition,ordinal,node,b)?;
            // Resolve every same-shaped original candidate by its real binder.
            // A different candidate cannot donate its class inventory.
            if !self.capture_creator_source(module,creator,f,value,source,b)?{continue;}
            if !self.same_type(environment,actual,expected,b)?{return Err(bad("logical capture changes the complete original type/class/origin occurrence"));}
            if selected.replace(ordinal).is_some(){return Err(bad("logical capture has several original environment sources"));}
        }
        selected.ok_or_else(||bad("logical capture does not select its exact original free-value binding"))
    }
}
pub(super) fn complete_environment(module:&TypedModule,value:&CheckedType,b:&mut Budget)->Check<()>{
    dynamic_metadata::ty(&value.ty,b)?;dynamic_metadata::life_shape(&value.life,b)?;
    let(count,_)=dynamic_witness::intrinsic_domain(module,&value.ty,b)?;
    if count!=value.life.binder_count||value.classes.len()!=count as usize||value.origins.len()!=count as usize{return Err(bad("logical environment omits a complete intrinsic class or original origin"));}Ok(())
}

impl CheckedContext{
    pub(super) fn environment_capture_header(&self,relation:&StorageLifeContract,child:&SemanticInstance,index:u32,ordinal:usize,value:&CheckedType,b:&mut Budget)->Check<()>{
        b.spend(value.classes.len().saturating_add(relation.bindings.len()).saturating_add(relation.classes.len()).saturating_add(relation.static_bindings.len()))?;
        let mut seen=vec![false;value.classes.len()];
        let mut occurrence=|binder:StorageBinder,class:u32|->Check<()>{
            let slot=seen.get_mut(class as usize).ok_or_else(||bad("captured header class escapes its complete original environment"))?;*slot=true;
            b.spend(child.header_embedding.len())?;
            let mut maps=child.header_embedding.iter().filter(|row|row.header==binder);
            let map=maps.next().ok_or_else(||bad("logical capture class has no original environment embedding"))?;
            let expected=SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(ordinal as u32),path:vec![],class};
            if !matches!(binder,StorageBinder::Fixed(_))||maps.next().is_some()||map.actual!=expected{return Err(bad("logical capture changes an exact original environment occurrence"));}Ok(())
        };
        for row in relation.bindings.iter().filter(|row|row.position.base==StorageBase::Capture(index)){
            let path=row.position.path.iter().map(|step|match step{ContractProjection::Place(step)=>Ok(*step),_=>Err(unsupported("captured logical borrow needs its complete retained-port occurrence",None))}).collect::<Check<Vec<_>>>()?;
            let mut slots=value.life.slots.iter().filter(|slot|slot.path==path);let slot=slots.next().ok_or_else(||bad("logical capture header invents a physical borrow occurrence"))?;
            if slots.next().is_some(){return Err(bad("logical capture physical borrow occurrence is ambiguous"));}
            occurrence(row.binder,slot.binder)?;
        }
        for row in relation.classes.iter().filter(|row|row.position.base==StorageBase::Capture(index)){
            let class=if row.position.path.is_empty(){row.position.class}else{
                let mut rows=value.life.retained.iter().filter(|actual|actual.path==row.position.path&&actual.class==row.position.class);
                let row=rows.next().ok_or_else(||bad("logical capture omits a full child class occurrence"))?;
                if rows.next().is_some(){return Err(bad("logical capture child occurrence is duplicated"));}row.binder
            };occurrence(row.binder,class)?;
        }
        if seen.iter().any(|seen|!*seen){return Err(bad("logical capture header omits an original intrinsic class including ghosts"));}
        for position in relation.static_bindings.iter().filter(|row|row.base==StorageBase::Capture(index)){
            let path=position.path.iter().map(|step|match step{ContractProjection::Place(step)=>Ok(LifeTypeProjection::Place(*step)),_=>Err(unsupported("static captured port requires its complete original leaf selector",None))}).collect::<Check<Vec<_>>>()?;
            if !value.statics.iter().any(|row|matches!(row,StaticOccurrence::Direct{path:actual,..}if actual==&path)){return Err(bad("logical capture Static header has no original pending source occurrence"));}
        }Ok(())
    }
}

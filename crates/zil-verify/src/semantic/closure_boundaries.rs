//! A closure has one intrinsic domain and a different outside image at each
//! actual construction. Neither a header count nor a same-typed occupant proves
//! that image. All records remain provisional until the whole entry pass ends.
use super::*;
impl CheckedContext {
    pub(super) fn original_closure_value(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,creator:ScopeKey,environment:&EvalEnv,b:&mut Budget)->Check<Option<CheckedType>> {
        let node=self.original.borrow().as_ref().ok_or_else(||bad("original closure source has no checked body"))?.closure_creation(module,definition,source,b)?;
        let Some(node)=node else{return Ok(None);};
        let at=environment.at.ok_or_else(||unsupported("original closure value requires its exact actual creation point",None))?;
        let function=module.functions.get(at.function.0 as usize).filter(|function|function.id==at.function).ok_or_else(||bad("original closure use has no exact actual function"))?;
        let rows=self.closure_boundaries.borrow();b.spend(rows.len())?;
        let mut selected=None;
        for row in rows.iter().filter(|row|row.creator==creator&&row.node==node) {
            if row.at.function!=at.function||!creation_dominates(function,row.at,at,b)?{continue;}
            if selected.replace(row.value.clone()).is_some(){return Err(unsupported("original closure value has several unresolved actual creation images",None));}
        }
        selected.map(Some).ok_or_else(||unsupported("original closure initializer has no independently sealed actual creation",None))
    }
    pub(super) fn closure_domain(&self,id:ClosureId,b:&mut Budget)->Check<Option<CheckedType>> {
        let rows=self.closure_boundaries.borrow();b.spend(rows.len())?;
        let mut selected:Option<CheckedType>=None;
        for row in rows.iter().filter(|row|row.value.ty==Type::Closure(id)) {
            if let Some(old)=&selected {
                if old.life!=row.value.life||old.origins!=row.value.origins||old.statics!=row.value.statics{return Err(bad("Closure identity has conflicting independently reconstructed intrinsic domains"));}
            }else{selected=Some(row.value.clone());}
        }
        Ok(selected)
    }
    pub(super) fn capture_local_source(&self,module:&TypedModule,creator:&SemanticInstance,child:&SemanticInstance,ordinal:u32,capture:ValueId,b:&mut Budget)->Check<()> {
        let source=self.original.borrow().as_ref().ok_or_else(||bad("actual capture has no original body proof"))?.implicit_capture_binding(module,child.definition,ordinal,b)?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("capture creator is not executable"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("capture creator function is absent"))?;
        if !self.capture_creator_source(module,creator,f,capture,source,b)?{return Err(bad("captured head differs from its exact original creator free-value binder"));}Ok(())
    }
}
impl CheckedContext {
    pub(super) fn reborrow_value(&self,parent:&CheckedType,scope:ScopeKey,result:ValueId,mutable:bool,b:&mut Budget)->Check<CheckedType> {
        let Type::Address{mutable:old_mutable,inner}=&parent.ty else{return Err(bad("actual reborrow operand is not a complete address"));};
        if mutable&&!*old_mutable{return Err(bad("actual reborrow upgrades a shared address"));}
        let mode=if mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
        let mut value=parent.clone();value.ty=Type::Address{mutable,inner:inner.clone()};
        let mut outer=value.life.slots.iter().enumerate().filter(|(_,slot)|slot.path.is_empty());
        let Some(index)=outer.next().map(|(i,_)|i)else{
            let mut statics=parent.statics.iter().filter(|row|matches!(row,StaticOccurrence::Direct{path,leaf,mode:StoredBorrowMode::SharedAddress}if path.is_empty()&&leaf==&parent.ty));
            if mutable||statics.next().is_none()||statics.next().is_some(){return Err(bad("actual reborrow has neither one bounded head nor an exact shared static source"));}
            return Ok(value);
        };
        if outer.next().is_some(){return Err(bad("actual reborrow outer scope has duplicate variable occurrences"));}
        b.spend(value.classes.len().saturating_add(value.life.slots.len()).saturating_add(value.life.retained.len()).saturating_add(value.life.nominal_arguments.len()))?;
        let mut old=value.classes.clone();let mut origins=value.origins.clone();let mut classes=Vec::new();let mut mapped_origins=Vec::new();
        let new_outer=old.len() as u32;
        let key=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
        old.push(key.clone());origins.push(IntrinsicOrigin::DerivedClass{source:key});
        let mut map=BTreeMap::<u32,u32>::new();
        let mut remap=|binder:u32|->Check<u32>{
            if let Some(index)=map.get(&binder){return Ok(*index);}
            let index=classes.len() as u32;
            classes.push(old.get(binder as usize).cloned().ok_or_else(||bad("reborrow child class is absent"))?);
            mapped_origins.push(origins.get(binder as usize).cloned().ok_or_else(||bad("reborrow child intrinsic origin is absent"))?);
            map.insert(binder,index);Ok(index)
        };
        // Canonical occurrence order is nominal arguments, physical slots,
        // then retained classes. Existing inner indices remain distinct even
        // when all their outside class images compare equal.
        for row in &mut value.life.nominal_arguments{row.binder=remap(row.binder)?;}
        for (i,row) in value.life.slots.iter_mut().enumerate(){if i==index{row.binder=remap(new_outer)?;row.mode=mode;}else{row.binder=remap(row.binder)?;}}
        for row in &mut value.life.retained{row.binder=remap(row.binder)?;}
        value.life.binder_count=classes.len() as u32;value.classes=classes;value.origins=mapped_origins;
        Ok(value)
    }
}

// Every actual path to a later use must pass this exact creator. Class or
// Type equality never selects an initializer from another branch or loop.
fn creation_dominates(f:&TypedFunction,creation:SitePoint,use_:SitePoint,b:&mut Budget)->Check<bool>{
    if creation.block==use_.block{return Ok(creation.instruction<use_.instruction||(creation.instruction==use_.instruction&&use_.after));}
    let mut pending=vec![f.entry];let mut seen=BTreeSet::new();
    while let Some(id)=pending.pop(){
        b.spend(1)?;
        if id==creation.block||!seen.insert(id){continue;}
        if id==use_.block{return Ok(false);}
        let block=f.blocks.get(id.0 as usize).filter(|block|block.id==id).ok_or_else(||bad("closure domination selects an absent actual block"))?;
        match block.terminator{
            Terminator::Jump(target)=>pending.push(target),
            Terminator::Branch{then_block,else_block,..}=>pending.extend([then_block,else_block]),
            Terminator::Return(_)|Terminator::Trap|Terminator::Unreachable=>{},
        }
    }
    Ok(true)
}

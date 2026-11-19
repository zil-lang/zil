//! A closure has one intrinsic domain and a different outside image at each
//! actual construction. Neither a header count nor a same-typed occupant proves
//! that image. All records remain provisional until the whole entry pass ends.
use super::*;
impl CheckedContext {
    /// Only the exact independently interpreted original closure cursor may
    /// supply its intrinsic Receiver indices to an image-only raw claim. Its
    /// constructor/seal and full specialization are still discharged at exit.
    pub(super) fn original_closure_claim(&self,actual:&CheckedType,expected:&CheckedType,b:&mut Budget)->Check<bool>{
        if !matches!(expected.ty,Type::Closure(_))||actual.ty!=expected.ty||actual.life!=expected.life||actual.origins.len()!=expected.origins.len(){return Ok(false);}
        let cursors=self.pending_closures.borrow();b.spend(cursors.len())?;
        for cursor in cursors.iter(){
            cursor.expected.charge_clone(b)?;
            for origin in cursor.expected.origins.iter().chain(&expected.origins){nominal_origins::charge_origin(origin,b)?;}
            if cursor.expected!=*expected{continue;}
            let mut intrinsic_scope=None;
            for(class,origin)in expected.origins.iter().enumerate(){
                b.spend(1)?;
                let IntrinsicOrigin::Existing{scope:scope@ScopeKey::Instance(_),operand:OperandKey::Receiver,path,class:actual}=origin else{return Ok(false);};
                if !path.is_empty()||*actual as usize!=class||intrinsic_scope.is_some_and(|known|known!=*scope){return Ok(false);}
                intrinsic_scope=Some(*scope);
            }
            return Ok(true);
        }
        Ok(false)
    }
    // A raw claim may name the outside image through the exact original
    // sealed source occurrence. It does not replace the closure's own ordinal.
    pub(super) fn closure_image_origin(
        &self, module: &TypedModule, expected: &CheckedType, index: usize,
        origin: &IntrinsicOrigin, environment: &EvalEnv, b: &mut Budget,
    ) -> Check<bool> {
        let cursors = self.pending_closures.borrow();
        b.spend(cursors.len())?;
        for cursor in cursors.iter() {
            if cursor.expected != *expected { continue; }
            let instance = self.instance(module, cursor.instance, b)?;
            b.spend(instance.header_embedding.len())?;
            let mut rows = instance.header_embedding.iter().filter(|row| row.header == StorageBinder::Fixed(index as u32));
            let Some(row) = rows.next() else { continue; };
            if rows.next().is_some() { return Err(bad("Closure image has duplicate actual Fixed bindings")); }
            let SemanticClassExpr::IntrinsicClass { operand: SemanticOperand::Environment(slot), path, class } = &row.actual else { continue; };
            let Some(CheckedActual::Type(source)) = cursor.sealed.get(*slot as usize) else { continue; };
            let child;
            let value = if path.is_empty() { source } else {
                child = self.child_value(module, source, path, b)?;
                &child
            };
            if let Some(exact) = value.origins.get(*class as usize)
                && self.same_origin(environment, origin, exact, b)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(super) fn original_closure_value(&self,module:&TypedModule,selection:&mut original::SourceType<'_>,creator:ScopeKey,environment:&EvalEnv,b:&mut Budget)->Check<Option<CheckedType>> {
        let node=self.original.borrow().as_ref().ok_or_else(||bad("original closure source has no checked body"))?.closure_creation(module,selection,b)?;
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
    pub(super) fn capture_local_source(&self,module:&TypedModule,creator:&SemanticInstance,child:&SemanticInstance,ordinal:u32,capture:ValueId,owner_description:bool,b:&mut Budget)->Check<()> {
        let source=self.original.borrow().as_ref().ok_or_else(||bad("actual capture has no original body proof"))?.implicit_capture_binding(module,child.definition,ordinal,b)?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("capture creator is not executable"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("capture creator function is absent"))?;
        if owner_description{
            let (_, _, producer) = self.site_index.producers.get(f, capture, b)?;
            if !matches!(producer.kind, InstructionKind::DescribeLocal{..}){
                return Err(bad("default Managed capture lacks its exact creator description operation"));
            }
        }
        if !self.capture_creator_source(module,creator,f,capture,&source,b)?{return Err(bad("captured head differs from its exact original creator free-value binder"));}Ok(())
    }
}
impl CheckedContext {
    pub(super) fn reborrow_value(&self,parent:&CheckedType,scope:ScopeKey,result:ValueId,mutable:bool,b:&mut Budget)->Check<CheckedType> {
        let Type::Address{mutable:old_mutable,inner}=&parent.ty else{return Err(bad("actual reborrow operand is not a complete address"));};
        if mutable&&!*old_mutable{return Err(bad("actual reborrow upgrades a shared address"));}
        let mode=if mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
        parent.charge_clone(b)?;
        let mut value=parent.clone();
        if mutable == *old_mutable {
            dynamic_metadata::bounds_type(inner,b)?;
        } else {
            dynamic_metadata::ty(inner,b)?;
            value.replace_type(Type::Address{mutable,inner:inner.clone()});
        }
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
        // This is a fresh wrapper: its new head is first, followed by the
        // existing whole child domain in its own ordinal order. Nominal row
        // traversal must never renumber an existing child κ.
        remap(new_outer)?;
        let mut child=std::collections::BTreeMap::new();let mut child_binders=std::collections::BTreeSet::new();
        for row in &parent.life.retained{b.spend(row.path.len().saturating_add(1))?;if row.path==[LifeTypeProjection::Pointee]{
            if child.insert(row.class,row.binder).is_some()||!child_binders.insert(row.binder){return Err(bad("reborrow child domain has duplicate or coalesced intrinsic occurrences"));}
        }}
        for(class,(ordinal,binder))in child.iter().enumerate(){if *ordinal as usize!=class{return Err(bad("reborrow child domain omits an original intrinsic ordinal"));}remap(*binder)?;}
        let old_outer=parent.life.slots[index].binder;
        let old_outer_retained=parent.life.retained.iter().any(|row|row.binder==old_outer)||parent.life.nominal_arguments.iter().any(|row|row.binder==old_outer)||parent.life.slots.iter().enumerate().any(|(i,row)|i!=index&&row.binder==old_outer);
        for binder in 0..parent.life.binder_count{if binder!=old_outer||old_outer_retained{remap(binder)?;}}
        for row in &mut value.life.nominal_arguments{row.binder=remap(row.binder)?;}
        for (i,row) in value.life.slots.iter_mut().enumerate(){if i==index{row.binder=remap(new_outer)?;row.mode=mode;}else{row.binder=remap(row.binder)?;}}
        for row in &mut value.life.retained{row.binder=remap(row.binder)?;}
        value.life.binder_count=classes.len() as u32;value.classes=classes;value.origins=mapped_origins;
        Ok(value)
    }
}

// Every actual path to a later use must pass this exact creator. Class or
// Type equality never selects an initializer from another branch or loop.
pub(super) fn creation_dominates(f:&TypedFunction,creation:SitePoint,use_:SitePoint,b:&mut Budget)->Check<bool>{
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

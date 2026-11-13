//! A closure has one intrinsic domain and a different outside image at each
//! actual construction. Neither a header count nor a same-typed occupant proves
//! that image. All records remain provisional until the whole entry pass ends.
use super::*;
impl CheckedContext {
    pub(super) fn original_closure_value(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,creator:ScopeKey,environment:&EvalEnv,b:&mut Budget)->Check<Option<CheckedType>> {
        let node=self.original.borrow().as_ref().ok_or_else(||bad("original closure source has no checked body"))?.closure_creation(module,definition,source,b)?;
        let Some(node)=node else{return Ok(None);};
        let at=environment.at.ok_or_else(||unsupported("original closure value requires its exact actual creation point",None))?;
        let rows=self.closure_boundaries.borrow();b.spend(rows.len())?;
        let mut selected=None;
        for row in rows.iter().filter(|row|row.creator==creator&&row.node==node) {
            if row.at.function!=at.function||row.at.block!=at.block||row.at.instruction>=at.instruction {continue;}
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
        let (definition,node)=self.original.borrow().as_ref().ok_or_else(||bad("actual capture has no original body proof"))?.implicit_capture_binding(module,child.definition,ordinal,b)?;
        if definition!=creator.definition{return Err(bad("actual capture belongs to another original creator scope"));}
        let original=self.definition(module,definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&original.body else{return Err(bad("actual capture parent has no original binding ledger"));};
        let program=module.semantic.as_ref().ok_or_else(||bad("actual capture program is absent"))?;
        let mut local=None;
        for source in site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Statement(node)) {
            for site in program.sites.iter().filter(|site|site.instance==creator.id&&site.recipe==source.site) {
                b.spend(1)?;
                if let SemanticSiteAction::DeclareLocal{local:SemanticLocalTarget::Typed(value),..}=&site.action {
                    if local.replace(*value).is_some(){return Err(bad("original captured binding has several actual local identities"));}
                }
            }
        }
        let local=local.ok_or_else(||bad("original captured binding lacks its actual local declaration"))?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("capture creator is not an executable scope"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("capture creator function is absent"))?;
        let mut current=capture;let mut visited=std::collections::BTreeSet::new();
        loop {
            b.spend(1)?;if !visited.insert(current){return Err(bad("actual captured head has a producer cycle"));}
            let mut producer=None;
            for block in &f.blocks {b.spend(block.instructions.len())?;for instruction in &block.instructions {if instruction.result==Some(current)&&producer.replace(instruction).is_some(){return Err(bad("actual capture has duplicate SSA definitions"));}}}
            match &producer.ok_or_else(||bad("actual captured head has no SSA definition"))?.kind {
                InstructionKind::ReborrowAddress{address,..}=>current=*address,
                InstructionKind::BorrowLocal{local:actual}|InstructionKind::BorrowLocalMut{local:actual} if *actual==local=>return Ok(()),
                _=>return Err(bad("actual captured head differs from its original free-value binding")),
            }
        }
    }
}
impl CheckedContext {
    pub(super) fn reborrow_value(&self,parent:&CheckedType,scope:ScopeKey,result:ValueId,mutable:bool,b:&mut Budget)->Check<CheckedType> {
        let Type::Address{mutable:old_mutable,inner}=&parent.ty else{return Err(bad("actual reborrow operand is not a complete address"));};
        if mutable&&!*old_mutable{return Err(bad("actual reborrow upgrades a shared address"));}
        let mode=if mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
        let mut value=parent.clone();value.ty=Type::Address{mutable,inner:inner.clone()};
        let mut outer=value.life.slots.iter().enumerate().filter(|(_,slot)|slot.path.is_empty());
        let index=outer.next().map(|(i,_)|i).ok_or_else(||unsupported("actual reborrow requires its independently bounded outer/static occurrence",None))?;
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

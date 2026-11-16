//! Static values originate in actual immutable producers. A missing lifetime
//! slot is never a source proof; pending static obligations travel separately.
use super::*;
impl CheckedContext {
    pub(super) fn static_utf8(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,b:&mut Budget)->Check<CheckedType>{
        let InstructionKind::StaticUtf8{bytes}=&instruction.kind else{return Err(bad("static literal consumer selects another actual operation"));};
        crate::check_instruction(module,f,instruction,b)?;
        let result=instruction.result.ok_or_else(||bad("static literal has no actual value"))?;
        let programme=module.semantic.as_ref().ok_or_else(||bad("static literal lacks its complete original programme"))?;
        let mut matched=false;
        for site in &programme.sites{
            b.spend(1)?;if site.instance!=instance.id{continue;}
            let SemanticSiteAddress::Typed{block,instruction:at,edge}=site.at else{return Err(bad("static source retains a Core operation"));};
            let block=f.blocks.get(block.0 as usize).ok_or_else(||bad("static literal source has no actual site block"))?;
            if at as usize>block.instructions.len()||at as usize==block.instructions.len()&&edge!=SemanticSiteEdge::Before{return Err(bad("static literal source is outside its actual instruction/terminator boundary"));}
            // A TypeUse may observe an SSA value before a terminator. Only
            // Store/Call source roles require an instruction at this address.
            let operation=block.instructions.get(at as usize);
            let mut candidates=Vec::new();
            match &site.action{
                SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(value),..}=>{candidates.push((*value,None));},
                SemanticSiteAction::DeclareLocal{local:SemanticLocalTarget::Typed(local),..}=>{
                    let operation=operation.ok_or_else(||bad("static local source selects a terminator instead of its actual Store"))?;
                    if let InstructionKind::Store{local:actual,value}=operation.kind{if actual==*local{candidates.push((value,None));}}
                },
                SemanticSiteAction::BodyCall{..}|SemanticSiteAction::Call{..}|SemanticSiteAction::CallableCall{..}=>{
                    let operation=operation.ok_or_else(||bad("static Call source selects a terminator instead of its actual Call"))?;
                    let(args,receiver_argument)=match &operation.kind{
                        InstructionKind::Call{arguments,..}|InstructionKind::ForeignCall{arguments,..}|InstructionKind::SuspendCall{arguments,..}=>(arguments,true),
                        InstructionKind::CallIndirect{arguments,..}|InstructionKind::CallDynSlot{arguments,..}|InstructionKind::CallClosure{arguments,..}|InstructionKind::CallClosureOnce{arguments,..}|InstructionKind::CallClosureBorrowed{arguments,..}|InstructionKind::CallClosureShared{arguments,..}|InstructionKind::CallClosureMut{arguments,..}=>(arguments,false),
                        _=>continue,
                    };
                    let mut selects=false;for argument in args{if static_operand_root(f,*argument,result,b)?{selects=true;break;}}if !selects{continue;}
                    let original=self.original.borrow();let original=original.as_ref().ok_or_else(||bad("static Call literal has no independently checked original body"))?;
                    if let Some((arity,literals))=original.static_call_literals(module,instance.definition,site.recipe,receiver_argument,b)?{
                        if literals.is_empty(){continue;}
                        if arity!=args.len(){return Err(bad("static literal Call changes its exact original argument roles"));}
                        for(ordinal,bytes)in literals{candidates.push((*args.get(ordinal).ok_or_else(||bad("original static argument has no actual operand"))?,Some(bytes)));}
                    }
                },
                _=>{},
            }
            for(value,spelling)in candidates{
                if !static_operand_root(f,value,result,b)?{continue;}
                let expected=if let Some(bytes)=spelling{Some(bytes)}else{let original=self.original.borrow();original.as_ref().ok_or_else(||bad("static literal has no independently checked original body"))?.static_literal(module,instance.definition,site.recipe,b)?};
                if let Some(expected)=expected{b.spend(expected.len().saturating_add(bytes.len()))?;if expected!=*bytes{return Err(bad("actual static bytes differ from their exact original literal"));}matched=true;}
            }
        }
        if !matched{return Err(bad("actual static literal has no exact original source occurrence"));}
        let ty=Type::Address{mutable:false,inner:Box::new(Type::Slice{element:Box::new(Type::U8)})};
        let mut value=self.construct_value(module,&ty,&empty_life(),vec![],b)?;
        value.statics.push(StaticOccurrence::Direct{path:vec![],leaf:ty,mode:StoredBorrowMode::SharedAddress});Ok(value)
    }
    pub(super) fn claim_static_obligations(&self,actual:&mut CheckedType,expected:&CheckedType,b:&mut Budget)->Check<()>{
        if actual.ty!=expected.ty||actual.life!=expected.life||actual.classes.len()!=expected.classes.len(){return Ok(());}
        // Resolved claim rows contain no static certificate. The independently
        // interpreted original recipe supplies only an obligation here; the
        // actual SSA/return/source kernels must still discharge it.
        for row in &expected.statics{if let StaticOccurrence::Direct{path,leaf,..}=row{
            dynamic_metadata::ty(leaf,b)?;b.spend(path.len())?;
            if lifetimes::select(&actual.ty,path,b)?!=leaf{return Err(bad("static claim changes the complete declared leaf"));}
            if let Some(physical)=path.iter().map(|step|if let LifeTypeProjection::Place(step)=step{Some(*step)}else{None}).collect::<Option<Vec<_>>>(){if actual.life.slots.iter().any(|slot|slot.path==physical){return Err(bad("static claim erases an actual variable occurrence"));}}
        }}
        if actual.statics.iter().any(|row|!expected.statics.contains(row)){return Err(bad("static claim omits a previously checked pending source occurrence"));}
        b.spend(expected.statics.len())?;actual.statics=expected.statics.clone();Ok(())
    }
}

fn static_operand_root(f:&TypedFunction,mut value:ValueId,result:ValueId,b:&mut Budget)->Check<bool>{
    let mut seen=BTreeSet::new();loop{
        if value==result{return Ok(true);}b.spend(1)?;if !seen.insert(value){return Err(bad("static literal role contains an actual SSA cycle"));}
        let mut producer=None;for block in &f.blocks{b.spend(block.instructions.len())?;for operation in &block.instructions{if operation.result==Some(value)&&producer.replace(operation).is_some(){return Err(bad("static literal role has duplicate actual SSA definitions"));}}}
        match producer.map(|row|&row.kind){Some(InstructionKind::ReborrowAddress{address,mutable:false})=>value=*address,_=>return Ok(false)}
    }
}

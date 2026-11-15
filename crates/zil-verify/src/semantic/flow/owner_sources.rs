//! Match original observation Places to actual source projections. These
//! selectors prove identities only; address/description loans, initialization,
//! field debts and retirement remain obligations of the ordinary kernels.
use super::*;
use original::intrinsics::{BufferRoot,BufferSource};
use zil_base::NodeId;

impl CheckedContext{
    pub(super) fn projected_buffer_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,mut value:ValueId,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<()>{
        if definition!=caller.definition{return Err(bad("projected buffer observation uses another original scope"));}
        let (root,mut fields)=self.original.borrow().as_ref().ok_or_else(||bad("projected buffer has no original body proof"))?.buffer_place(definition,node,b)?;
        let mut seen=std::collections::BTreeSet::new();
        loop{
            b.spend(1)?;if !seen.insert(value){return Err(bad("projected buffer has a cyclic actual producer chain"));}
            let(block,index,instruction)=producer(f,value,b)?;
            match &instruction.kind{
                InstructionKind::ReborrowAddress{address,..}|InstructionKind::AddressSlice{address,..}|InstructionKind::DescribeAddress{address}=>value=*address,
                InstructionKind::AddressField{address,index}=>{let ty=f.values.get(address.0 as usize).ok_or_else(||bad("field address operand is absent"))?;consume_field(ty,*index,&mut fields,b)?;value=*address;},
                InstructionKind::ExtractField{aggregate,index}=>{let ty=f.values.get(aggregate.0 as usize).ok_or_else(||bad("field aggregate operand is absent"))?;consume_field(ty,*index,&mut fields,b)?;value=*aggregate;},
                InstructionKind::DescribePackageField{local,index}|InstructionKind::ReadPackageField{local,index}|InstructionKind::TakePackageField{local,index}|InstructionKind::BorrowPackageField{local,index,..}=>{
                    consume_field(&f.locals.get(local.0 as usize).ok_or_else(||bad("package source local is absent"))?.ty,*index,&mut fields,b)?;
                    if !fields.is_empty(){return Err(unsupported("nested package observation requires its actual local-to-parent projection ledger",Some(instruction.span)));}
                    return self.projected_local_root(module,caller,f,*local,root,node,b);
                },
                InstructionKind::DescribeLocal{local}|InstructionKind::Load(local)|InstructionKind::TakeLocal{local}|InstructionKind::BorrowLocal{local}|InstructionKind::BorrowLocalMut{local}=>{
                    if !fields.is_empty(){return Err(bad("actual buffer descriptor omitted an original field projection"));}
                    return self.projected_local_root(module,caller,f,*local,root,node,b);
                },
                InstructionKind::ReadCapture{index:capture}|InstructionKind::TakeCapture{index:capture}|InstructionKind::DescribeCapture{index:capture}|InstructionKind::BorrowCapture{index:capture,..}=>{
                    if !fields.is_empty()||!matches!(root,BufferRoot::Capture){return Err(bad("actual capture observation changes its original root or field path"));}
                    return self.buffer_capture_use(module,caller,f,*capture,node,block,index,instruction,b);
                },
                _=>return Err(unsupported("buffer observation lacks its complete actual projection/creator chain",Some(instruction.span))),
            }
        }
    }
    fn projected_local_root(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,root:BufferRoot,node:NodeId,b:&mut Budget)->Check<()>{
        match root{
            BufferRoot::Local{node}=>if self.buffer_source_local(module,caller,f,BufferSource::Local{definition:caller.definition,node},b)?!=local{return Err(bad("field observation selects another original local"));},
            BufferRoot::Parameter{ordinal}=>if self.buffer_source_local(module,caller,f,BufferSource::Parameter{definition:caller.definition,ordinal},b)?!=local{return Err(bad("field observation selects another original parameter"));},
            BufferRoot::Capture=>{
                self.captured_local_alias(module,caller,f,local,node,b)?;
            },
        }Ok(())
    }
    fn captured_local_alias(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,node:NodeId,b:&mut Budget)->Check<()>{
        // Every real store contributes to the source upper. A convenient last
        // store, matching Type or untrusted same-numbered capture is not proof.
        let mut work=vec![(true,local.0,false)];let mut active=BTreeSet::new();let mut proven=BTreeSet::new();let mut rooted=false;
        while let Some((local,index,exit))=work.pop(){
            b.spend(1)?;
            if exit{active.remove(&(local,index));proven.insert((local,index));continue;}
            if proven.contains(&(local,index)){continue;}
            if !active.insert((local,index)){return Err(bad("captured observation local/SSA alias has a cyclic source"));}
            work.push((local,index,true));
            if local{
                let local=LocalId(index);let mut binding=None;let mut stores=Vec::new();
                for block in &f.blocks{b.spend(block.instructions.len())?;for(at,instruction)in block.instructions.iter().enumerate(){match instruction.kind{
                    InstructionKind::BindPackageCapture{local:actual,index:capture}if actual==local=>{if binding.replace((block.id,at as u32,capture,instruction)).is_some(){return Err(bad("package capture view has several actual bindings"));}},
                    InstructionKind::Store{local:actual,value}if actual==local=>stores.push(value),_=>{},
                }}}
                if let Some((block,at,capture,instruction))=binding{
                    if !stores.is_empty(){return Err(bad("captured package view mixes environment binding with value replacement"));}
                    crate::packages::check_instruction(module,f,instruction,b)?;self.buffer_capture_use(module,caller,f,capture,node,block,at,instruction,b)?;rooted=true;
                }else{
                    if stores.is_empty(){return Err(bad("captured observation local has no complete actual environment source"));}
                    b.spend(stores.len())?;work.extend(stores.into_iter().map(|value|(false,value.0,false)));
                }
            }else{
                let(block,at,instruction)=producer(f,ValueId(index),b)?;
                match instruction.kind{
                    InstructionKind::ReadCapture{index:capture}|InstructionKind::TakeCapture{index:capture}|InstructionKind::DescribeCapture{index:capture}|InstructionKind::BorrowCapture{index:capture,..}=>{self.buffer_capture_use(module,caller,f,capture,node,block,at,instruction,b)?;rooted=true;},
                    InstructionKind::Load(local)|InstructionKind::TakeLocal{local}|InstructionKind::DescribeLocal{local}|InstructionKind::BorrowLocal{local}|InstructionKind::BorrowLocalMut{local}=>work.push((true,local.0,false)),
                    InstructionKind::ReborrowAddress{address,..}|InstructionKind::DescribeAddress{address}=>work.push((false,address.0,false)),
                    _=>return Err(unsupported("captured observation alias does not select a complete actual environment root",Some(instruction.span))),
                }
            }
        }
        if !rooted{return Err(bad("captured observation alias has no actual creator root"));}Ok(())
    }
    fn buffer_capture_use(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,capture:u32,node:NodeId,_block:BlockId,_index:u32,instruction:&Instruction,b:&mut Budget)->Check<()>{
        let environment=f.environment.ok_or_else(||bad("buffer capture observation has no actual closure environment"))?;
        let definition=crate::closures::definition(module,environment,Some(instruction.span))?;
        let expected=definition.captures.get(capture as usize).ok_or_else(||bad("buffer capture index is outside the actual environment"))?;
        dynamic_metadata::ty(expected,b)?;
        let occurrences=self.occurrences.borrow();let actual=occurrences.get(&(ScopeKey::Instance(caller.id),OperandKey::Capture(capture))).ok_or_else(||bad("buffer capture has no independently sealed actual creator operand"))?;
        if actual.ty!=*expected{return Err(bad("buffer capture actual type differs from its sealed creator operand"));}
        ghost_captures::complete_environment(module,actual,b)?;
        b.spend(actual.classes.len().saturating_add(actual.origins.len()).saturating_add(actual.statics.len()))?;
        let source=self.original.borrow().as_ref().ok_or_else(||bad("buffer capture lacks independent original proof"))?.buffer_capture_source(caller.definition,node,b)?;
        let boundaries=self.closure_boundaries.borrow();let mut matched=false;
        for boundary in boundaries.iter().filter(|row|row.child==caller.id){
            b.spend(1)?;
            let ScopeKey::Instance(creator)=boundary.creator else{return Err(bad("captured buffer creator is not an actual original instance"));};
            let creator=self.instance(module,creator,b)?;
            if caller.parent!=Some(creator.id){return Err(bad("captured buffer creation changes its exact original lexical parent"));}
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("captured buffer creator is not executable"));};
            if function!=boundary.at.function{return Err(bad("captured buffer boundary changes its actual creator function"));}
            let f=module.functions.get(function.0 as usize).filter(|f|f.id==function).ok_or_else(||bad("captured buffer creator function is absent"))?;
            let instruction=f.blocks.get(boundary.at.block.0 as usize).and_then(|block|block.instructions.get(boundary.at.instruction as usize)).ok_or_else(||bad("captured buffer creator point is absent"))?;
            let InstructionKind::MakeClosure{closure,captures}=&instruction.kind else{return Err(bad("captured buffer boundary is not its actual environment constructor"));};
            if *closure!=environment||boundary.value.ty!=Type::Closure(environment){return Err(bad("captured buffer uses another actual creator environment"));}
            let operand=*captures.get(capture as usize).ok_or_else(||bad("captured buffer creator omits its physical capture operand"))?;
            if !self.capture_creator_source(module,creator,f,operand,source,b)?{return Err(bad("buffer projection substitutes another same-typed original free binder"));}
            matched=true;
        }
        if !matched{return Err(bad("buffer capture projection has no exact sealed original creator occurrence"));}Ok(())
    }
}

fn producer<'a>(f:&'a TypedFunction,value:ValueId,b:&mut Budget)->Check<(BlockId,u32,&'a Instruction)>{
    let mut result=None;
    for block in &f.blocks{b.spend(block.instructions.len())?;for(index,instruction)in block.instructions.iter().enumerate(){if instruction.result==Some(value)&&result.replace((block.id,index as u32,instruction)).is_some(){return Err(bad("buffer projection has several actual SSA producers"));}}}
    result.ok_or_else(||bad("buffer projection lacks its actual SSA definition"))
}
fn consume_field(ty:&Type,index:u32,fields:&mut Vec<String>,b:&mut Budget)->Check<()>{
    dynamic_metadata::ty(ty,b)?;let name=fields.pop().ok_or_else(||bad("actual buffer projection adds a field absent from its original Place"))?;
    let mut storage=ty;
    loop{b.spend(1)?;match storage{Type::BorrowedStorage{storage:inner,..}|Type::Managed(inner)|Type::Address{inner,..}|Type::Desc(inner)=>storage=inner,_=>break,}}
    let correct=match storage{Type::Record{fields,..}|Type::Package{fields,..}=>fields.get(index as usize).is_some_and(|(actual,_)|actual==&name),Type::Tuple(fields)=>(index as usize)<fields.len()&&index.to_string()==name,_=>false};
    if !correct{return Err(bad("actual field index differs from the original ordered nominal/tuple field"));}Ok(())
}

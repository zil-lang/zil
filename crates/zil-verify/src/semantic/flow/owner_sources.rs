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
                InstructionKind::ReborrowAddress{address,..}|InstructionKind::DescribeAddress{address}=>value=*address,
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
                let mut binding=None;
                for block in &f.blocks{b.spend(block.instructions.len())?;for(index,instruction)in block.instructions.iter().enumerate(){if let InstructionKind::BindPackageCapture{local:actual,index:capture}=instruction.kind{if actual==local&&binding.replace((block.id,index as u32,capture,instruction)).is_some(){return Err(bad("package capture view has several actual bindings"));}}}}
                let(block,index,capture,instruction)=binding.ok_or_else(||bad("captured package observation has no actual environment view"))?;
                crate::packages::check_instruction(module,f,instruction,b)?;
                self.buffer_capture_use(module,caller,f,capture,node,block,index,instruction,b)?;
            },
        }Ok(())
    }
    fn buffer_capture_use(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,capture:u32,node:NodeId,block:BlockId,index:u32,instruction:&Instruction,b:&mut Budget)->Check<()>{
        let environment=f.environment.ok_or_else(||bad("buffer capture observation has no actual closure environment"))?;
        let definition=crate::closures::definition(module,environment,Some(instruction.span))?;
        let expected=definition.captures.get(capture as usize).ok_or_else(||bad("buffer capture index is outside the actual environment"))?;
        dynamic_metadata::ty(expected,b)?;
        let occurrences=self.occurrences.borrow();let actual=occurrences.get(&(ScopeKey::Instance(caller.id),OperandKey::Capture(capture))).ok_or_else(||bad("buffer capture has no independently sealed actual creator operand"))?;
        if actual.ty!=*expected{return Err(bad("buffer capture actual type differs from its sealed creator operand"));}
        if !actual.classes.is_empty()||!actual.statics.is_empty(){return Err(unsupported("qualified buffer capture needs its complete retained-port source transport",Some(instruction.span)));}
        let semantic=module.semantic.as_ref().ok_or_else(||bad("buffer capture has no exact original site ledger"))?;
        let original=self.definition(module,caller.definition,b)?;let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&original.body else{return Err(bad("buffer capture lacks its original body"));};
        let mut matched=false;
        for site in &semantic.sites{
            b.spend(site_nodes.len().saturating_add(1))?;
            if site.instance!=caller.id||!matches!(site.at,SemanticSiteAddress::Typed{block:actual,instruction:at,..}if actual==block&&at==index){continue;}
            let SemanticSiteAction::TypeUse{subject,..}=&site.action else{continue;};
            let selects=match (&instruction.kind,subject){(InstructionKind::BindPackageCapture{local,..},SemanticOperand::Local(actual))=>local==actual,(_,SemanticOperand::Value(actual))=>instruction.result==Some(*actual),_=>false};if !selects{return Err(bad("buffer capture site selects another actual use"));}
            let source=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("buffer capture use lacks its original node link"))?;
            let SemanticOriginalNode::Expression(read)=source.node else{return Err(bad("buffer capture use points outside its original value expression"));};
            if !self.original.borrow().as_ref().ok_or_else(||bad("buffer capture lacks independent original proof"))?.buffer_capture_matches(caller.definition,node,read,b)?{return Err(bad("buffer projection uses another same-typed original capture"));}
            matched=true;
        }
        if !matched{return Err(bad("buffer capture projection has no exact original free-binder observation"));}Ok(())
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

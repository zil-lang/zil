//! Original assignment correspondence, never a target, loan or access proof.
use super::*;
use std::collections::BTreeSet;

impl CheckedContext {
    pub(in crate::semantic::flow) fn copy_field_update(
        &self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,b:&mut Budget,
    )->Check<()> {
        let result=instruction.result.ok_or_else(||bad("Copy field update has no SSA result"))?;
        let (block,at,_)=super::owner_sources::producer(f,result,b)?;
        let block=f.blocks.get(block.0 as usize).ok_or_else(||bad("Copy field update has no actual block"))?;
        // Nested updates rebuild from the innermost child to the original
        // local, without evaluating or publishing any intermediate parent.
        let mut previous=result;
        let mut following=block.instructions.iter().skip(at as usize+1);
        let (root,local)=loop {
            b.spend(1)?;
            match following.next() {
                Some(Instruction{kind:InstructionKind::UpdateField{value,..},result:Some(output),span}) if *value==previous&&*span==instruction.span=>previous=*output,
                Some(Instruction{kind:InstructionKind::Store{local,value},span,..}) if *value==previous&&*span==instruction.span=>break(previous,*local),
                _=>return Err(bad("Copy field update has no exact original local publication")),
            }
        };
        let original=self.field_assignment(module,caller,instruction.span,b)?;
        let mut selected=root;
        let mut previous_child=None;
        let mut root_value=None;
        let mut path=Vec::new();
        loop {
            b.spend(1)?;
            if path.len()>=b.limits.max_call_depth {return Err(limit("Copy field update exceeds original path depth"));}
            let (_,_,actual)=super::owner_sources::producer(f,selected,b)?;
            let InstructionKind::UpdateField{aggregate,index,value}=actual.kind else{return Err(bad("Copy field update chain omits its constructor"));};
            if let Some((parent,field))=previous_child {
                let (_,_,projection)=super::owner_sources::producer(f,aggregate,b)?;
                if projection.kind!=(InstructionKind::ExtractField{aggregate:parent,index:field}) {
                    return Err(bad("nested field update changes its actual parent snapshot"));
                }
            }
            root_value.get_or_insert(aggregate);
            path.push(PlaceProjection::Field(index));
            let (_,_,child)=super::owner_sources::producer(f,value,b)?;
            if matches!(child.kind,InstructionKind::UpdateField{..})&&child.span==instruction.span {
                previous_child=Some((aggregate,index));
                selected=value;
            }else{
                self.field_assignment_rhs(module,caller,f,value,&original,b)?;
                break;
            }
        }
        let root_value=root_value.ok_or_else(||bad("Copy field update omits its original parent"))?;
        let (_,_,read)=super::owner_sources::producer(f,root_value,b)?;
        if read.kind!=InstructionKind::Load(local)||!f.locals.get(local.0 as usize).is_some_and(|row|row.mutable){
            return Err(bad("Copy field update publication differs from its initialized mutable source local"));
        }
        let ty=f.values.get(root_value.0 as usize).ok_or_else(||bad("Copy field update has no actual parent type"))?;
        self.original_projection_source_prefix(module,caller,f,root_value,caller.definition,original.target,true,Some((ty,&path)),b)
    }

    pub(in crate::semantic::flow) fn description_field_write(
        &self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,
        desc:ValueId,index:u32,value:ValueId,b:&mut Budget,
    )->Check<()> {
        let Some(Type::Desc(payload))=f.values.get(desc.0 as usize) else{return Err(bad("description field write lacks a real Desc source"));};
        let original=self.field_assignment(module,caller,instruction.span,b)?;
        self.original_projection_source_prefix(module,caller,f,desc,caller.definition,original.target,true,Some((payload,&[PlaceProjection::Field(index)])),b)?;
        self.field_assignment_rhs(module,caller,f,value,&original,b)
    }

    fn field_assignment(&self,module:&TypedModule,caller:&SemanticInstance,span:Span,b:&mut Budget)->Check<original::Assignment> {
        let originals=self.original.borrow().as_ref().ok_or_else(||bad("field write has no independently checked original body"))?.assignments(module,caller.definition,b)?;
        b.spend(originals.len())?;
        let mut matches=originals.into_iter().filter(|original|original.span==span||original.statement_span==span);
        let selected=matches.next().ok_or_else(||bad("field write has no corresponding original assignment"))?;
        if matches.next().is_some(){return Err(bad("field write has several possible original assignments"));}
        Ok(selected)
    }

    fn field_assignment_rhs(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,original:&original::Assignment,b:&mut Budget)->Check<()> {
        if let Some(op)=original.compound {
            let (_,_,actual)=super::owner_sources::producer(f,value,b)?;
            let InstructionKind::Binary{op:actual_op,left,right}=actual.kind else{return Err(bad("compound field assignment omits its original operator"));};
            if actual_op!=op{return Err(bad("compound field assignment changes its original operator"));}
            self.original_projection_source(module,caller,f,left,caller.definition,original.target,true,b)?;
            self.assignment_value_source(module,caller,f,right,original.value,b)
        }else{
            self.assignment_value_source(module,caller,f,value,original.value,b)
        }
    }

    pub(crate) fn assignment_writes(
        &self,module:&TypedModule,f:&TypedFunction,b:&mut Budget,
    )->Check<Vec<ValueId>> {
        let program=module.semantic.as_ref().ok_or_else(||bad("assignment original programme is absent"))?;
        let mut complete=None::<Vec<ValueId>>;
        b.spend(program.instances.len())?;
        for caller in &program.instances {
            if !matches!(caller.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==f.id){continue;}
            let originals=self.original.borrow().as_ref().ok_or_else(||bad("assignment has no independently checked original body"))?.assignments(module,caller.definition,b)?;
            let mut used=BTreeSet::new();
            let mut writes=Vec::new();
            for block in &f.blocks {for instruction in &block.instructions {
                b.spend(1)?;
                let InstructionKind::WriteAddress{address,value}=instruction.kind else{continue;};
                let carrier=f.values.get(value.0 as usize).ok_or_else(||bad("assignment actual value is absent"))?;
                if !carrier.is_integer()&&!carrier.is_float()&&!matches!(carrier,Type::Unit|Type::Bool){continue;}
                // Spans only find candidates. The exact original binder,
                // ordered path, index/endpoints and RHS operations below are
                // the correspondence proof; a span supplies no authority.
                b.spend(originals.len())?;
                let mut candidates=originals.iter().filter(|original|original.span==instruction.span);
                let Some(original)=candidates.next() else{continue;};
                if candidates.next().is_some(){continue;}
                crate::check_instruction(module,f,instruction,b)?;
                self.original_projection_source(module,caller,f,address,caller.definition,original.target,false,b)?;
                if let Some(op)=original.compound {
                    let(_,_,actual)=super::owner_sources::producer(f,value,b)?;
                    let InstructionKind::Binary{op:actual_op,left,right}=actual.kind else{return Err(bad("compound assignment omits its complete original operator"));};
                    if actual_op!=op{return Err(bad("compound assignment changes its original operator"));}
                    self.original_projection_source(module,caller,f,left,caller.definition,original.target,true,b)?;
                    self.assignment_value_source(module,caller,f,right,original.value,b)?;
                } else {
                    self.assignment_value_source(module,caller,f,value,original.value,b)?;
                }
                b.spend(used.len().saturating_add(writes.len()).saturating_add(1))?;
                if !used.insert(original.statement){return Err(bad("one original assignment occurrence supplies several actual writes"));}
                writes.push(address);
            }}
            if let Some(previous)=&complete {
                b.spend(previous.len().saturating_add(writes.len()))?;
                if previous!=&writes{return Err(bad("assignment actual writes differ between complete original worlds"));}
            } else { complete=Some(writes); }
        }
        complete.ok_or_else(||bad("assignment executable function has no complete original instance"))
    }
}

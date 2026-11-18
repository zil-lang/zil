//! Original assignment correspondence, never a target, loan or access proof.
use super::*;
use std::collections::BTreeSet;

impl CheckedContext {
    pub(in crate::semantic::flow) fn copy_field_update(
        &self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,b:&mut Budget,
    )->Check<()> {
        let spread=self.original.borrow().as_ref().ok_or_else(||bad("Copy field update has no complete original body"))?
            .record_spread(module,caller.definition,instruction.span,b)?;
        if let Some((base,fields))=spread {
            return self.copy_record_spread(module,caller,f,instruction,base,&fields,b);
        }
        let result=instruction.result.ok_or_else(||bad("Copy field update has no SSA result"))?;
        let (block,at,_)=self.site_index.producers.get(f,result,b)?;
        let block=f.blocks.get(block.0 as usize).ok_or_else(||bad("Copy field update has no actual block"))?;
        // Nested updates rebuild from the innermost child to the original
        // local, without evaluating or publishing any intermediate parent.
        let mut previous=result;
        let mut following=block.instructions.iter().skip(at as usize+1);
        let (root,local)=loop {
            b.spend(1)?;
            match following.next() {
                Some(Instruction{kind:InstructionKind::UpdateField{value,..}|InstructionKind::UpdateIndex{value,..},result:Some(output),span}) if *value==previous&&*span==instruction.span=>previous=*output,
                Some(Instruction{kind:InstructionKind::Store{local,value},span,..}) if *value==previous&&*span==instruction.span=>break(previous,*local),
                _=>return Err(bad("Copy field update has no exact original local publication")),
            }
        };
        let original=self.field_assignment(module,caller,instruction.span,b)?;
        let mut selected = root;
        let mut expected_read = InstructionKind::Load(local);
        if !f.locals.get(local.0 as usize).is_some_and(|row| row.mutable) {
            return Err(bad("Copy update does not publish to its original mutable local"));
        }
        for _ in 0..b.limits.max_call_depth {
            b.spend(1)?;
            let (_, _, update) = self.site_index.producers.get(f, selected, b)?;
            let (parent, child, projected_read) = match update.kind {
                InstructionKind::UpdateField { aggregate, index, value } =>
                    (aggregate, value, InstructionKind::ExtractField { aggregate, index }),
                InstructionKind::UpdateIndex { aggregate, index, value } =>
                    (aggregate, value, InstructionKind::ExtractIndex { aggregate, index }),
                _ => return Err(bad("Copy update chain omits its rebuilding operation")),
            };
            let (_, _, read) = self.site_index.producers.get(f, parent, b)?;
            if read.kind != expected_read {
                return Err(bad("nested Copy update changes its actual parent snapshot"));
            }
            let (_, _, rhs) = self.site_index.producers.get(f, child, b)?;
            if matches!(rhs.kind, InstructionKind::UpdateField { .. } | InstructionKind::UpdateIndex { .. })
                && rhs.span == instruction.span {
                expected_read = projected_read;
                selected = child;
                continue;
            }
            self.field_assignment_rhs(module, caller, f, child, &original, b)?;
            return self.copy_assignment_target(module, caller, f, parent, &projected_read, original.target, b);
        }
        Err(limit("Copy update exceeds original path depth"))
    }

    fn copy_record_spread(
        &self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,
        base:zil_base::NodeId,fields:&[(String,zil_base::NodeId)],b:&mut Budget,
    )->Check<()> {
        let result=instruction.result.ok_or_else(||bad("record spread field has no SSA result"))?;
        let (block,at,_)=self.site_index.producers.get(f,result,b)?;
        let body=f.blocks.get(block.0 as usize).ok_or_else(||bad("record spread has no actual block"))?;
        let mut final_value=result;
        for next in body.instructions.iter().skip(at as usize+1) {
            b.spend(1)?;
            if matches!(next.kind,InstructionKind::UpdateField{aggregate,..} if aggregate==final_value)
                && next.span==instruction.span {
                final_value=next.result.ok_or_else(||bad("record spread field has no actual result"))?;
            }
        }
        let mut actual_count=0usize;
        for other in &f.blocks {
            b.spend(other.instructions.len())?;
            actual_count=actual_count.saturating_add(other.instructions.iter().filter(|op|
                op.span==instruction.span&&matches!(op.kind,InstructionKind::UpdateField{..})).count());
        }
        if actual_count!=fields.len(){return Err(bad("record spread changes the number of explicit field publications"));}
        let mut selected=final_value;
        let mut seen=BTreeSet::new();
        for _ in fields {
            b.spend(fields.len().saturating_add(seen.len()).saturating_add(1))?;
            let (_,_,update)=self.site_index.producers.get(f,selected,b)?;
            let InstructionKind::UpdateField{aggregate,index,value}=update.kind else{return Err(bad("record spread omits an original explicit field"));};
            if update.span!=instruction.span||!seen.insert(index){return Err(bad("record spread changes or repeats an original field"));}
            let Some(Type::Record{fields:layout,..})=f.values.get(aggregate.0 as usize) else{return Err(bad("record spread has no exact Copy Record base"));};
            let (name,_)=layout.get(index as usize).ok_or_else(||bad("record spread publishes an absent field"))?;
            let (_,node)=fields.iter().find(|(field,_)|field==name).ok_or_else(||bad("record spread changes the original explicit field identity"))?;
            let (at_block,at,operand)=self.site_index.producers.get(f,value,b)?;
            let value=if let InstructionKind::Load(local)=operand.kind {
                super::owner_sources::scalar_preparations::copy_value_source(module,f,at_block,at,local,b)?.unwrap_or(value)
            }else{value};
            self.assignment_value_source(module,caller,f,value,*node,b)?;
            selected=aggregate;
        }
        let (at_block,at,operand)=self.site_index.producers.get(f,selected,b)?;
        if let InstructionKind::Load(local)=operand.kind {
            if let Some(source)=super::owner_sources::scalar_preparations::copy_value_source(module,f,at_block,at,local,b)? {
                selected=source;
            }
        }
        self.assignment_value_source(module,caller,f,selected,base,b)
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
            let (_,_,actual)=self.site_index.producers.get(f,value,b)?;
            let InstructionKind::Binary{op:actual_op,left,right}=actual.kind else{return Err(bad("compound field assignment omits its original operator"));};
            if actual_op!=op{return Err(bad("compound field assignment changes its original operator"));}
            self.original_projection_source(module,caller,f,left,caller.definition,original.target,true,b)?;
            self.assignment_value_source(module,caller,f,right,original.value,b)
        }else{
            self.assignment_value_source(module,caller,f,value,original.value,b)
        }
    }

    pub(crate) fn assignment_writes(
        &self, module: &TypedModule, f: &TypedFunction, worlds: &[SemanticInstanceId], b: &mut Budget,
    )->Check<Vec<ValueId>> {
        let mut complete=None::<Vec<ValueId>>;
        for world in worlds {
            let caller = self.instance(module, *world, b)?;
            if caller.target != SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(f.id)) {
                return Err(bad("original assignment world belongs to another actual function"));
            }
            let originals=self.original.borrow().as_ref().ok_or_else(||bad("assignment has no independently checked original body"))?.assignments(module,caller.definition,b)?;
            let mut used=BTreeSet::new();
            let mut writes=Vec::new();
            for block in &f.blocks {for instruction in &block.instructions {
                b.spend(1)?;
                let InstructionKind::WriteAddress{address,value}=instruction.kind else{continue;};
                let carrier=f.values.get(value.0 as usize).ok_or_else(||bad("assignment actual value is absent"))?;
                if !carrier.is_inline_scalar_product(&mut || b.spend(1))? { continue; }
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
                    let(_,_,actual)=self.site_index.producers.get(f,value,b)?;
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

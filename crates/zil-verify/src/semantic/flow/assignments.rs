//! Original assignment correspondence, never a target, loan or access proof.
use super::*;
use std::collections::BTreeSet;

impl CheckedContext {
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
                    self.exact_expression_value(module,caller,f,right,original.value,b)?;
                } else {
                    self.exact_expression_value(module,caller,f,value,original.value,b)?;
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

//! A compiler-created argument reborrow adapts this outer parameter role.
//! It does not equate two SSA heads, retag a pointee class, or change the
//! caller's original lifetime actual/result image.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_argument(&self,module:&TypedModule,caller:&SemanticInstance,argument:ValueId,values:&[Option<CheckedType>],expected:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<bool>{
        let actual=value_at(values,argument)?;
        if self.compatible(environment,actual,expected,b)?{return Ok(true);}
        if actual.ty!=expected.ty||!matches!(actual.ty,Type::Address{..}){return Ok(false);}
        let mut rows=expected.life.slots.iter().filter(|row|row.path.is_empty());
        let Some(outer)=rows.next()else{return Ok(false);};if rows.next().is_some(){return Err(bad("Call parameter has duplicate outer borrow occurrences"));}
        // Independent child/phantom classes must retain their own origin.
        // A pre-existing collapsed representation cannot be repaired here.
        if expected.life.retained.iter().any(|row|row.binder==outer.binder)||expected.life.nominal_arguments.iter().any(|row|row.binder==outer.binder)||expected.life.slots.iter().any(|row|!row.path.is_empty()&&row.binder==outer.binder){return Err(unsupported("Call outer role shares an unresolved whole-child intrinsic class",None));}
        let Some(actual_outer)=actual.life.slots.iter().find(|row|row.path.is_empty()&&row.mode==outer.mode)else{return Ok(false);};
        let source=expected.classes.get(outer.binder as usize).ok_or_else(||bad("Call original outer class is absent"))?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=caller.target else{return Err(bad("Call borrow role has no actual caller"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("Call borrow caller is absent"))?;
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        let mut current=argument;let mut seen=std::collections::BTreeSet::new();let mut related=false;
        loop{
            b.spend(1)?;if !seen.insert(current){return Err(bad("Call argument reborrow has an SSA producer cycle"));}
            let mut producer=None;for block in &f.blocks{b.spend(block.instructions.len())?;for row in &block.instructions{if row.result==Some(current)&&producer.replace(row).is_some(){return Err(bad("Call argument has several SSA definitions"));}}}
            let InstructionKind::ReborrowAddress{address,..}=&producer.ok_or_else(||bad("Call argument borrow is undefined"))?.kind else{break;};
            current=*address;
            let parent=value_at(values,current)?;
            let Some(parent_outer)=parent.life.slots.iter().find(|row|row.path.is_empty()&&row.mode==outer.mode)else{break;};
            let key=parent.classes.get(parent_outer.binder as usize).ok_or_else(||bad("Call reborrow parent class is absent"))?;
            if self.same(environment,source,key,b)?{
                // The exact original expression head must be a real earlier
                // operand in this same caller, never another class image.
                let exact_occurrence=matches!(key,ClassKey::Outer{scope:owner,operand:OperandKey::Value(id),path,mode}if *owner==scope&&*id==current&&path.is_empty()&&*mode==outer.mode);
                let parameter_role=match key{
                    ClassKey::Formal{..}=>{
                        b.spend(environment.lexical.iter().fold(1usize,|n,frame|n.saturating_add(frame.actuals.len())))?;
                        environment.lexical.iter().any(|frame|frame.actuals.iter().any(|actual|matches!(actual,CheckedActual::Life(life)if life==key)))&&loaded_parameter(f,current,b)?
                    },
                    ClassKey::Implicit{scope:owner,..}if *owner==scope=>loaded_parameter(f,current,b)?,
                    _=>false,
                };
                if !exact_occurrence&&!parameter_role{return Ok(false);}
                if !self.compatible(environment,parent,expected,b)?{return Ok(false);}
                related=true;break;
            }
        }
        if !related{return Ok(false);}
        dynamic_metadata::ty(&expected.ty,b)?;
        b.spend(expected.classes.len().saturating_add(expected.origins.len()).saturating_add(expected.statics.len()))?;
        let mut role=expected.clone();
        role.classes[outer.binder as usize]=actual.classes.get(actual_outer.binder as usize).cloned().ok_or_else(||bad("Call actual child head is absent"))?;
        role.origins[outer.binder as usize]=actual.origins.get(actual_outer.binder as usize).cloned().ok_or_else(||bad("Call actual child head origin is absent"))?;
        self.same_type(environment,actual,&role,b)
    }
}

fn loaded_parameter(f:&TypedFunction,value:ValueId,b:&mut Budget)->Check<bool>{
    let mut selected=None;
    for block in &f.blocks{b.spend(block.instructions.len())?;for operation in &block.instructions{if operation.result==Some(value)&&selected.replace(operation).is_some(){return Err(bad("Call parameter head has several actual SSA definitions"));}}}
    b.spend(f.parameters.len())?;
    Ok(matches!(selected.map(|row|&row.kind),Some(InstructionKind::Load(local))if f.parameters.contains(local)))
}

//! Phantom classes are logical occurrences, not borrow roots. A physical
//! captured value still has to be the exact original free binder at this
//! creation; matching a nominal type or an outside lifetime is insufficient.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn phantom_capture(&self,module:&TypedModule,creator:&SemanticInstance,child:&SemanticInstance,node:zil_base::NodeId,value:ValueId,actual:&CheckedType,sealed:&[CheckedActual],environment:&EvalEnv,b:&mut Budget)->Check<usize>{
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("logical capture creator is not an actual function"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("logical capture creator function is absent"))?;
        let mut selected=None;
        for(ordinal,expected)in sealed.iter().enumerate(){
            let CheckedActual::Type(expected)=expected else{continue;};
            if expected.ty!=actual.ty{continue;}
            phantom_only(expected,b)?;
            let source=self.original.borrow().as_ref().ok_or_else(||bad("logical capture has no checked original body"))?.ghost_capture_source(module,creator.definition,child.definition,ordinal,node,b)?;
            // Resolve every same-shaped original candidate by its real binder.
            // A different candidate cannot donate its class inventory.
            let local=self.buffer_source_local(module,creator,f,source,b)?;
            if !capture_local(f,value,local,b)?{continue;}
            if !self.same_type(environment,actual,expected,b)?{return Err(bad("logical capture changes the complete original type/class/origin occurrence"));}
            if selected.replace(ordinal).is_some(){return Err(bad("logical capture has several original environment sources"));}
        }
        selected.ok_or_else(||bad("logical capture does not select its exact original free-value binding"))
    }
}
pub(super) fn phantom_only(value:&CheckedType,b:&mut Budget)->Check<()> {
    if !value.statics.is_empty()||value.classes.len()!=value.life.binder_count as usize||value.origins.len()!=value.classes.len(){return Err(bad("logical phantom domain is incomplete or has pending static memory"));}
    let mut pending=vec![&value.ty];
    if !value.life.slots.is_empty()||!value.life.retained.is_empty(){return Err(unsupported("logical capture requires its complete retained borrow consumer",None));}
    while let Some(ty)=pending.pop(){b.spend(1)?;match ty{
        Type::BorrowedStorage{storage,life}=>{if !life.slots.is_empty()||!life.retained.is_empty(){return Err(unsupported("logical phantom capture crosses an actual borrow occurrence",None));}pending.push(storage);},
        Type::Tuple(fields)=>pending.extend(fields),Type::Record{fields,..}|Type::Package{fields,..}=>pending.extend(fields.iter().map(|(_,ty)|ty)),
        Type::Enum{variants,..}=>pending.extend(variants.iter().flat_map(|(_,fields)|fields)),Type::Array{element,..}=>pending.push(element),
        ty if ty.is_b0_scalar()=>{},
        _=>return Err(unsupported("logical phantom capture has an opaque/owning or borrowed child domain",None)),
    }}Ok(())
}
fn capture_local(f:&TypedFunction,value:ValueId,local:LocalId,b:&mut Budget)->Check<bool>{
    let mut producer=None;
    for block in &f.blocks{b.spend(block.instructions.len())?;for instruction in &block.instructions{if instruction.result==Some(value)&&producer.replace(instruction).is_some(){return Err(bad("logical capture operand has several actual SSA definitions"));}}}
    match &producer.ok_or_else(||bad("logical capture actual operand is undefined"))?.kind{InstructionKind::Load(actual)|InstructionKind::TakeLocal{local:actual}=>Ok(*actual==local),_=>Ok(false)}
}

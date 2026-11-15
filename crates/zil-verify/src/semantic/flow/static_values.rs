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
            let &SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(mut value),..}=&site.action else{continue;};
            let mut seen=std::collections::BTreeSet::new();loop{
                if value==result{break;}b.spend(1)?;if !seen.insert(value){return Err(bad("static literal role contains an actual SSA cycle"));}
                let mut producer=None;for block in &f.blocks{b.spend(block.instructions.len())?;for operation in &block.instructions{if operation.result==Some(value)&&producer.replace(operation).is_some(){return Err(bad("static literal role has duplicate actual SSA definitions"));}}}
                match producer.map(|row|&row.kind){Some(InstructionKind::ReborrowAddress{address,mutable:false})=>value=*address,_=>break}
            }
            if value!=result{continue;}
            let original=self.original.borrow();let original=original.as_ref().ok_or_else(||bad("static literal has no independently checked original body"))?;
            if let Some(expected)=original.static_literal(module,instance.definition,site.recipe,b)?{b.spend(expected.len().saturating_add(bytes.len()))?;if expected!=*bytes{return Err(bad("actual static bytes differ from their exact original literal"));}matched=true;}
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

//! Original implicit capture-head occurrences select real entry reads. The
//! traversal stops at external stored-borrow edges and never creates a root.
use super::*;
use std::collections::BTreeSet;
impl CheckedContext {
    pub(super) fn capture_implicit(&self,module:&TypedModule,child:&SemanticInstance,index:u32,b:&mut Budget)->Check<Option<ClassKey>> {
        let definition=self.definition(module,child.definition,b)?;
        let SemanticDefinitionBody::Function{sites,..}=&definition.body else{return Err(bad("capture-head declaration is not an actual function"));};
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=child.target else{return Err(bad("capture-head instance is not an actual function"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("capture-head actual function is absent"))?;
        let program=module.semantic.as_ref().ok_or_else(||bad("capture-head semantic program is absent"))?;
        let mut selected=None;
        for implicit in &definition.implicit_lifetimes {
            let mut found=BTreeSet::new();let mut observed=false;
            for recipe in sites.iter().filter(|recipe|recipe.occurrence_path==implicit.occurrence_path) {
                if !matches!(recipe.kind,SemanticSiteRecipeKind::TypeUse{..}){continue;}
                for site in program.sites.iter().filter(|site|site.instance==child.id&&site.recipe==recipe.id) {
                    b.spend(1)?;
                    let SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(value),..}=&site.action else {return Err(unsupported("implicit capture head requires its real SSA use occurrence",None));};
                    let (_,instruction)=definition_of(f,*value,b)?;
                    let InstructionKind::ReadAddress{address}=&instruction.kind else {continue;};
                    observed=true;
                    origins(f,*address,&mut found,b)?;
                }
            }
            if observed&&found.contains(&index) {
                if found.len()!=1 {return Err(unsupported("implicit capture occurrence has several unresolved actual heads",None));}
                let key=ClassKey::Implicit{scope:ScopeKey::Instance(child.id),ordinal:implicit.ordinal};
                if selected.as_ref().is_some_and(|previous|previous!=&key) {return Err(bad("one actual capture head has conflicting original implicit occurrences"));}
                selected=Some(key);
            }
        }
        Ok(selected)
    }
}
fn definition_of(f:&TypedFunction,value:ValueId,b:&mut Budget)->Check<(BlockId,&Instruction)> {
    let mut selected=None;
    for block in &f.blocks {b.spend(block.instructions.len())?;for instruction in &block.instructions {if instruction.result==Some(value) {if selected.is_some(){return Err(bad("capture-head SSA value has several actual definitions"));}selected=Some((block.id,instruction));}}}
    selected.ok_or_else(||bad("capture-head actual SSA definition is absent"))
}
fn origins(f:&TypedFunction,value:ValueId,out:&mut BTreeSet<u32>,b:&mut Budget)->Check<()> {
    let mut pending=vec![value];let mut visited=BTreeSet::new();
    while let Some(value)=pending.pop() {
        b.spend(1)?;if !visited.insert(value){continue;}
        let (_,instruction)=definition_of(f,value,b)?;
        match &instruction.kind {
            InstructionKind::ReadCapture{index}|InstructionKind::TakeCapture{index}=>{out.insert(*index);}
            InstructionKind::ReborrowAddress{address,..}|InstructionKind::AddressField{address,..}=>pending.push(*address),
            InstructionKind::Load(local)|InstructionKind::TakeLocal{local}=>{
                // All actual stores form an upper. An unrelated/unknown store
                // is rejected rather than selecting a convenient latest one.
                let mut observed=false;
                for block in &f.blocks {b.spend(block.instructions.len())?;for instruction in &block.instructions {if let InstructionKind::Store{local:target,value}=&instruction.kind {if target==local {observed=true;pending.push(*value);}}}}
                if !observed{return Err(unsupported("capture-head local has no actual stored origin",None));}
            }
            // A loaded address points at external data, not at the holder's
            // capture head. This route needs the independent stored solver.
            InstructionKind::LoadStoredBorrow{..}=>return Err(unsupported("implicit capture head crosses an external stored-borrow edge",None)),
            _=>return Err(unsupported("implicit capture head has no complete actual Capture provenance",None)),
        }
    }
    Ok(())
}

//! Actual Commit transfers one staged responsibility into an already live
//! owning position. The old destination has no second invented local debt.
use super::*;
use crate::place_replacements::{Inventory,actual};
pub(super) struct Plan {
    actual:actual::Program,
    inventories:Vec<Inventory>,
    written:Vec<BTreeSet<Root>>,
}
impl Plan {
    pub(super) fn new(module:&TypedModule,graphs:&[FunctionGraph],writes:&[BTreeSet<Root>],b:&mut Budget)->Check<Self>{
        let actual=actual::Program::reconstruct(module,graphs,b)?;
        b.spend(module.functions.len().saturating_add(writes.len()))?;
        let mut inventories=Vec::with_capacity(module.functions.len());
        for f in &module.functions {inventories.push(Inventory::reconstruct(module,f,b)?);}
        for row in writes {b.spend(row.len())?;}
        Ok(Self{actual,inventories,written:writes.to_vec()})
    }
    pub(super) fn writes(&self,id:FunctionId,b:&mut Budget)->Check<BTreeSet<Root>>{
        let row=&self.written[id.0 as usize];b.spend(row.len())?;Ok(row.clone())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn step(&self,transfer:&Transfer<'_>,state:&mut State,instruction:&Instruction,position:usize,block:usize,validate:bool,written:&mut BTreeSet<Root>,b:&mut Budget)->Check<()> {
        let InstructionKind::CommitPlaceReplace{reservation,value}=instruction.kind else{return Ok(())};
        let f=transfer.f;let span=instruction.span;
        let place=self.inventories[f.id.0 as usize].get(reservation,b)?;
        let source=transfer.slots.managed_value[value.0 as usize].ok_or_else(||bad(span,"replacement RHS has no actual resource responsibility"))?;
        if validate&&state.debt[source]!=LIVE{return Err(bad(span,"replacement RHS is absent or was consumed before Commit"))}
        let fresh=state.fresh[source];
        for target in self.actual.selected(f,place,b)? {
            sums::replacements::at_type(transfer.module,f,&target,b)?;
            match target.base {
                actual::Base::Local(local)=>{
                    let shell=transfer.slots.managed_local[local.0 as usize].ok_or_else(||bad(span,"replacement root has no actual owner responsibility"))?;
                    if validate&&state.debt[shell]!=LIVE{return Err(bad(span,"replacement owner shell is absent or moved"))}
                    if let Type::Package{fields,..}=&f.locals[local.0 as usize].ty
                        && let Some(zil_ir::PlaceProjection::Field(index))=target.steps.first()
                        && matches!(fields[*index as usize].1,Type::Managed(_)|Type::Package{..}) {
                        let field=transfer.slots.fields(local)[*index as usize].ok_or_else(||bad(span,"replacement owning field has no original responsibility"))?;
                        if validate&&state.debt[field]!=LIVE{return Err(bad(span,"replacement owning field was already moved or cleaned"))}
                        if validate {no_live_description(state,&transfer.after[block][position],Root::PackageField{local,index:*index},span,b)?;}
                        if target.steps.len()==1 {state.fresh[field]=fresh;}
                    }else if target.steps.is_empty(){
                        if validate {no_live_description(state,&transfer.after[block][position],Root::Local(local),span,b)?;}
                        state.fresh[shell]=fresh;
                    }
                    // Preserve shell debt, initialization, partial siblings
                    // and lexical cleanup age; the old value is replaced once.
                }
                actual::Base::Input(p)|actual::Base::InputCapture(p,_)=>{b.spend(written.len().saturating_add(1))?;written.insert(Root::Parameter(p));}
                actual::Base::Environment(c)=>{
                    if f.environment.is_some_and(|id|transfer.module.closures[id.0 as usize].kind==CallableKind::Fn){return Err(bad(span,"shared Fn entry cannot replace through a captured exclusive address"))}
                    b.spend(written.len().saturating_add(1))?;written.insert(Root::BorrowedCapture(c));
                }
                actual::Base::Element(_)=>return Err(unsupported("Store element replacement debt bridge is under construction",Some(span))),
            }
        }
        transfer.consume(state,value,span,validate)
    }
}

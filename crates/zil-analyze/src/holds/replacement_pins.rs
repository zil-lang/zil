//! A prepared position is pinned without activating an ordinary exclusive
//! payload loan. Reads remain ordinary; writes and invalidation must be apart.
use super::*;
use zil_ir::ReservationId;
fn writes(i:&Instruction)->bool {
    !matches!(i.kind,
        InstructionKind::ReadAddress{..}|InstructionKind::ReadDesc{..}
        |InstructionKind::Load(_)|InstructionKind::ReadCapture{..}
        |InstructionKind::BorrowLocal{..}|InstructionKind::VariantTagLocal{..}
        |InstructionKind::ReadVariantLocal{..}|InstructionKind::PlaceTagLocal{..}
        |InstructionKind::PlaceTagAddress{..}|InstructionKind::VariantTagAddress{..}
        |InstructionKind::BorrowPlaceLocal{mutable:false,..}
        |InstructionKind::BorrowVariantLocal{mutable:false,..}
        |InstructionKind::BorrowFieldLocal{mutable:false,..}
        |InstructionKind::BorrowPlaceAddress{mutable:false,..}
        |InstructionKind::BorrowVariantAddress{mutable:false,..}
        |InstructionKind::ReborrowAddress{mutable:false,..})
}
fn extra_access(module:&TypedModule,f:&TypedFunction,i:&Instruction,values:&[Roots],state:&State,b:&mut Budget)->CompileResult<Roots>{
    let mut out=Roots::new();
    let field=match i.kind {
        InstructionKind::StorePackageField{local,index,..}|InstructionKind::DropPackageField{local,index}
        |InstructionKind::TakePackageField{local,index}=>Some((local,index)),_=>None,
    };
    if let Some((local,index))=field {
        b.spend(1,"pin owning field path")?;
        out.insert(mixed_root(f,local,&[zil_ir::PlaceProjection::Field(index)],false,b)?);
    }
    // Unknown actual writes/callbacks use the complete real mutable argument
    // footprint. Shared reads do not become exclusive merely due to a pin.
    let ticket = matches!(i.kind, InstructionKind::ReservePlaceLocal{..}|InstructionKind::ReservePlaceAddress{..}|InstructionKind::CommitPlaceReplace{..}|InstructionKind::AbortPlaceReservation{..});
    let precise = if ticket { false } else { pattern_access(f, i, values, None, b)?.is_some() };
    let selection = matches!(i.kind, InstructionKind::AddressField{..}|InstructionKind::AddressIndex{..}|InstructionKind::AddressSlice{..});
    if writes(i) && !precise && !ticket && !selection {
        for value in resources::uses(&i.kind) {
            b.spend(1,"pin actual mutation operand")?;
            let ty=&f.values[value.0 as usize];
            let mutating=match ty {
                Type::Address{mutable,..}=>*mutable,
                Type::Desc(_)=>true,
                Type::Closure(_)=>thread_exclusive_type(module,ty,b)?,
                _=>false,
            };
            if mutating { extend(&mut out,&values[value.0 as usize],b)?; }
        }
        if let InstructionKind::CallClosureMut{callee,..}|InstructionKind::CallClosureShared{callee,..}=i.kind {
            let ty=&f.locals[callee.0 as usize].ty;
            if thread_exclusive_type(module,ty,b)? {
                b.lookup(state.len(),"pin actual callback local")?;
                if let Some(held)=state.get(&callee){extend(&mut out,held,b)?;}
            }
        }
    }
    Ok(out)
}
pub(super) fn transition(module:&TypedModule,f:&TypedFunction,i:&Instruction,values:&[Roots],state:&State,active:&BTreeSet<ReservationId>,definitions:&crate::place_replacements::Definitions,b:&mut Budget)->CompileResult<()> {
    let settlement=match i.kind {
        InstructionKind::CommitPlaceReplace{reservation,..}|InstructionKind::AbortPlaceReservation{reservation}=>Some(reservation),_=>None,
    };
    let mut access=extra_access(module,f,i,values,state,b)?;
    match &i.kind {
        InstructionKind::ReservePlaceLocal{reservation,..}|InstructionKind::ReservePlaceAddress{reservation,..}=>{
            let d=definitions.definition(*reservation,b)?;
            extend(&mut access,&replacement_roots(f,d,values,b)?,b)?;
        }
        InstructionKind::AbortPlaceReservation{..}=>{}
        _=>if let Some((roots,true,_))=pattern_access(f,i,values,Some(definitions),b)? {extend(&mut access,&roots,b)?;},
    }
    b.spend(active.len(),"pin active mutation check")?;
    for id in active {
        if settlement==Some(*id){continue}
        let definition=definitions.definition(*id,b)?;
        let pinned=replacement_roots(f,definition,values,b)?;
        if overlaps(&pinned,&access,b)? {return Err(window_conflict(i,"mutation or transfer invalidates an active owning replacement pin"))}
    }
    Ok(())
}

#[cfg(test)]
mod tests;

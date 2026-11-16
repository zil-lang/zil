//! Exact EvalOnce scalar transport. A temporary name or scalar shape alone
//! proves nothing: the unique actual Store must dominate this Load in the
//! same active storage epoch, and the storage must never acquire an alias.
use super::*;

pub(super) struct Slot {
    pub(super) live:(BlockId,usize),
    pub(super) stores:Vec<(BlockId,usize,ValueId)>,
    pub(super) loads:Vec<(BlockId,usize,Option<ValueId>)>,
    pub(super) dead:Vec<(BlockId,usize)>,
}

pub(super) fn slot(f:&TypedFunction,local:LocalId,readonly:bool,b:&mut Budget)->Check<Option<Slot>> {
    let declaration=f.locals.get(local.0 as usize).ok_or_else(||bad("scalar source local is absent"))?;
    b.spend(f.parameters.len().saturating_add(1))?;
    if readonly&&declaration.mutable||f.parameters.contains(&local){return Ok(None);}
    let ty=&declaration.ty;
    if !ty.is_integer()&&!ty.is_float()&&!matches!(ty,Type::Unit|Type::Bool){return Ok(None);}
    crate::stored_flow::type_work(ty,&b.limits.clone(),b)?;
    let mut live=None;let mut stores=Vec::new();let mut loads=Vec::new();let mut dead=Vec::new();
    for body in &f.blocks {
        b.spend(body.instructions.len().saturating_add(1))?;
        for(index,instruction)in body.instructions.iter().enumerate(){
            match &instruction.kind {
                InstructionKind::StorageLive(actual)if *actual==local=>{
                    if live.replace((body.id,index)).is_some(){return Ok(None);}
                },
                InstructionKind::Store{local:actual,value}if *actual==local=>{
                    if stores.len()==stores.capacity(){b.spend(stores.len().saturating_add(1))?;}b.spend(1)?;stores.push((body.id,index,*value));
                },
                InstructionKind::BorrowDynOwned{local:actual,..}|InstructionKind::DropDynOwned{local:actual}
                |InstructionKind::RegisterDefer{local:actual,..}|InstructionKind::DropErrorUnion{local:actual}
                |InstructionKind::ProjectOwnedArrayField{local:actual,..}|InstructionKind::TakeOwnedArrayElement{local:actual,..}
                |InstructionKind::DropOwnedArrayElement{local:actual,..}|InstructionKind::StoreOwnedArrayElement{local:actual,..}
                |InstructionKind::DropOwnedArray{local:actual}|InstructionKind::BorrowOwnedArrayElement{local:actual,..}
                |InstructionKind::BorrowPackageField{local:actual,..}|InstructionKind::ReadPackageField{local:actual,..}
                |InstructionKind::DescribePackageField{local:actual,..}|InstructionKind::DropPackage{local:actual}
                |InstructionKind::TakePackageField{local:actual,..}|InstructionKind::DropPackageField{local:actual,..}
                |InstructionKind::StorePackageField{local:actual,..}|InstructionKind::BindPackageCapture{local:actual,..}
                |InstructionKind::EndPackageCapture{local:actual,..}|InstructionKind::DescribeLocal{local:actual}
                |InstructionKind::DropManaged{local:actual}|InstructionKind::TakeLocal{local:actual}
                |InstructionKind::BorrowLocal{local:actual}|InstructionKind::BorrowLocalMut{local:actual}
                |InstructionKind::DropMutexGuard{local:actual}|InstructionKind::DropTuple{local:actual}
                |InstructionKind::TakeTupleField{local:actual,..}|InstructionKind::TakeVariantField{local:actual,..}
                |InstructionKind::BorrowPlaceLocal{local:actual,..}|InstructionKind::PlaceTagLocal{local:actual,..}
                |InstructionKind::ReservePlaceLocal{local:actual,..}|InstructionKind::BorrowFieldLocal{local:actual,..}
                |InstructionKind::VariantTagLocal{local:actual,..}|InstructionKind::ReadVariantLocal{local:actual,..}
                |InstructionKind::BorrowVariantLocal{local:actual,..}|InstructionKind::DropEnum{local:actual}
                |InstructionKind::DropClosure{local:actual}|InstructionKind::CallClosureShared{callee:actual,..}
                |InstructionKind::CallClosureMut{callee:actual,..}if *actual==local=>return Ok(None),
                InstructionKind::UnpackTuple{local:source,fields}|InstructionKind::UnpackVariant{local:source,fields,..}=>{
                    b.spend(fields.len())?;
                    if *source==local||fields.contains(&local){return Ok(None);}
                },
                InstructionKind::Load(actual)if *actual==local=>{if loads.len()==loads.capacity(){b.spend(loads.len().saturating_add(1))?;}b.spend(1)?;loads.push((body.id,index,instruction.result));},
                InstructionKind::StorageDead(actual)if *actual==local=>{if dead.len()==dead.capacity(){b.spend(dead.len().saturating_add(1))?;}b.spend(1)?;dead.push((body.id,index));},
                _=>{},
            }
        }
    }
    Ok(live.map(|live|Slot{live,stores,loads,dead}))
}

pub(super) fn source(module:&TypedModule,f:&TypedFunction,block:BlockId,at:u32,local:LocalId,b:&mut Budget)->Check<Option<ValueId>> {
    let Some(slot)=slot(f,local,true,b)? else{return Ok(None);};
    let [(store_block,store_at,value)]=slot.stores.as_slice()else{return Ok(None);};
    let(live_block,live_at)=slot.live;
    let declaration=&f.locals[local.0 as usize];
    if live_block!=block||*store_block!=block||live_at.checked_add(1)!=Some(*store_at)||*store_at>=at as usize{return Ok(None);}
    let body=f.blocks.get(block.0 as usize).filter(|body|body.id==block).ok_or_else(||bad("scalar preparation block is absent"))?;
    let activation=&body.instructions[live_at];let assignment=&body.instructions[*store_at];
    if activation.span!=declaration.span||assignment.span!=declaration.span{return Ok(None);}
    let Some(load)=body.instructions.get(at as usize)else{return Err(bad("scalar preparation Load point is absent"));};
    if load.kind!=InstructionKind::Load(local){return Err(bad("scalar preparation consumer is another actual operation"));}
    let Some(interval)=body.instructions.get(*store_at+1..at as usize)else{return Err(bad("scalar preparation storage interval is reversed"));};
    b.spend(interval.len())?;
    if interval.iter().any(|instruction|instruction.kind==InstructionKind::StorageDead(local)
        ||matches!(instruction.kind,InstructionKind::ScopeExit{..})){return Ok(None);}
    crate::check_instruction(module,f,activation,b)?;
    crate::check_instruction(module,f,assignment,b)?;
    crate::check_instruction(module,f,load,b)?;
    Ok(Some(*value))
}

#[cfg(test)]
mod tests {
    use super::*;
    mod fixture {
        include!(concat!(env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"));
    }
    fn prepared()->(TypedModule,TypedFunction){
        let module=fixture::module();let mut f=module.functions[0].clone();
        f.environment=None;f.parameters=vec![LocalId(0)];f.locals.truncate(1);
        f.locals[0].ty=Type::I64;
        let mut slot=f.locals[0].clone();slot.id=LocalId(1);slot.mutable=false;
        // The spelling is arbitrary and is not inspected by the source proof.
        slot.name="ordinary_scalar_binding".into();f.locals.push(slot);
        f.values=vec![Type::I64,Type::I64,Type::I64];f.return_type=Type::I64;
        f.blocks.truncate(1);
        f.blocks[0].instructions=vec![
            fixture::instruction(Some(0),InstructionKind::Load(LocalId(0))),
            fixture::instruction(None,InstructionKind::StorageLive(LocalId(1))),
            fixture::instruction(None,InstructionKind::Store{local:LocalId(1),value:ValueId(0)}),
            fixture::instruction(Some(1),InstructionKind::Load(LocalId(1))),
            fixture::instruction(None,InstructionKind::StorageDead(LocalId(1))),
        ];
        f.blocks[0].terminator=Terminator::Return(Some(ValueId(1)));
        (module,f)
    }
    fn prove(module:&TypedModule,f:&TypedFunction,block:u32,at:u32)->Check<Option<ValueId>>{
        source(module,f,BlockId(block),at,LocalId(1),&mut Budget::new(&Limits::default()))
    }
    #[test]
    fn scalar_preparation_keeps_exact_stored_ssa_not_same_type_or_name(){
        let(module,mut f)=prepared();
        assert_eq!(prove(&module,&f,0,3).unwrap(),Some(ValueId(0)));
        f.locals[1].name="$some_other_generated_name".into();
        assert_eq!(prove(&module,&f,0,3).unwrap(),Some(ValueId(0)));
        f.blocks[0].instructions.insert(1,fixture::instruction(Some(2),InstructionKind::Constant(Constant::I64(7))));
        f.blocks[0].instructions[3].kind=InstructionKind::Store{local:LocalId(1),value:ValueId(2)};
        // Both inputs are I64. The enclosing exact-expression matcher must
        // now see SSA2, which is not the original anchored SSA0.
        assert_eq!(prove(&module,&f,0,4).unwrap(),Some(ValueId(2)));
        assert_ne!(prove(&module,&f,0,4).unwrap(),Some(ValueId(0)));
    }
    #[test]
    fn scalar_preparation_rejects_other_writes_epochs_aliases_and_domains(){
        for case in 0..13{
            let(module,mut f)=prepared();let mut block=0;let mut at=3;
            match case{
                0=>{f.blocks[0].instructions.insert(3,fixture::instruction(None,InstructionKind::Store{local:LocalId(1),value:ValueId(0)}));at=4;},
                1=>{f.blocks[0].instructions.insert(3,fixture::instruction(None,InstructionKind::StorageLive(LocalId(1))));at=4;},
                2=>{f.blocks[0].instructions.insert(3,fixture::instruction(None,InstructionKind::StorageDead(LocalId(1))));at=4;},
                3=>{f.blocks[0].instructions.swap(2,3);at=2;},
                4=>f.locals[1].mutable=true,
                5=>f.parameters.push(LocalId(1)),
                6=>{
                    let mut other=f.blocks[0].clone();other.id=BlockId(1);
                    other.instructions=vec![fixture::instruction(Some(1),InstructionKind::Load(LocalId(1)))];
                    f.blocks[0].instructions.remove(3);f.blocks.push(other);block=1;at=0;
                },
                7|8=>{
                    let mut other=f.blocks[0].clone();other.id=BlockId(1);
                    other.instructions=vec![fixture::instruction(None,if case==7{InstructionKind::BorrowLocalMut{local:LocalId(1)}}else{InstructionKind::Store{local:LocalId(1),value:ValueId(0)}})];
                    f.blocks.push(other);
                },
                9=>f.locals[1].ty=Type::Address{mutable:false,inner:Box::new(Type::I64)},
                10=>f.locals[1].ty=Type::Desc(Box::new(Type::ByteBuffer)),
                11=>{f.blocks[0].instructions.insert(3,fixture::instruction(None,InstructionKind::ScopeExit{scope:ScopeId(0),kind:CleanupExitKind::Fallthrough}));at=4;},
                _=>{f.blocks[0].instructions.insert(3,fixture::instruction(None,InstructionKind::UnpackTuple{local:LocalId(0),fields:vec![LocalId(1)]}));at=4;},
            }
            assert_eq!(prove(&module,&f,block,at).unwrap(),None,"case {case}");
        }
        let(module,mut f)=prepared();
        f.values[0]=Type::U8;
        assert!(prove(&module,&f,0,3).is_err(),"a Store cannot change the complete scalar carrier");
    }
}

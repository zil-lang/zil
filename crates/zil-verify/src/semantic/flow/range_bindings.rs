//! Actual integer iteration storage is selected by the complete range stage,
//! not by a scalar type or a generated counter name. No loan is produced here.
use super::*;

fn one(value:&Constant)->bool {
    matches!(value,Constant::I8(1)|Constant::I16(1)|Constant::I32(1)|Constant::I64(1)|Constant::Isize(1)
        |Constant::U8(1)|Constant::U16(1)|Constant::U32(1)|Constant::U64(1)|Constant::Usize(1))
}
fn successors(block:&BasicBlock)->Vec<BlockId>{match block.terminator{
    Terminator::Jump(next)=>vec![next],Terminator::Branch{then_block,else_block,..}=>vec![then_block,else_block],_=>vec![],
}}
pub(super) fn check(producers: &sites::Producers, module:&TypedModule,f:&TypedFunction,local:LocalId,range:&original::RangeBinding,b:&mut Budget)->Check<(ValueId,ValueId)> {
    let declaration=f.locals.get(local.0 as usize).ok_or_else(||bad("range actual iteration local is absent"))?;
    b.spend(f.locals.len().saturating_add(range.name.len()))?;
    if declaration.mutable||!declaration.ty.is_integer()||declaration.name!=range.name||declaration.span!=range.span
        ||f.locals.iter().filter(|other|other.name==range.name&&other.span==range.span&&!other.mutable&&other.ty.is_integer()).count()!=1{
        return Err(bad("range changes its unique original iteration binding"));
    }
    let iteration=scalar_preparations::iteration_slot(f,local,b)?.ok_or_else(||bad("range iteration storage is aliased, replaced or lacks an activation"))?;
    let [(body,store_at,current)]=iteration.stores.as_slice()else{return Err(bad("range iteration binding does not have one actual snapshot producer"));};
    let(head,load_at,load)=producers.get(f,*current,b)?;
    crate::check_instruction(module,f,load,b)?;
    let InstructionKind::Load(counter)=load.kind else{return Err(bad("range iteration snapshot is not its actual header counter"));};
    if counter==local{return Err(bad("range iteration snapshot aliases its mutable counter"));}
    let counter_declaration=f.locals.get(counter.0 as usize).ok_or_else(||bad("range actual counter storage is absent"))?;
    crate::stored_flow::type_work(&counter_declaration.ty,&b.limits.clone(),b)?;
    if counter_declaration.ty!=declaration.ty||counter_declaration.span!=range.span{return Err(bad("range counter changes its complete original integer carrier or occurrence"));}
    let counter_slot=scalar_preparations::slot(f,counter,false,b)?.ok_or_else(||bad("range counter storage escapes or has an unrelated writer"))?;
    if counter_slot.loads.as_slice()!=[(head,load_at as usize,Some(*current))]{return Err(bad("range counter is observed outside its unique actual header"));}
    let header=f.blocks.get(head.0 as usize).filter(|block|block.id==head).ok_or_else(||bad("range header is absent"))?;
    let Terminator::Branch{condition,then_block,else_block:after}=header.terminator else{return Err(bad("range header does not branch on its actual comparison"));};
    if then_block!=*body||head==*body||head==after||*body==after{return Err(bad("range header changes its body/exit stage"));}
    let(condition_block,_,comparison)=producers.get(f,condition,b)?;
    crate::check_instruction(module,f,comparison,b)?;
    let InstructionKind::Binary{op:BinaryOp::Less,left,right:end}=comparison.kind else{return Err(bad("range header omits its strict original end bound"));};
    if condition_block!=head||left!=*current{return Err(bad("range comparison uses another actual counter snapshot"));}
    let body_block=f.blocks.get(body.0 as usize).filter(|block|block.id==*body).ok_or_else(||bad("range body entry is absent"))?;
    let binding=body_block.instructions.get(*store_at).ok_or_else(||bad("range snapshot storage point is absent"))?;
    crate::check_instruction(module,f,binding,b)?;
    b.spend(*store_at)?;
    if binding.span!=range.span||body_block.instructions[..*store_at].iter().any(|instruction|!matches!(instruction.kind,InstructionKind::ScopeEnter{..})){
        return Err(bad("range iteration value is not bound at its true guarded body entry"));
    }
    let mut initial=None;let mut step=None;
    for &(block,index,value)in &counter_slot.stores {
        b.spend(1)?;
        let instruction=f.blocks.get(block.0 as usize).and_then(|body|body.instructions.get(index)).ok_or_else(||bad("range counter write is absent"))?;
        crate::check_instruction(module,f,instruction,b)?;
        if instruction.span!=range.span{return Err(bad("range counter write changes its original loop occurrence"));}
        if block==iteration.live.0&&block==counter_slot.live.0 {
            if index<=iteration.live.1||index<=counter_slot.live.1||initial.replace((block,index,value)).is_some(){return Err(bad("range counter has no unique initial endpoint binding"));}
        }else{
            if step.replace((block,index,value)).is_some(){return Err(bad("range counter has several unrelated step writes"));}
        }
    }
    let(preheader,_,start)=initial.ok_or_else(||bad("range counter omits its original start value"))?;
    if !matches!(f.blocks[preheader.0 as usize].terminator,Terminator::Jump(next)if next==head){return Err(bad("range initialization does not enter its true header"));}
    for &(block,index)in &[iteration.live,counter_slot.live] {
        let instruction=&f.blocks[block.0 as usize].instructions[index];
        crate::check_instruction(module,f,instruction,b)?;
        if instruction.span!=range.span{return Err(bad("range storage activation changes its original statement"));}
    }
    if let Some((block,index,value))=step {
        let(step_block,add_at,addition)=producers.get(f,value,b)?;
        crate::check_instruction(module,f,addition,b)?;
        let InstructionKind::Binary{op:BinaryOp::Add,left,right}=addition.kind else{return Err(bad("range step is not its complete integer successor"));};
        if step_block!=block||add_at as usize>=index||left!=*current{return Err(bad("range step uses another header or storage epoch"));}
        let(one_block,one_at,literal)=producers.get(f,right,b)?;
        crate::check_instruction(module,f,literal,b)?;
        if one_block!=block||one_at>=add_at||!matches!(&literal.kind,InstructionKind::Constant(value)if one(value))
            ||!matches!(f.blocks[block.0 as usize].terminator,Terminator::Jump(next)if next==head){return Err(bad("range step changes its exact increment or backedge"));}
    }
    b.spend(f.blocks.len().saturating_mul(3))?;
    let mut predecessors=vec![Vec::new();f.blocks.len()];
    for block in &f.blocks {for next in successors(block){b.spend(1)?;predecessors.get_mut(next.0 as usize).ok_or_else(||bad("range CFG successor is absent"))?.push(block.id);}}
    if predecessors[body.0 as usize].as_slice()!=[head]{return Err(bad("range body snapshot can be entered without its header guard"));}
    if predecessors[head.0 as usize].iter().any(|previous|*previous!=preheader&&Some(*previous)!=step.map(|step|step.0)){
        return Err(bad("range header has an unproved counter initialization/backedge"));
    }
    let mut region=vec![false;f.blocks.len()];let mut pending=vec![*body];
    while let Some(block)=pending.pop(){
        b.spend(1)?;
        if block==after||Some(block)==step.map(|step|step.0){continue;}
        if block==head{return Err(bad("range body skips its actual successor stage"));}
        let visited=region.get_mut(block.0 as usize).ok_or_else(||bad("range body CFG block is absent"))?;
        if *visited{continue;}*visited=true;
        for next in successors(&f.blocks[block.0 as usize]){b.spend(pending.len().saturating_add(1))?;pending.push(next);}
    }
    for (block,parents)in predecessors.iter().enumerate(){
        if region[block] {
            b.spend(parents.len())?;
            if parents.iter().any(|parent|!region[parent.0 as usize]&&!(block==body.0 as usize&&*parent==head)){
                return Err(bad("range iteration body admits a foreign entry"));
            }
        }
    }
    if let Some((step,_,_))=step {
        b.spend(predecessors[step.0 as usize].len())?;
        if predecessors[step.0 as usize].iter().any(|parent|!region[parent.0 as usize]){return Err(bad("range step is entered outside its original body"));}
    }
    for &(block,at,_)in &iteration.loads {b.spend(1)?;if !region[block.0 as usize] || (block==*body && at<=*store_at) {return Err(bad("range binding is read outside its current guarded iteration"));}}
    for &(block,_)in iteration.dead.iter().chain(&counter_slot.dead){
        b.spend(1)?;
        if block!=after&&!matches!(f.blocks[block.0 as usize].terminator,Terminator::Return(_)|Terminator::Trap|Terminator::Unreachable){return Err(bad("range storage is ended before its current iteration stage settles"));}
    }
    let _=range.statement;
    Ok((start,end))
}

#[cfg(test)]
mod tests {
    use super::*;
    mod fixture{include!(concat!(env!("CARGO_MANIFEST_DIR"),"/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"));}
    fn loop_body()->(TypedModule,TypedFunction,original::RangeBinding){
        let module=fixture::module();let mut f=module.functions[0].clone();let span=f.span;
        f.environment=None;f.parameters.clear();f.locals.truncate(1);f.locals[0].id=LocalId(0);f.locals[0].ty=Type::I64;f.locals[0].mutable=true;f.locals[0].span=span;
        let mut item=f.locals[0].clone();item.id=LocalId(1);item.name="index".into();item.mutable=false;f.locals.push(item);
        f.values=vec![Type::I64,Type::I64,Type::I64,Type::Bool,Type::I64,Type::I64,Type::I64];f.return_type=Type::Unit;
        f.blocks=vec![
            BasicBlock{id:BlockId(0),instructions:vec![
                fixture::instruction(Some(0),InstructionKind::Constant(Constant::I64(0))),
                fixture::instruction(Some(1),InstructionKind::Constant(Constant::I64(4))),
                fixture::instruction(None,InstructionKind::StorageLive(LocalId(0))),
                fixture::instruction(None,InstructionKind::StorageLive(LocalId(1))),
                fixture::instruction(None,InstructionKind::Store{local:LocalId(0),value:ValueId(0)}),
            ],terminator:Terminator::Jump(BlockId(1))},
            BasicBlock{id:BlockId(1),instructions:vec![
                fixture::instruction(Some(2),InstructionKind::Load(LocalId(0))),
                fixture::instruction(Some(3),InstructionKind::Binary{op:BinaryOp::Less,left:ValueId(2),right:ValueId(1)}),
            ],terminator:Terminator::Branch{condition:ValueId(3),then_block:BlockId(2),else_block:BlockId(4)}},
            BasicBlock{id:BlockId(2),instructions:vec![
                fixture::instruction(None,InstructionKind::Store{local:LocalId(1),value:ValueId(2)}),
                fixture::instruction(Some(4),InstructionKind::Load(LocalId(1))),
            ],terminator:Terminator::Jump(BlockId(3))},
            BasicBlock{id:BlockId(3),instructions:vec![
                fixture::instruction(Some(5),InstructionKind::Constant(Constant::I64(1))),
                fixture::instruction(Some(6),InstructionKind::Binary{op:BinaryOp::Add,left:ValueId(2),right:ValueId(5)}),
                fixture::instruction(None,InstructionKind::Store{local:LocalId(0),value:ValueId(6)}),
            ],terminator:Terminator::Jump(BlockId(1))},
            BasicBlock{id:BlockId(4),instructions:vec![
                fixture::instruction(None,InstructionKind::StorageDead(LocalId(1))),
                fixture::instruction(None,InstructionKind::StorageDead(LocalId(0))),
            ],terminator:Terminator::Return(None)},
        ];
        let range=original::RangeBinding{statement:NodeId(7),name:"index".into(),span,start:NodeId(1),end:NodeId(2)};
        (module,f,range)
    }
    #[test]
    fn range_source_uses_real_snapshot_guard_successor_and_storage_epoch(){
        let(module,f,range)=loop_body();let limits=Limits::default();
        assert_eq!(check(&sites::Producers::default(),&module,&f,LocalId(1),&range,&mut Budget::new(&limits)).unwrap(),(ValueId(0),ValueId(1)));
        for attack in 0..11{
            let mut f=f.clone();
            match attack{
                0=>f.blocks[2].instructions[0].kind=InstructionKind::Store{local:LocalId(1),value:ValueId(1)},
                1=>f.blocks[1].instructions[1].kind=InstructionKind::Binary{op:BinaryOp::LessEqual,left:ValueId(2),right:ValueId(1)},
                2=>f.blocks[3].instructions[0].kind=InstructionKind::Constant(Constant::I64(2)),
                3=>f.blocks[3].instructions[1].kind=InstructionKind::Binary{op:BinaryOp::Add,left:ValueId(1),right:ValueId(5)},
                4=>f.blocks[2].instructions.push(fixture::instruction(None,InstructionKind::Store{local:LocalId(0),value:ValueId(1)})),
                5|6=>f.blocks.push(BasicBlock{id:BlockId(5),instructions:if attack==5{vec![fixture::instruction(None,InstructionKind::BorrowLocal{local:LocalId(0)})]}else{vec![]},terminator:if attack==5{Terminator::Return(None)}else{Terminator::Jump(BlockId(2))}}),
                7=>f.blocks[4].instructions.insert(0,fixture::instruction(None,InstructionKind::Load(LocalId(1)))),
                8=>f.blocks[3].terminator=Terminator::Jump(BlockId(4)),
                9=>f.locals[1].mutable=true,
                _=>{let mut duplicate=f.locals[1].clone();duplicate.id=LocalId(2);f.locals.push(duplicate);},
            }
            assert!(check(&sites::Producers::default(),&module,&f,LocalId(1),&range,&mut Budget::new(&limits)).is_err(),"attack {attack}");
        }
        // Equal carrier types do not identify either endpoint. The enclosing
        // source proof must compare these actual operands with the two exact
        // original endpoint occurrences.
        let mut changed=f;changed.blocks[0].instructions[4].kind=InstructionKind::Store{local:LocalId(0),value:ValueId(1)};
        assert_eq!(check(&sites::Producers::default(),&module,&changed,LocalId(1),&range,&mut Budget::new(&limits)).unwrap(),(ValueId(1),ValueId(1)));
    }
}

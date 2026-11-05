// Neutral untrusted CFG models shared by independent resolver tests.
use zil_base::Span;
use zil_ir::*;
fn op(result: Option<ValueId>, kind: InstructionKind) -> Instruction { Instruction { result, kind, span: Span::default() } }
pub fn diamond(f: &mut TypedFunction, drop_branch: bool) {
    let call = f.blocks[0].instructions.iter().find(|i| matches!(i.kind,InstructionKind::CallClosureOnce{..})).unwrap();
    let InstructionKind::CallClosureOnce{callee,..} = call.kind else { unreachable!() };
    let take_at = f.blocks[0].instructions.iter().position(|i| i.result == Some(callee)).unwrap();
    let InstructionKind::TakeLocal{local:stage} = f.blocks[0].instructions[take_at].kind else { unreachable!() };
    let mut split_at = take_at;
    if !f.blocks[0].instructions.iter().any(|i| matches!(i.kind, InstructionKind::StorageLive(local) if local == stage)) {
        let at = f.blocks[0].instructions.iter().position(|i| matches!(i.kind,InstructionKind::Store{local,..} if local == stage)).unwrap();
        f.blocks[0].instructions.insert(at,op(None,InstructionKind::StorageLive(stage)));
        split_at += 1;
    }
    let suffix = f.blocks[0].instructions.split_off(split_at);
    let exit = f.blocks[0].terminator.clone();
    let flag = ValueId(f.values.len() as u32); f.values.push(Type::Bool);
    f.blocks[0].instructions.push(op(Some(flag),InstructionKind::Constant(Constant::Bool(true))));
    f.blocks[0].terminator=Terminator::Branch{condition:flag,then_block:BlockId(1),else_block:BlockId(2)};
    if drop_branch {
        f.blocks.push(BasicBlock{id:BlockId(1),instructions:suffix,terminator:exit});
        let result=ValueId(f.values.len() as u32); f.values.push(f.result.clone());
        let constant = if f.result == Type::I64 { Constant::I64(0) } else { Constant::I32(0) };
        f.blocks.push(BasicBlock{id:BlockId(2),instructions:vec![op(None,InstructionKind::DropClosure{local:stage}),op(Some(result),InstructionKind::Constant(constant))],terminator:Terminator::Return(Some(result))});
    }else{
        for id in [1,2] {f.blocks.push(BasicBlock{id:BlockId(id),instructions:vec![],terminator:Terminator::Jump(BlockId(3))});}
        f.blocks.push(BasicBlock{id:BlockId(3),instructions:suffix,terminator:exit});
    }
}
pub fn hostile(original:&TypedFunction)->Vec<(&'static str,TypedFunction)>{
    let mut result=vec![];
    let call=original.blocks.iter().flat_map(|b|&b.instructions).find(|i|matches!(i.kind,InstructionKind::CallClosureOnce{..})).unwrap();
    let InstructionKind::CallClosureOnce{callee,..}=call.kind else{unreachable!()};
    let take=original.blocks.iter().flat_map(|b|&b.instructions).find(|i|i.result==Some(callee)).unwrap();
    let InstructionKind::TakeLocal{local:stage}=take.kind else{unreachable!()};
    let source=original.blocks[0].instructions.iter().find(|i|matches!(i.kind,InstructionKind::TakeLocal{local}if local==original.parameters[original.callable_contract.as_ref().unwrap().roles[0].parameter as usize])).unwrap().result.unwrap();
    for name in ["island","backedge","self-loop","duplicate-block","other-entry","one-path-drop-then-call","early-dead","reactivate","address-exposure","duplicate-ssa","after-call-ssa-use","same-type-other-parameter","branch-origin-escape","return-origin-escape","one-path-take-then-call", "missing-live", "late-live"]{
        let mut f=original.clone();
        match name{
            "missing-live" | "late-live" => {
                for block in &mut f.blocks { block.instructions.retain(|i| !matches!(i.kind,InstructionKind::StorageLive(local) if local == stage)); }
                if name == "late-live" { let at=f.blocks[3].instructions.iter().position(|i| matches!(i.kind,InstructionKind::TakeLocal{local} if local == stage)).unwrap(); f.blocks[3].instructions.insert(at,op(None,InstructionKind::StorageLive(stage))); }
            },
            "island"=>f.blocks.push(BasicBlock{id:BlockId(4),instructions:vec![],terminator:Terminator::Trap}),
            "backedge"=>f.blocks[1].terminator=Terminator::Jump(BlockId(0)),
            "self-loop"=>f.blocks[1].terminator=Terminator::Jump(BlockId(1)),
            "duplicate-block"=>f.blocks[2].id=BlockId(1),
            "other-entry"=>f.entry=BlockId(1),
            "one-path-drop-then-call"=>f.blocks[1].instructions.push(op(None,InstructionKind::DropClosure{local:stage})),
            "early-dead"=>f.blocks[1].instructions.push(op(None,InstructionKind::StorageDead(stage))),
            "reactivate"=>{f.blocks[1].instructions.push(op(None,InstructionKind::StorageLive(stage)));f.blocks[1].instructions.push(op(None,InstructionKind::StorageLive(stage)));},
            "address-exposure"=>f.blocks[2].instructions.push(op(None,InstructionKind::BorrowLocalMut{local:stage})),
            "duplicate-ssa"=>f.blocks[2].instructions.push(op(Some(source),InstructionKind::Constant(Constant::I32(0)))),
            "after-call-ssa-use"=>f.blocks[3].instructions.push(op(None,InstructionKind::Store{local:stage,value:callee})),
            "same-type-other-parameter"=>{let local=LocalId(f.locals.len() as u32);let mut decl=f.locals[f.parameters[f.callable_contract.as_ref().unwrap().roles[0].parameter as usize].0 as usize].clone();decl.id=local;f.locals.push(decl);f.parameters.push(local);f.blocks[0].instructions.iter_mut().find(|i|i.result==Some(source)).unwrap().kind=InstructionKind::TakeLocal{local};},
            "branch-origin-escape"=>if let Terminator::Branch{condition,..}=&mut f.blocks[0].terminator{*condition=source;},
            "return-origin-escape"=>f.blocks[1].terminator=Terminator::Return(Some(source)),
            "one-path-take-then-call"=>{let value=ValueId(f.values.len() as u32);f.values.push(f.values[callee.0 as usize].clone());f.blocks[1].instructions.push(op(Some(value),InstructionKind::TakeLocal{local:stage}));},
            _=>unreachable!(),
        }
        result.push((name,f));
    }
    result
}

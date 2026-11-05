// Shared untrusted function mutations, not a proof or Source acceptance oracle.
use zil_ir::*;
fn op(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction { result, kind, span: Span::default() }
}
pub fn fork(f: &mut TypedFunction) {
    let exit = f.blocks[0].terminator.clone();
    let condition = ValueId(f.values.len() as u32);
    f.values.push(Type::Bool);
    f.blocks[0].instructions.push(op(Some(condition), InstructionKind::Constant(Constant::Bool(true))));
    f.blocks[0].terminator = Terminator::Branch { condition, then_block: BlockId(1), else_block: BlockId(2) };
    for id in [1, 2] {
        f.blocks.push(BasicBlock { id: BlockId(id), instructions: vec![], terminator: exit.clone() });
    }
}
pub fn hostile(original: &TypedFunction) -> Vec<(&'static str, TypedFunction)> {
    let mut out = vec![];
    let call = original.blocks[0].instructions.iter().find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. })).unwrap().clone();
    let InstructionKind::CallClosureOnce { callee, .. } = call.kind else { unreachable!() };
    let take = original.blocks[0].instructions.iter().find(|i| i.result == Some(callee)).unwrap().clone();
    let InstructionKind::TakeLocal { local: stage } = take.kind else { unreachable!() };
    let store = original.blocks[0].instructions.iter().find(|i| matches!(i.kind, InstructionKind::Store {local,..} if local == stage)).unwrap().clone();
    let InstructionKind::Store { value: source, .. } = store.kind else { unreachable!() };
    for name in ["unreachable", "backedge", "duplicate-block", "entry-edge", "call-in-leaf", "duplicate-call", "duplicate-ssa", "return-ssa", "branch-ssa", "leaf-store", "leaf-take", "leaf-load", "leaf-borrow", "leaf-live", "early-drop", "early-dead", "repeated-live", "ssa-extra-use", "same-type-parameter"] {
        let mut f = original.clone();
        let index = f.blocks[0].instructions.iter().position(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. })).unwrap();
        match name {
            "unreachable" => f.blocks.push(BasicBlock { id: BlockId(3), instructions: vec![], terminator: Terminator::Trap }),
            "backedge" => f.blocks[1].terminator = Terminator::Jump(f.entry),
            "duplicate-block" => f.blocks[2].id = f.blocks[1].id,
            "entry-edge" => if let Terminator::Branch { then_block, .. } = &mut f.blocks[0].terminator { *then_block = f.entry; },
            "call-in-leaf" => { let i = f.blocks[0].instructions.remove(index); f.blocks[1].instructions.push(i); },
            "duplicate-call" => f.blocks[1].instructions.push(call.clone()),
            "duplicate-ssa" => f.blocks[1].instructions.push(op(Some(source), InstructionKind::Constant(Constant::I32(0)))),
            "return-ssa" => f.blocks[1].terminator = Terminator::Return(Some(source)),
            "branch-ssa" => if let Terminator::Branch { condition, .. } = &mut f.blocks[0].terminator { *condition = source; },
            "leaf-store" => f.blocks[1].instructions.push(store.clone()),
            "leaf-take" => f.blocks[1].instructions.push(take.clone()),
            "leaf-load" => f.blocks[1].instructions.push(op(None, InstructionKind::Load(stage))),
            "leaf-borrow" => f.blocks[1].instructions.push(op(None, InstructionKind::BorrowLocalMut { local: stage })),
            "leaf-live" => f.blocks[1].instructions.push(op(None, InstructionKind::StorageLive(stage))),
            "early-drop" => f.blocks[0].instructions.insert(0, op(None, InstructionKind::DropClosure { local: stage })),
            "early-dead" => f.blocks[0].instructions.insert(0, op(None, InstructionKind::StorageDead(stage))),
            "repeated-live" => { f.blocks[0].instructions.insert(0, op(None, InstructionKind::StorageLive(stage))); f.blocks[0].instructions.insert(0, op(None, InstructionKind::StorageLive(stage))); },
            "ssa-extra-use" => f.blocks[0].instructions.push(store.clone()),
            "same-type-parameter" => {
                let local = LocalId(f.locals.len() as u32);
                let mut decl = f.locals[f.parameters[0].0 as usize].clone(); decl.id = local;
                f.locals.push(decl); f.parameters.push(local);
                let definition = f.blocks[0].instructions.iter_mut().find(|i| i.result == Some(source)).unwrap();
                definition.kind = InstructionKind::TakeLocal { local };
            },
            _ => unreachable!(),
        }
        out.push((name, f));
    }
    out
}

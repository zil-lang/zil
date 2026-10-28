fn finite_contract_cannot_hide_a_reachable_cycle() {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 0, owned());
    let f = &mut raw.module.functions[0];
    f.values.push(Type::Bool);
    f.blocks[0].instructions.push(op(
        Some(ValueId(2)),
        InstructionKind::Constant(Constant::Bool(true)),
    ));
    f.blocks[0].terminator = Terminator::Jump(BlockId(1));
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![],
        terminator: Terminator::Branch {
            condition: ValueId(2),
            then_block: BlockId(1),
            else_block: BlockId(2),
        },
    });
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: vec![],
        terminator: Terminator::Return(Some(ValueId(0))),
    });
    raw.reject("acyclic finite call-trace");
}
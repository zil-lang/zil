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
    // 190 independently reconstructs Flow; a Finite claim cannot hide this cycle.
    for result in raw.both() {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == zil_base::Phase::Verify
                && d.category == zil_base::ResultCategory::SafeReject
                && d.code == "E-VERIFY-CALLABLE-CONTRACT"
                && d.message
                    .contains("traces differ from the independent contract")),
            "{errors:#?}"
        );
    }
}
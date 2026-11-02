fn review254_slot_through_reused_local(replace: bool) -> Raw {
    let mut raw = dispatch();
    let mut keep = Body::new(0);
    keep.f.return_type = env();
    let f = keep.local(env(), true);
    let value = keep.value(env(), InstructionKind::TakeLocal { local: f }, vec![]);
    keep.f.callable_contract = Some(CallableParameterContract { roles: vec![CallableRole { parameter: 0, closure: ClosureId(0), kind: CallableKind::FnOnce, hold: vec![], call: vec![], writes: vec![], cleanup: CallableCleanup::Returned }], traces: CallableTraces::Finite(vec![CallableTrace { calls: vec![], exit: CallableExit::Return }]) });
    keep.f.callable_return_contract = Some(CallableReturnContract { source: CallableReturnSource::Parameter(0), lending_captures: vec![], closure: ClosureId(0), kind: CallableKind::FnOnce, hold: vec![], call: vec![], writes: vec![], cleanup: vec![] });
    raw.module.functions[0] = finish_scoped(keep, value).f;
    let mut caller = Body::new(2);
    let slot_type = Type::Array { element: Box::new(env()), length: 1 };
    let slot = caller.local(slot_type.clone(), false);
    let bridge = caller.local(env(), false);
    let original = caller.value(env(), InstructionKind::MakeClosure { closure: ClosureId(0), captures: vec![] }, vec![]);
    let array = caller.value(slot_type, InstructionKind::MakeOwnedArray { elements: vec![original] }, vec![]);
    caller.operation(InstructionKind::Store { local: slot, value: array });
    let taken = caller.value(env(), InstructionKind::TakeOwnedArrayElement { local: slot, index: 0 }, vec![]);
    caller.operation(InstructionKind::Store { local: bridge, value: taken });
    if replace {
        caller.operation(InstructionKind::DropClosure { local: bridge });
        let other = caller.value(env(), InstructionKind::MakeClosure { closure: ClosureId(0), captures: vec![] }, vec![]);
        caller.operation(InstructionKind::Store { local: bridge, value: other });
    }
    let actual = caller.value(env(), InstructionKind::TakeLocal { local: bridge }, vec![]);
    let result = caller.value(env(), InstructionKind::Call { function: FunctionId(0), arguments: vec![actual] }, vec![]);
    caller.operation(InstructionKind::Store { local: bridge, value: result });
    caller.operation(InstructionKind::DropClosure { local: bridge });
    caller.operation(InstructionKind::DropOwnedArray { local: slot });
    let zero = caller.number(0);
    raw.module.functions[2] = finish_scoped(caller, zero).f;
    raw
}

#[test]
fn review254_slot_permission_cannot_follow_overwritten_local() {
    review254_slot_through_reused_local(false).accept();
    let raw = review254_slot_through_reused_local(true);
    let bytes = zil_ir::wire::encode_module_candidate(&raw.module, &raw.plan(), &Limits::default()).unwrap();
    std::fs::write("/workspace/.zil-env/forward-review-254/R1-overwritten-local.wire", &bytes).unwrap();
    let mut accepted = vec![];
    for (wire, result) in raw.both().into_iter().enumerate() {
        match result {
            Ok(_) => { println!("review254 overwritten bridge wire={wire}: UNEXPECTED ACCEPT"); accepted.push(wire); },
            Err(errors) => println!("review254 overwritten bridge wire={wire}: {errors:?}"),
        }
    }
    assert!(accepted.is_empty(), "review254 slot qualification followed overwritten bridge local; accepted={accepted:?}");
}

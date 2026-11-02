//! Schema 31 raw and wire probes: a ClosureId is not an instance source.
use super::*;
use zil_ir::CallableReturnSource;

fn keep(actual: CallableKind, declared: CallableKind) -> Raw {
    let mut raw = fixture(actual, declared, false, 0, Type::I32);
    let function = &mut raw.module.functions[0];
    function.return_type = env();
    function.values = vec![env()];
    function.blocks[0].instructions = vec![op(
        Some(ValueId(0)),
        InstructionKind::TakeLocal { local: LocalId(0) },
    )];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    function.callable_contract.as_mut().unwrap().roles[0].cleanup = CallableCleanup::Returned;
    function.callable_return_contract = Some(CallableReturnContract {
        source: CallableReturnSource::Parameter(0),
        lending_captures: vec![],
        closure: ClosureId(0),
        kind: declared,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
    });
    raw
}

#[test]
fn raw_wire_forwarding_keeps_declared_modes_and_actual_instance() {
    for (actual, declared) in [
        (CallableKind::Fn, CallableKind::Fn),
        (CallableKind::FnMut, CallableKind::FnMut),
        (CallableKind::FnOnce, CallableKind::FnOnce),
        (CallableKind::Fn, CallableKind::FnOnce),
    ] {
        keep(actual, declared).accept();
    }
}

#[test]
fn same_type_is_not_the_input_instance() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    // A second value of the same concrete environment type cannot stand in
    // for this invocation's actual parameter, even if it is a valid closure.
    let function = &mut raw.module.functions[0];
    function.values = vec![Type::I32, env()];
    function.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::Constant(Constant::I32(7)),
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![ValueId(0)],
            },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    raw.reject("instance");
}

#[test]
fn forged_index_cleanup_and_permission_are_rejected() {
    let mut wrong = keep(CallableKind::Fn, CallableKind::FnOnce);
    wrong.module.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .source = CallableReturnSource::Parameter(1);
    wrong.reject("parameter");

    let mut cleanup = keep(CallableKind::Fn, CallableKind::FnOnce);
    cleanup.module.functions[0]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[0]
        .cleanup = CallableCleanup::Owned;
    cleanup.reject("envelope");

    let mut upgrade = keep(CallableKind::Fn, CallableKind::FnOnce);
    upgrade.module.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .kind = CallableKind::Fn;
    upgrade.reject("mode");
}

#[test]
fn fnmut_call_preserves_instance_but_fnonce_call_consumes_it() {
    let mut permitted = keep(CallableKind::FnMut, CallableKind::FnMut);
    let function = &mut permitted.module.functions[0];
    function.locals[0].mutable = true;
    function.values = vec![Type::I32, env()];
    function.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::CallClosureMut {
                callee: LocalId(0),
                arguments: vec![],
            },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    function.callable_contract.as_mut().unwrap().traces =
        CallableTraces::Finite(vec![CallableTrace {
            calls: vec![CallEvent {
                parameter: 0,
                kind: CallableKind::FnMut,
            }],
            exit: CallableExit::Return,
        }]);
    permitted.accept();

    let mut consumed = keep(CallableKind::FnOnce, CallableKind::FnOnce);
    let function = &mut consumed.module.functions[0];
    function.values = vec![env(), Type::I32, env()];
    function.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::CallClosureOnce {
                callee: ValueId(0),
                arguments: vec![],
            },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    consumed.reject("initialization");
}

#[test]
fn nested_keep_uses_actual_argument_and_does_not_upgrade_bound() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let mut child = raw.module.functions[0].clone();
    child.id = FunctionId(2);
    child.name = "child_keep".into();
    raw.module.functions.push(child);
    let parent = &mut raw.module.functions[0];
    parent.values = vec![env(), env()];
    parent.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![ValueId(0)],
            },
        ),
    ];
    parent.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    raw.accept();

    let mut upgraded = raw.clone();
    upgraded.module.functions[2]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[0]
        .kind = CallableKind::Fn;
    upgraded.module.functions[2]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .kind = CallableKind::Fn;
    upgraded.reject("bound");
}

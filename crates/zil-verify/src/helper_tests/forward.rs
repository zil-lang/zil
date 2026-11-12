//! Schema 31 raw and wire probes: a ClosureId is not an instance source.
use super::*;
use zil_base::ResultCategory;
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
        capture_sources: Some(Vec::new()),
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
fn owned_copy_fn_observation_counts_aliases_without_merging_local_instances() {
    for parameter_alias in [false, true] {
        let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, false, 0, Type::I32);
        let f = &mut raw.module.functions[0];
        // This is the ordinary Fn/Owned observation boundary, not an identity
        // return. Both snapshots retain their actual source despite one ID.
        assert!(f.callable_return_contract.is_none());
        f.locals.push(LocalDecl {
            id: LocalId(1),
            name: "observed".into(),
            ty: env(),
            mutable: true,
            span: span(),
        });
        f.values = vec![
            Type::I32,
            env(),
            env(),
            Type::I32,
            env(),
            Type::I32,
            Type::I32,
        ];
        f.blocks[0].instructions = vec![
            op(None, InstructionKind::StorageLive(LocalId(1))),
            op(
                Some(ValueId(0)),
                InstructionKind::Constant(Constant::I32(5)),
            ),
            op(Some(ValueId(1)), InstructionKind::Load(LocalId(0))),
            op(
                Some(ValueId(2)),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![ValueId(0)],
                },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(if parameter_alias { 1 } else { 2 }),
                },
            ),
            op(
                Some(ValueId(3)),
                InstructionKind::CallClosure {
                    callee: ValueId(1),
                    arguments: vec![],
                },
            ),
            op(Some(ValueId(4)), InstructionKind::Load(LocalId(1))),
            op(
                Some(ValueId(5)),
                InstructionKind::CallClosure {
                    callee: ValueId(4),
                    arguments: vec![],
                },
            ),
            op(
                Some(ValueId(6)),
                InstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(3),
                    right: ValueId(5),
                },
            ),
            op(None, InstructionKind::StorageDead(LocalId(1))),
        ];
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(6)));
        f.callable_contract.as_mut().unwrap().traces =
            CallableTraces::Finite(vec![CallableTrace {
                calls: vec![
                    CallEvent {
                        parameter: 0,
                        kind: CallableKind::Fn
                    };
                    if parameter_alias { 2 } else { 1 }
                ],
                exit: CallableExit::Return,
            }]);
        raw.accept();
        let mut forged = raw;
        forged.module.functions[0]
            .callable_contract
            .as_mut()
            .unwrap()
            .traces
            .as_finite_mut()
            .unwrap()[0]
            .calls = vec![
            CallEvent {
                parameter: 0,
                kind: CallableKind::Fn
            };
            if parameter_alias { 1 } else { 2 }
        ];
        forged.reject("traces differ");
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
fn raw_companion_cannot_hide_f_or_an_external_address() {
    for ty in [
        Type::Package {
            name: "Hidden".into(),
            fields: vec![("callback".into(), env())],
        },
        address(Type::I32, false),
    ] {
        let mut raw = keep(CallableKind::Fn, CallableKind::Fn);
        let function = &mut raw.module.functions[0];
        function.locals.push(LocalDecl {
            id: LocalId(1),
            name: "data".into(),
            ty,
            mutable: false,
            span: span(),
        });
        function.parameters.push(LocalId(1));
        raw.reject("E-UNSUPPORTED");
    }
}

#[test]
fn symbolic_f_cannot_enter_state_and_also_return_the_same_ssa() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let token = owned();
    let state = Type::Package {
        name: "State".into(),
        fields: vec![("token".into(), token.clone()), ("callback".into(), env())],
    };
    let function = &mut raw.module.functions[0];
    function.values = vec![env(), Type::I32, token, state];
    function.blocks[0].instructions = vec![
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::Constant(Constant::I32(3)),
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(1)],
            },
        ),
        op(
            Some(ValueId(3)),
            InstructionKind::MakePackage {
                fields: vec![ValueId(2), ValueId(0)],
            },
        ),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    raw.reject("unsupported value operation");
}

#[test]
fn symbolic_f_cannot_enter_general_array_and_also_return_the_same_ssa() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let function = &mut raw.module.functions[0];
    function.values = vec![
        env(),
        Type::I32,
        env(),
        Type::Array {
            element: Box::new(env()),
            length: 2,
        },
    ];
    function.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::Constant(Constant::I32(3)),
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![ValueId(1)],
            },
        ),
        op(
            Some(ValueId(3)),
            InstructionKind::MakeOwnedArray {
                elements: vec![ValueId(0), ValueId(2)],
            },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    raw.reject("existing internal single-element owning slot");
}

#[test]
fn symbolic_f_cannot_enter_state_field_and_also_return_the_same_ssa() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let state = Type::Package {
        name: "State".into(),
        fields: vec![("token".into(), owned()), ("callback".into(), env())],
    };
    let function = &mut raw.module.functions[0];
    function.locals.push(LocalDecl {
        id: LocalId(1),
        name: "state".into(),
        ty: state.clone(),
        mutable: true,
        span: span(),
    });
    function.values = vec![Type::I32, owned(), Type::I32, env(), state, env()];
    function.blocks[0].instructions = vec![
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        op(None, InstructionKind::StorageLive(LocalId(1))),
        op(
            Some(ValueId(0)),
            InstructionKind::Constant(Constant::I32(3)),
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::Constant(Constant::I32(7)),
        ),
        op(
            Some(ValueId(3)),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![ValueId(2)],
            },
        ),
        op(
            Some(ValueId(4)),
            InstructionKind::MakePackage {
                fields: vec![ValueId(1), ValueId(3)],
            },
        ),
        op(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(4),
            },
        ),
        op(
            None,
            InstructionKind::DropPackageField {
                local: LocalId(1),
                index: 1,
            },
        ),
        op(
            Some(ValueId(5)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            None,
            InstructionKind::StorePackageField {
                local: LocalId(1),
                index: 1,
                value: ValueId(5),
            },
        ),
        op(None, InstructionKind::DropPackage { local: LocalId(1) }),
        op(None, InstructionKind::StorageDead(LocalId(1))),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(5)));
    raw.reject("unsupported value operation");
}

fn slot_type() -> Type {
    Type::Array {
        element: Box::new(env()),
        length: 1,
    }
}

fn slot_fixture() -> Raw {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let mut child = raw.module.functions[0].clone();
    child.id = FunctionId(2);
    child.name = "nested_keep".into();
    raw.module.functions.push(child);
    let function = &mut raw.module.functions[0];
    function.locals.push(LocalDecl {
        id: LocalId(1),
        name: "slot".into(),
        ty: slot_type(),
        mutable: true,
        span: span(),
    });
    function.values = vec![env(), slot_type(), env(), env(), env()];
    function.blocks[0].instructions = vec![
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        op(None, InstructionKind::StorageLive(LocalId(1))),
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::MakeOwnedArray {
                elements: vec![ValueId(0)],
            },
        ),
        op(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(1),
            },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::TakeOwnedArrayElement {
                local: LocalId(1),
                index: 0,
            },
        ),
        op(
            Some(ValueId(3)),
            InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![ValueId(2)],
            },
        ),
        op(
            None,
            InstructionKind::DropOwnedArrayElement {
                local: LocalId(1),
                index: 0,
            },
        ),
        op(
            None,
            InstructionKind::StoreOwnedArrayElement {
                local: LocalId(1),
                index: 0,
                value: ValueId(3),
            },
        ),
        op(
            Some(ValueId(4)),
            InstructionKind::TakeOwnedArrayElement {
                local: LocalId(1),
                index: 0,
            },
        ),
        op(None, InstructionKind::DropOwnedArray { local: LocalId(1) }),
        op(None, InstructionKind::StorageDead(LocalId(1))),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
    raw
}

#[test]
fn internal_single_slot_keeps_exact_instance_across_nested_forwarding() {
    let raw = slot_fixture();
    raw.accept();
}

#[test]
fn internal_slot_rejects_fresh_substitution_and_repeated_take() {
    let mut fresh = slot_fixture();
    let function = &mut fresh.module.functions[0];
    function.values.extend([Type::I32, env()]);
    let refill = function.blocks[0]
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::StoreOwnedArrayElement { .. }
            )
        })
        .unwrap();
    function.blocks[0].instructions.splice(
        refill..refill,
        [
            op(
                Some(ValueId(5)),
                InstructionKind::Constant(Constant::I32(9)),
            ),
            op(
                Some(ValueId(6)),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![ValueId(5)],
                },
            ),
        ],
    );
    let InstructionKind::StoreOwnedArrayElement { value, .. } =
        &mut function.blocks[0].instructions[refill + 2].kind
    else {
        unreachable!()
    };
    *value = ValueId(6);
    fresh.reject("instance");

    let mut repeated = slot_fixture();
    let function = &mut repeated.module.functions[0];
    function.values.push(env());
    let first_take = function.blocks[0]
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::TakeOwnedArrayElement { .. }
            )
        })
        .unwrap();
    function.blocks[0].instructions.insert(
        first_take + 1,
        op(
            Some(ValueId(5)),
            InstructionKind::TakeOwnedArrayElement {
                local: LocalId(1),
                index: 0,
            },
        ),
    );
    repeated.reject("absent");

    let mut wrong_index = slot_fixture();
    let function = &mut wrong_index.module.functions[0];
    let InstructionKind::TakeOwnedArrayElement { index, .. } =
        &mut function.blocks[0].instructions[first_take].kind
    else {
        unreachable!()
    };
    *index = 1;
    wrong_index.reject("index");

    let mut wrong_slot = slot_fixture();
    let function = &mut wrong_slot.module.functions[0];
    function.locals.push(LocalDecl {
        id: LocalId(2),
        name: "other_slot".into(),
        ty: slot_type(),
        mutable: true,
        span: span(),
    });
    function.values.extend([Type::I32, env(), slot_type()]);
    let first_store = function.blocks[0]
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::Store {
                    local: LocalId(1),
                    ..
                }
            )
        })
        .unwrap();
    function.blocks[0].instructions.splice(
        first_store + 1..first_store + 1,
        [
            op(None, InstructionKind::StorageLive(LocalId(2))),
            op(
                Some(ValueId(5)),
                InstructionKind::Constant(Constant::I32(4)),
            ),
            op(
                Some(ValueId(6)),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![ValueId(5)],
                },
            ),
            op(
                Some(ValueId(7)),
                InstructionKind::MakeOwnedArray {
                    elements: vec![ValueId(6)],
                },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(2),
                    value: ValueId(7),
                },
            ),
        ],
    );
    let last_take = function.blocks[0]
        .instructions
        .iter_mut()
        .rev()
        .find(|instruction| {
            matches!(
                instruction.kind,
                InstructionKind::TakeOwnedArrayElement { .. }
            )
        })
        .unwrap();
    let InstructionKind::TakeOwnedArrayElement { local, .. } = &mut last_take.kind else {
        unreachable!()
    };
    *local = LocalId(2);
    let drop_other = function.blocks[0]
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.kind, InstructionKind::StorageDead(LocalId(1)))
        })
        .unwrap();
    function.blocks[0].instructions.splice(
        drop_other..drop_other,
        [
            op(None, InstructionKind::DropOwnedArray { local: LocalId(2) }),
            op(None, InstructionKind::StorageDead(LocalId(2))),
        ],
    );
    wrong_slot.reject("instance");
}

#[test]
fn internal_slot_take_invalidates_its_prior_borrowed_callable_address() {
    let mut raw = keep(CallableKind::FnMut, CallableKind::FnMut);
    let function = &mut raw.module.functions[0];
    function.locals.push(LocalDecl {
        id: LocalId(1),
        name: "slot".into(),
        ty: slot_type(),
        mutable: true,
        span: span(),
    });
    function.values = vec![env(), slot_type(), address(env(), true), Type::I32, env()];
    function.blocks[0].instructions = vec![
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        op(None, InstructionKind::StorageLive(LocalId(1))),
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::MakeOwnedArray {
                elements: vec![ValueId(0)],
            },
        ),
        op(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(1),
            },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::BorrowOwnedArrayElement {
                local: LocalId(1),
                index: 0,
                mutable: true,
            },
        ),
        op(
            Some(ValueId(3)),
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(2),
                kind: CallableKind::FnMut,
                arguments: vec![],
            },
        ),
        op(
            Some(ValueId(4)),
            InstructionKind::TakeOwnedArrayElement {
                local: LocalId(1),
                index: 0,
            },
        ),
        op(None, InstructionKind::DropOwnedArray { local: LocalId(1) }),
        op(None, InstructionKind::StorageDead(LocalId(1))),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
    function.callable_contract.as_mut().unwrap().traces =
        CallableTraces::Finite(vec![CallableTrace {
            calls: vec![CallEvent {
                parameter: 0,
                kind: CallableKind::FnMut,
            }],
            exit: CallableExit::Return,
        }]);
    raw.holds.insert((0, 2), vec![HoldRoot::Local(LocalId(1))]);
    raw.accept();

    let mut stale = raw.clone();
    stale.module.functions[0].blocks[0].instructions.swap(6, 7);
    stale.reject("BORROW-CONFLICT");
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
fn moved_f_cannot_leave_a_borrow_of_its_old_position_callable() {
    let mut raw = keep(CallableKind::FnMut, CallableKind::FnMut);
    raw.module.resources.clear();
    let function = &mut raw.module.functions[0];
    function.locals[0].mutable = true;
    function.values = vec![address(env(), true), Type::I32, env()];
    function.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::BorrowLocalMut { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(0),
                kind: CallableKind::FnMut,
                arguments: vec![],
            },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    function.callable_contract.as_mut().unwrap().traces =
        CallableTraces::Finite(vec![CallableTrace {
            calls: vec![CallEvent {
                parameter: 0,
                kind: CallableKind::FnMut,
            }],
            exit: CallableExit::Return,
        }]);
    raw.holds.insert((0, 0), vec![HoldRoot::Local(LocalId(0))]);
    raw.accept();

    let mut stale = raw.clone();
    stale.module.functions[0].blocks[0].instructions.swap(1, 2);
    stale.reject("BORROW-CONFLICT");
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

#[test]
fn each_branch_returns_this_instances_parameter() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let function = &mut raw.module.functions[0];
    function.locals.push(LocalDecl {
        id: LocalId(1),
        name: "flag".into(),
        ty: Type::Bool,
        mutable: false,
        span: span(),
    });
    function.parameters.push(LocalId(1));
    function.values = vec![Type::Bool, env(), env()];
    function.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![op(Some(ValueId(0)), InstructionKind::Load(LocalId(1)))],
            terminator: Terminator::Branch {
                condition: ValueId(0),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![op(
                Some(ValueId(1)),
                InstructionKind::TakeLocal { local: LocalId(0) },
            )],
            terminator: Terminator::Return(Some(ValueId(1))),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![op(
                Some(ValueId(2)),
                InstructionKind::TakeLocal { local: LocalId(0) },
            )],
            terminator: Terminator::Return(Some(ValueId(2))),
        },
    ];
    raw.accept();

    let mut mixed = raw.clone();
    let function = &mut mixed.module.functions[0];
    function.values = vec![Type::Bool, env(), Type::I32, env()];
    function.blocks[2].instructions = vec![
        op(
            Some(ValueId(2)),
            InstructionKind::Constant(Constant::I32(9)),
        ),
        op(
            Some(ValueId(3)),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![ValueId(2)],
            },
        ),
    ];
    function.blocks[2].terminator = Terminator::Return(Some(ValueId(3)));
    mixed.reject("instance");
}

#[test]
fn owned_environment_returns_its_remaining_responsibility() {
    let mut raw = fixture(
        CallableKind::FnOnce,
        CallableKind::FnOnce,
        false,
        0,
        owned(),
    );
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
        capture_sources: Some(Vec::new()),
        source: CallableReturnSource::Parameter(0),
        lending_captures: vec![],
        closure: ClosureId(0),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![0],
    });
    raw.accept();

    let mut missing = raw.clone();
    missing.module.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .clear();
    missing.reject("cleanup");
}

#[test]
fn historical_loan_wire30_stays_rejected_without_reencoding() {
    let original = include_bytes!("../../../../tests/fixtures/wire/loan-revival-v30.wire");
    let errors =
        verify_module_bytes(original, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    assert!(format!("{errors:?}").contains("version"), "{errors:?}");
}

#[test]
fn forward_raw_wire_budget_has_exact_fresh_boundary() {
    let raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let plan = raw.plan();
    let bytes =
        zil_ir::wire::encode_module_candidate(&raw.module, &plan, &Limits::default()).unwrap();
    for wire in [false, true] {
        let run = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if wire {
                verify_module_bytes(&bytes, &raw.module.target, &limits)
            } else {
                verify_module(
                    raw.module.clone(),
                    plan.clone(),
                    &raw.module.target,
                    &limits,
                )
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap().fingerprint();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(verified) => {
                    assert_eq!(verified.fingerprint(), expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        println!("forward254 wire={wire} N={high}; N-1=ResourceLimit; fresh N=Pass");
    }
}

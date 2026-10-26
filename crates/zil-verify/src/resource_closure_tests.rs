//! Raw public-IR boundary probes, independent of Analyze and Types.
use super::*;
use zil_base::{FileId, ModuleId};

fn s() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn payload() -> Type {
    Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}
fn managed() -> Type {
    Type::Managed(Box::new(payload()))
}
fn desc() -> Type {
    Type::Desc(Box::new(payload()))
}
fn inst(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: s(),
    }
}
struct Body(TypedFunction);
impl Body {
    fn new(id: u32) -> Self {
        Self(TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(id),
            name: format!("f{id}"),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(None),
            }],
            entry: BlockId(0),
            span: s(),
        })
    }
    fn local(&mut self, ty: Type) -> LocalId {
        let id = LocalId(self.0.locals.len() as u32);
        self.0.locals.push(LocalDecl {
            id,
            name: format!("l{}", id.0),
            ty,
            mutable: true,
            span: s(),
        });
        self.op(InstructionKind::StorageLive(id));
        id
    }
    fn value(&mut self, ty: Type, kind: InstructionKind) -> ValueId {
        let id = ValueId(self.0.values.len() as u32);
        self.0.values.push(ty);
        self.0.blocks[0].instructions.push(inst(Some(id), kind));
        id
    }
    fn op(&mut self, kind: InstructionKind) {
        self.0.blocks[0].instructions.push(inst(None, kind));
    }
    fn constant(&mut self, n: i32) -> ValueId {
        self.value(Type::I32, InstructionKind::Constant(Constant::I32(n)))
    }
    fn finish(mut self, value: ValueId) -> TypedFunction {
        self.0.blocks[0].terminator = Terminator::Return(Some(value));
        self.0
    }
}
pub(super) fn fixture(kind: CallableKind, borrowed: bool, count: usize) -> TypedModule {
    let mut main = Body::new(0);
    let mut captures = vec![];
    let mut owners = vec![];
    for index in 0..count {
        let local = main.local(managed());
        owners.push(local);
        let value = main.constant(index as i32);
        let owner = main.value(
            managed(),
            InstructionKind::MakeManaged {
                fields: vec![value],
            },
        );
        main.op(InstructionKind::Store {
            local,
            value: owner,
        });
        captures.push(if borrowed {
            main.value(desc(), InstructionKind::DescribeLocal { local })
        } else {
            main.value(managed(), InstructionKind::TakeLocal { local })
        });
    }
    let environment = main.local(Type::Closure(ClosureId(0)));
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures,
        },
    );
    main.op(InstructionKind::Store {
        local: environment,
        value: closure,
    });
    let result = match kind {
        CallableKind::Fn => main.value(
            Type::I32,
            InstructionKind::CallClosureShared {
                callee: environment,
                arguments: vec![],
            },
        ),
        CallableKind::FnMut => main.value(
            Type::I32,
            InstructionKind::CallClosureMut {
                callee: environment,
                arguments: vec![],
            },
        ),
        CallableKind::FnOnce => {
            let value = main.value(
                Type::Closure(ClosureId(0)),
                InstructionKind::TakeLocal { local: environment },
            );
            main.value(
                Type::I32,
                InstructionKind::CallClosureOnce {
                    callee: value,
                    arguments: vec![],
                },
            )
        }
        _ => unreachable!(),
    };
    if !borrowed || kind == CallableKind::FnOnce {
        main.op(InstructionKind::DropClosure { local: environment });
    }
    main.op(InstructionKind::StorageDead(environment));
    for local in owners.into_iter().rev() {
        main.op(InstructionKind::DropManaged { local });
        main.op(InstructionKind::StorageDead(local));
    }
    let mut entry = Body::new(1);
    entry.0.environment = Some(ClosureId(0));
    let result_entry = if count == 0 {
        entry.constant(3)
    } else {
        let d = entry.value(
            desc(),
            if borrowed {
                InstructionKind::ReadCapture { index: 0 }
            } else {
                InstructionKind::DescribeCapture { index: 0 }
            },
        );
        entry.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 })
    };
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![main.finish(result), entry.finish(result_entry)],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(1),
            kind,
            captures: (0..count)
                .map(|_| if borrowed { desc() } else { managed() })
                .collect(),
            parameters: vec![],
            result: Type::I32,
        }],
        resources: vec![ResourceDefinition {
            name: "Token".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn check(module: TypedModule) -> CompileResult<VerifiedModule> {
    let plan = resource_tests::plan(&module);
    let target = module.target.clone();
    verify_module(module, plan, &target, &Limits::default())
}
fn reject_resource(module: TypedModule) {
    let errors = check(module).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == "E-VERIFY-RESOURCE"),
        "{errors:?}"
    );
}
fn append_entry(module: &mut TypedModule, kind: InstructionKind) {
    module.functions[1].blocks[0]
        .instructions
        .push(inst(None, kind));
}
fn add_helper(module: &mut TypedModule, write: bool) {
    let mut helper = Body::new(2);
    helper.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "arg".into(),
        ty: desc(),
        mutable: false,
        span: s(),
    });
    helper.0.parameters.push(LocalId(0));
    let d = helper.value(desc(), InstructionKind::Load(LocalId(0)));
    let n = helper.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    if write {
        helper.op(InstructionKind::WriteDesc {
            desc: d,
            index: 0,
            value: n,
        });
    }
    module.functions.push(helper.finish(n));
    module.functions[1].blocks[0].instructions[1].kind = InstructionKind::Call {
        function: FunctionId(2),
        arguments: vec![ValueId(0)],
    };
}

#[test]
fn owned_fn_repeated_shared_reads_and_uncalled_drop_are_valid() {
    let mut m = fixture(CallableKind::Fn, false, 1);
    let f = &mut m.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
        .unwrap();
    let mut repeat = f.blocks[0].instructions[at].clone();
    repeat.result = Some(ValueId(f.values.len() as u32));
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(at, repeat);
    check(m).unwrap();
    let mut m = fixture(CallableKind::Fn, false, 1);
    for i in &mut m.functions[0].blocks[0].instructions {
        if matches!(i.kind, InstructionKind::CallClosureShared { .. }) {
            i.kind = InstructionKind::Constant(Constant::I32(0));
        }
    }
    check(m).unwrap();
}
#[test]
fn owned_captures_cannot_be_copied_or_called_by_value() {
    let mut m = fixture(CallableKind::Fn, false, 1);
    m.functions[1].blocks[0].instructions[0].kind = InstructionKind::ReadCapture { index: 0 };
    m.functions[1].values[0] = managed();
    assert_eq!(check(m).unwrap_err()[0].code, "E-VERIFY-CAPTURE");
    let mut m = fixture(CallableKind::FnOnce, false, 1);
    let f = &mut m.functions[0];
    for i in &mut f.blocks[0].instructions {
        if matches!(i.kind, InstructionKind::TakeLocal { local: LocalId(1) }) {
            i.kind = InstructionKind::Load(LocalId(1));
        }
    }
    reject_resource(m);
    let mut m = fixture(CallableKind::Fn, false, 1);
    for i in &mut m.functions[0].blocks[0].instructions {
        if matches!(i.kind, InstructionKind::CallClosureShared { .. }) {
            i.kind = InstructionKind::CallClosure {
                callee: ValueId(3),
                arguments: vec![],
            };
        }
    }
    assert_eq!(check(m).unwrap_err()[0].code, "E-VERIFY-CLOSURE-CALL");
}
#[test]
fn owned_writes_require_mutable_entry_borrowed_target_writes_do_not() {
    for (kind, borrowed, accepted) in [
        (CallableKind::Fn, false, false),
        (CallableKind::FnMut, false, true),
        (CallableKind::Fn, true, true),
    ] {
        let mut m = fixture(kind, borrowed, 1);
        append_entry(
            &mut m,
            InstructionKind::WriteDesc {
                desc: ValueId(0),
                index: 0,
                value: ValueId(1),
            },
        );
        if accepted {
            check(m).unwrap();
        } else {
            reject_resource(m);
        }
    }
}
#[test]
fn direct_and_transitive_write_effects_are_reconstructed_from_callee_body() {
    for (write, kind, borrowed, accepted) in [
        (false, CallableKind::Fn, false, true),
        (true, CallableKind::Fn, false, false),
        (true, CallableKind::FnMut, false, true),
        (true, CallableKind::Fn, true, true),
    ] {
        let mut m = fixture(kind, borrowed, 1);
        add_helper(&mut m, write);
        if accepted {
            check(m.clone()).unwrap();
        } else {
            reject_resource(m.clone());
        }
        let mut forwarded = m.functions[2].clone();
        forwarded.id = FunctionId(3);
        forwarded.name = "forwarded".into();
        m.functions.push(forwarded);
        m.functions[2].blocks[0].instructions.truncate(2);
        m.functions[2].blocks[0].instructions[1].kind = InstructionKind::Call {
            function: FunctionId(3),
            arguments: vec![ValueId(0)],
        };
        if accepted {
            check(m).unwrap();
        } else {
            reject_resource(m);
        }
    }
}
#[test]
fn consume_can_give_one_field_and_settle_the_remaining_fields() {
    let mut m = fixture(CallableKind::FnOnce, false, 2);
    let mut entry = Body(m.functions[1].clone());
    let place = entry.local(managed());
    let moved = entry.value(managed(), InstructionKind::TakeCapture { index: 0 });
    entry.op(InstructionKind::Store {
        local: place,
        value: moved,
    });
    entry.op(InstructionKind::DropManaged { local: place });
    entry.op(InstructionKind::StorageDead(place));
    m.functions[1] = entry.0;
    check(m.clone()).unwrap();
    let v = ValueId(m.functions[1].values.len() as u32);
    m.functions[1].values.push(managed());
    m.functions[1].blocks[0]
        .instructions
        .push(inst(Some(v), InstructionKind::TakeCapture { index: 0 }));
    reject_resource(m);
}
#[test]
fn take_capture_requires_consuming_entry_and_cannot_lose_moved_ssa() {
    let mut m = fixture(CallableKind::Fn, false, 1);
    m.functions[1].values.push(managed());
    m.functions[1].blocks[0].instructions.push(inst(
        Some(ValueId(2)),
        InstructionKind::TakeCapture { index: 0 },
    ));
    assert_eq!(check(m).unwrap_err()[0].code, "E-VERIFY-CLOSURE-CALL");
    let mut m = fixture(CallableKind::FnOnce, false, 1);
    m.functions[1].values.push(managed());
    m.functions[1].blocks[0].instructions.push(inst(
        Some(ValueId(2)),
        InstructionKind::TakeCapture { index: 0 },
    ));
    reject_resource(m);
}
#[test]
fn closure_debts_cannot_be_lost_or_cleared_twice() {
    let mut m = fixture(CallableKind::Fn, false, 1);
    m.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropClosure { .. }));
    reject_resource(m);
    let mut m = fixture(CallableKind::Fn, false, 1);
    let f = &mut m.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropClosure { .. }))
        .unwrap();
    let duplicate = f.blocks[0].instructions[at].clone();
    f.blocks[0].instructions.insert(at, duplicate);
    reject_resource(m);
    let mut m = fixture(CallableKind::Fn, false, 1);
    let f = &mut m.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::Store {
                    local: LocalId(1),
                    ..
                }
            )
        })
        .unwrap();
    let value = match f.blocks[0].instructions[at].kind {
        InstructionKind::Store { value, .. } => value,
        _ => unreachable!(),
    };
    f.locals.push(LocalDecl {
        id: LocalId(2),
        name: "double".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: true,
        span: s(),
    });
    f.blocks[0].instructions.insert(
        at + 1,
        inst(
            None,
            InstructionKind::Store {
                local: LocalId(2),
                value,
            },
        ),
    );
    reject_resource(m);
}
#[test]
fn a_consuming_environment_ssa_is_not_reusable_even_without_owned_fields() {
    for count in [0, 1] {
        let mut m = fixture(CallableKind::FnOnce, false, count);
        check(m.clone()).unwrap();
        let f = &mut m.functions[0];
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
            .unwrap();
        let mut call = f.blocks[0].instructions[at].clone();
        call.result = Some(ValueId(f.values.len() as u32));
        f.values.push(Type::I32);
        f.blocks[0].instructions.insert(at + 1, call);
        reject_resource(m);
    }
}
#[test]
fn description_environment_cannot_outlive_or_follow_moved_target() {
    let mut m = fixture(CallableKind::Fn, true, 1);
    let f = &mut m.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
        .unwrap();
    f.blocks[0].instructions.insert(
        at,
        inst(None, InstructionKind::DropManaged { local: LocalId(0) }),
    );
    reject_resource(m);
    let mut m = fixture(CallableKind::FnOnce, false, 1);
    let f = &mut m.functions[1];
    f.values.push(managed());
    f.blocks[0].instructions.insert(
        1,
        inst(Some(ValueId(2)), InstructionKind::TakeCapture { index: 0 }),
    );
    reject_resource(m);
}
#[test]
fn one_conditional_consume_keeps_other_edge_cleanup_live() {
    let mut m = fixture(CallableKind::FnOnce, false, 1);
    let f = &mut m.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::TakeLocal { local: LocalId(1) }))
        .unwrap();
    let mut tail = f.blocks[0].instructions.split_off(at);
    let consuming: Vec<_> = tail.drain(..2).collect();
    let n = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    f.blocks[0]
        .instructions
        .push(inst(Some(n), InstructionKind::Constant(Constant::I32(0))));
    let condition = ValueId(f.values.len() as u32);
    f.values.push(Type::Bool);
    f.blocks[0].instructions.push(inst(
        Some(condition),
        InstructionKind::Constant(Constant::Bool(true)),
    ));
    f.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: consuming,
        terminator: Terminator::Jump(BlockId(3)),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: vec![],
        terminator: Terminator::Jump(BlockId(3)),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(3),
        instructions: tail,
        terminator: Terminator::Return(Some(n)),
    });
    check(m).unwrap();
}

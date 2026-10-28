//! Raw and wire boundary probes. These construct public candidates directly;
//! none calls the source type solver or Analyze, and none is native evidence.
use super::*;
use std::collections::BTreeMap;
use zil_base::{FileId, ModuleId};

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn owned() -> Type {
    Type::Managed(Box::new(payload()))
}
fn desc() -> Type {
    Type::Desc(Box::new(payload()))
}
fn payload() -> Type {
    Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}
fn env() -> Type {
    Type::Closure(ClosureId(0))
}
fn address(ty: Type, mutable: bool) -> Type {
    Type::Address {
        mutable,
        inner: Box::new(ty),
    }
}
fn op(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: span(),
    }
}

struct Body {
    f: TypedFunction,
    holds: Vec<Vec<HoldRoot>>,
}
impl Body {
    fn new(id: u32) -> Self {
        Self {
            f: TypedFunction {
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
                    terminator: Terminator::Trap,
                }],
                entry: BlockId(0),
                span: span(),
            },
            holds: vec![],
        }
    }
    fn local(&mut self, ty: Type, parameter: bool) -> LocalId {
        let id = LocalId(self.f.locals.len() as u32);
        self.f.locals.push(LocalDecl {
            id,
            name: format!("l{}", id.0),
            ty,
            mutable: !parameter,
            span: span(),
        });
        if parameter {
            self.f.parameters.push(id);
        } else {
            self.operation(InstructionKind::StorageLive(id));
        }
        id
    }
    fn value(&mut self, ty: Type, kind: InstructionKind, hold: Vec<HoldRoot>) -> ValueId {
        let id = ValueId(self.f.values.len() as u32);
        self.f.values.push(ty);
        self.holds.push(hold);
        self.f.blocks[0].instructions.push(op(Some(id), kind));
        id
    }
    fn operation(&mut self, kind: InstructionKind) {
        self.f.blocks[0].instructions.push(op(None, kind));
    }
    fn number(&mut self, n: i32) -> ValueId {
        self.value(
            Type::I32,
            InstructionKind::Constant(Constant::I32(n)),
            vec![],
        )
    }
    fn finish(mut self, value: ValueId) -> Self {
        self.f.blocks[0].terminator = Terminator::Return(Some(value));
        self
    }
}
#[derive(Clone)]
struct Raw {
    module: TypedModule,
    holds: BTreeMap<(u32, u32), Vec<HoldRoot>>,
}
impl Raw {
    fn push(&mut self, body: Body) {
        for (i, roots) in body.holds.into_iter().enumerate() {
            self.holds.insert((body.f.id.0, i as u32), roots);
        }
        assert_eq!(body.f.id.0 as usize, self.module.functions.len());
        self.module.functions.push(body.f);
    }
    fn plan(&self) -> CandidatePlan {
        let mut plan = resource_tests::plan(&self.module);
        for function in &mut plan.functions {
            for claim in &mut function.holds {
                if let Some(roots) = self.holds.get(&(function.function.0, claim.value.0)) {
                    claim.roots = roots.clone();
                }
            }
        }
        plan
    }
    fn check(&self) -> CompileResult<VerifiedModule> {
        verify_module(
            self.module.clone(),
            self.plan(),
            &self.module.target,
            &Limits::default(),
        )
    }
    fn both(&self) -> [CompileResult<VerifiedModule>; 2] {
        let bytes =
            zil_ir::wire::encode_module_candidate(&self.module, &self.plan(), &Limits::default())
                .unwrap();
        [
            self.check(),
            verify_module_bytes(&bytes, &self.module.target, &Limits::default()),
        ]
    }
    fn accept(&self) {
        for result in self.both() {
            result.unwrap();
        }
    }
    fn reject(&self, needle: &str) {
        for result in self.both() {
            let errors = result.unwrap_err();
            assert!(
                format!("{errors:?}").contains(needle),
                "wanted {needle}: {errors:?}"
            );
        }
    }
}

fn fixture(
    actual: CallableKind,
    requested: CallableKind,
    borrowed: bool,
    count: usize,
    capture: Type,
) -> Raw {
    let mut helper = Body::new(0);
    let parameter = helper.local(
        if borrowed {
            address(env(), requested == CallableKind::FnMut)
        } else {
            env()
        },
        true,
    );
    let mut result = helper.number(0);
    let load = if borrowed {
        Some(helper.value(
            address(env(), requested == CallableKind::FnMut),
            InstructionKind::Load(parameter),
            vec![HoldRoot::Parameter(0)],
        ))
    } else {
        None
    };
    for _ in 0..count {
        result = if let Some(callee) = load {
            helper.value(
                Type::I32,
                InstructionKind::CallClosureBorrowed {
                    callee,
                    kind: requested,
                    arguments: vec![],
                },
                vec![],
            )
        } else if requested == CallableKind::FnOnce {
            let callee = helper.value(
                env(),
                InstructionKind::TakeLocal { local: parameter },
                if matches!(capture, Type::Address { .. }) {
                    vec![HoldRoot::Parameter(0)]
                } else {
                    vec![]
                },
            );
            helper.value(
                Type::I32,
                InstructionKind::CallClosureOnce {
                    callee,
                    arguments: vec![],
                },
                vec![],
            )
        } else {
            helper.value(
                Type::I32,
                InstructionKind::CallClosureShared {
                    callee: parameter,
                    arguments: vec![],
                },
                vec![],
            )
        };
    }
    if !borrowed && (matches!(capture, Type::Managed(_)) || actual == CallableKind::FnOnce) {
        helper.operation(InstructionKind::DropClosure { local: parameter });
    }
    helper.f.callable_contract = Some(CallableParameterContract {
        parameter: 0,
        closure: ClosureId(0),
        kind: requested,
        hold: if matches!(capture, Type::Address { .. }) {
            vec![0]
        } else {
            vec![]
        },
        call: if matches!(capture, Type::Address { .. } | Type::Desc(_)) {
            vec![0]
        } else {
            vec![]
        },
        writes: if matches!(capture, Type::Address { mutable: true, .. } | Type::Desc(_)) {
            vec![0]
        } else {
            vec![]
        },
        cleanup: if borrowed {
            CallableCleanup::Borrowed
        } else {
            CallableCleanup::Owned
        },
        traces: zil_ir::CallableTraces::Finite(vec![CallableTrace {
            calls: vec![requested; count],
            exit: CallableExit::Return,
        }]),
    });
    let helper = helper.finish(result);
    let mut entry = Body::new(1);
    entry.f.environment = Some(ClosureId(0));
    let read = match &capture {
        Type::Managed(_) => {
            let d = entry.value(
                desc(),
                InstructionKind::DescribeCapture { index: 0 },
                vec![],
            );
            entry.value(
                Type::I32,
                InstructionKind::ReadDesc { desc: d, index: 0 },
                vec![],
            )
        }
        Type::Desc(_) => {
            let d = entry.value(desc(), InstructionKind::ReadCapture { index: 0 }, vec![]);
            entry.value(
                Type::I32,
                InstructionKind::ReadDesc { desc: d, index: 0 },
                vec![],
            )
        }
        Type::Address { .. } => {
            let a = entry.value(
                capture.clone(),
                InstructionKind::ReadCapture { index: 0 },
                vec![HoldRoot::Capture(0)],
            );
            entry.value(
                Type::I32,
                InstructionKind::ReadAddress { address: a },
                vec![],
            )
        }
        Type::I32 => entry.value(Type::I32, InstructionKind::ReadCapture { index: 0 }, vec![]),
        _ => unreachable!(),
    };
    let mut raw = Raw {
        module: TypedModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::of(b"independent raw helper fixtures"),
            target: TargetProfile::host_llvm(),
            functions: vec![],
            closures: vec![ClosureDefinition {
                lending_captures: Vec::new(),
                id: ClosureId(0),
                function: FunctionId(1),
                captures: vec![capture],
                parameters: vec![],
                result: Type::I32,
                kind: actual,
            }],
            resources: vec![ResourceDefinition {
                name: "Token".into(),
                payload: payload(),
                teardown: None,
            }],
            unsupported: vec![],
        },
        holds: BTreeMap::new(),
    };
    raw.push(helper);
    raw.push(entry.finish(read));
    raw
}

#[test]
fn borrowed_owned_fn_calls_twice_and_exports_checked_contract_raw_wire() {
    let raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 2, owned());
    raw.accept();
    let verified = raw.check().unwrap();
    assert_eq!(
        verified.callable_contract(FunctionId(0)),
        raw.module.functions[0].callable_contract.as_ref()
    );
    assert!(verified.callable_contract(FunctionId(1)).is_none());
    assert!(verified.callable_contract(FunctionId(99)).is_none());
}
#[test]
fn exclusive_borrow_calls_fnmut_and_actual_fn_adapter_without_mutable_binding() {
    for actual in [CallableKind::Fn, CallableKind::FnMut] {
        let raw = fixture(actual, CallableKind::FnMut, true, 2, owned());
        assert!(!raw.module.functions[0].locals[0].mutable);
        raw.accept();
    }
}
#[test]
fn owned_consume_adapters_call_once_or_drop_uncalled_environment() {
    for actual in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        for calls in [0, 1] {
            fixture(actual, CallableKind::FnOnce, false, calls, owned()).accept();
        }
    }
}
#[test]
fn owned_repeatable_fn_borrowed_calls_then_one_cleanup() {
    fixture(CallableKind::Fn, CallableKind::Fn, false, 2, owned()).accept();
}
#[test]
fn copy_environment_can_be_explicitly_moved_into_consume_adapter() {
    fixture(CallableKind::Fn, CallableKind::FnOnce, false, 1, Type::I32).accept();
}
#[test]
fn missing_contract_never_grants_helper_authority() {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 2, owned());
    raw.module.functions[0].callable_contract = None;
    raw.reject("E-VERIFY-CALLABLE-CONTRACT");
}
#[test]
fn forged_count_mode_parameter_identity_cleanup_and_duplicate_traces_fail() {
    for mutation in 0..7 {
        let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 2, owned());
        let c = raw.module.functions[0].callable_contract.as_mut().unwrap();
        match mutation {
            0 => {
                c.traces.as_finite_mut().unwrap()[0].calls.pop();
            }
            1 => c.kind = CallableKind::FnOnce,
            2 => c.parameter = 1,
            3 => c.closure = ClosureId(77),
            4 => c.cleanup = CallableCleanup::Owned,
            5 => {
                let duplicate = c.traces.as_finite().unwrap()[0].clone();
                c.traces.as_finite_mut().unwrap().push(duplicate);
            }
            6 => c.traces.as_finite_mut().unwrap()[0].exit = CallableExit::Trap,
            _ => unreachable!(),
        }
        raw.reject("E-VERIFY-CALLABLE-CONTRACT");
    }
}
#[test]
fn dropped_hold_call_or_write_bounds_are_rejected_independently() {
    for capture in [desc(), address(Type::I32, false), address(Type::I32, true)] {
        let kind = if matches!(capture, Type::Address { mutable: true, .. }) {
            CallableKind::FnMut
        } else {
            CallableKind::Fn
        };
        let base = fixture(kind, kind, true, 2, capture);
        base.accept();
        for field in 0..3 {
            let mut raw = base.clone();
            let c = raw.module.functions[0].callable_contract.as_mut().unwrap();
            let fields = match field {
                0 => &mut c.hold,
                1 => &mut c.call,
                _ => &mut c.writes,
            };
            if fields.is_empty() {
                continue;
            }
            fields.clear();
            raw.reject("E-VERIFY-CALLABLE-CONTRACT");
        }
    }
}
#[test]
fn borrowed_environment_cannot_be_dropped_or_loaded_as_value() {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 1, owned());
    raw.module.functions[0].blocks[0]
        .instructions
        .push(op(None, InstructionKind::DropClosure { local: LocalId(0) }));
    raw.reject("E-VERIFY-CLOSURE-CALL");
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 1, owned());
    raw.module.functions[0].values[2] = env();
    raw.module.functions[0].blocks[0].instructions[2].kind = InstructionKind::ReadAddress {
        address: ValueId(1),
    };
    raw.module.functions[0].return_type = Type::Unit;
    raw.module.functions[0].blocks[0].terminator = Terminator::Return(None);
    raw.reject("E-VERIFY-ADDRESS");
}
#[test]
fn borrowed_consume_and_unavailable_mode_are_rejected() {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 1, owned());
    let InstructionKind::CallClosureBorrowed { kind, .. } =
        &mut raw.module.functions[0].blocks[0].instructions[2].kind
    else {
        panic!()
    };
    *kind = CallableKind::FnOnce;
    raw.reject("E-VERIFY-CLOSURE-CALL");
    let mut raw = fixture(CallableKind::FnMut, CallableKind::FnMut, true, 1, owned());
    raw.module.functions[0].locals[0].ty = address(env(), false);
    raw.module.functions[0].values[1] = address(env(), false);
    raw.reject("E-VERIFY-CLOSURE-CALL");
}
#[test]
fn consumed_owned_parameter_cannot_be_recalled_or_leaked() {
    let raw = fixture(
        CallableKind::FnOnce,
        CallableKind::FnOnce,
        false,
        2,
        owned(),
    );
    raw.reject("E-VERIFY-INITIALIZATION");
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, false, 2, owned());
    raw.module.functions[0].blocks[0].instructions.pop();
    raw.reject("E-VERIFY-RESOURCE");
}
#[test]
fn longer_call_sequence_is_checked_in_full_without_truncation() {
    for count in [3, 4, 16, 65] {
        let raw = fixture(CallableKind::Fn, CallableKind::Fn, true, count, owned());
        raw.accept();
        for mutation in 0..4 {
            let mut changed = raw.clone();
            let claim = changed.module.functions[0]
                .callable_contract
                .as_mut()
                .unwrap();
            match mutation {
                0 => {
                    claim.traces.as_finite_mut().unwrap()[0].calls.pop();
                }
                1 => {
                    claim.traces.as_finite_mut().unwrap()[0].calls[count - 1] = CallableKind::FnMut;
                }
                2 => {
                    claim.traces.as_finite_mut().unwrap()[0].exit = CallableExit::Trap;
                }
                _ => {
                    let duplicate = claim.traces.as_finite().unwrap()[0].clone();
                    claim.traces.as_finite_mut().unwrap().push(duplicate);
                }
            }
            changed.reject("E-VERIFY-CALLABLE-CONTRACT");
        }
    }
}
#[test]
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
#[test]
fn helper_forwarding_does_not_guess_an_empty_nested_contract() {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 0, owned());
    let f = &mut raw.module.functions[0];
    f.values.push(Type::I32);
    f.blocks[0].instructions.push(op(
        Some(ValueId(2)),
        InstructionKind::Call {
            function: FunctionId(0),
            arguments: vec![ValueId(1)],
        },
    ));
    for result in raw.both() {
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].phase, Phase::Verify);
        assert_eq!(errors[0].category, zil_base::ResultCategory::Unsupported);
        assert_eq!(errors[0].code, "E-UNSUPPORTED");
        assert_eq!(
            errors[0].message,
            "recursive callable helper dependency requires an explicit finite contract"
        );
    }
}
#[test]
fn unrelated_function_cannot_forge_a_callback_contract() {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 1, owned());
    raw.module.functions[1].callable_contract = raw.module.functions[0].callable_contract.clone();
    raw.reject("E-VERIFY-CALLABLE-CONTRACT");
}

/// A real raw caller creates an environment over a local address, borrows that
/// environment into an ordinary public Call, then keeps using the original.
fn address_caller(mutable: bool) -> Raw {
    let kind = if mutable {
        CallableKind::FnMut
    } else {
        CallableKind::Fn
    };
    let capture = address(Type::I32, mutable);
    let mut raw = fixture(kind, kind, true, 2, capture.clone());
    let mut caller = Body::new(2);
    let target = caller.local(Type::I32, false);
    let n = caller.number(7);
    caller.operation(InstructionKind::Store {
        local: target,
        value: n,
    });
    let captured = caller.value(
        capture,
        if mutable {
            InstructionKind::BorrowLocalMut { local: target }
        } else {
            InstructionKind::BorrowLocal { local: target }
        },
        vec![HoldRoot::Local(target)],
    );
    let environment = caller.local(env(), false);
    let created = caller.value(
        env(),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![captured],
        },
        vec![HoldRoot::Local(target)],
    );
    caller.operation(InstructionKind::Store {
        local: environment,
        value: created,
    });
    let borrowed = caller.value(
        address(env(), mutable),
        if mutable {
            InstructionKind::BorrowLocalMut { local: environment }
        } else {
            InstructionKind::BorrowLocal { local: environment }
        },
        vec![HoldRoot::Local(target), HoldRoot::Local(environment)],
    );
    let result = caller.value(
        Type::I32,
        InstructionKind::Call {
            function: FunctionId(0),
            arguments: vec![borrowed],
        },
        vec![],
    );
    caller.operation(InstructionKind::StorageDead(environment));
    caller.operation(InstructionKind::StorageDead(target));
    raw.push(caller.finish(result));
    raw
}
#[test]
fn helper_call_keeps_address_capture_windows_and_borrowed_position() {
    address_caller(false).accept();
    address_caller(true).accept();
}
#[test]
fn borrowed_environment_cannot_hide_dead_capture_storage() {
    for mutable in [false, true] {
        let mut raw = address_caller(mutable);
        let f = &mut raw.module.functions[2];
        let dead = f.blocks[0].instructions.pop().unwrap();
        let call = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::Call { .. }))
            .unwrap();
        f.blocks[0].instructions.insert(call, dead);
        raw.reject("E-BORROW-CONFLICT");
    }
}
#[test]
fn plan_hold_cannot_erase_the_capture_behind_an_environment_borrow() {
    let mut raw = address_caller(false);
    let ((_, _), row) = raw
        .holds
        .iter_mut()
        .find(|((f, _), row)| *f == 2 && row.len() == 2)
        .unwrap();
    row.remove(0);
    raw.reject("E-VERIFY-HOLD");
}
#[test]
fn position_move_while_borrowed_environment_has_future_call_is_rejected() {
    let mut raw = address_caller(false);
    let f = &mut raw.module.functions[2];
    let id = ValueId(f.values.len() as u32);
    f.values.push(env());
    raw.holds
        .insert((2, id.0), vec![HoldRoot::Local(LocalId(0))]);
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    f.blocks[0].instructions.insert(
        at,
        op(Some(id), InstructionKind::TakeLocal { local: LocalId(1) }),
    );
    raw.reject("E-BORROW-CONFLICT");
}
#[test]
fn captured_exclusive_window_rejects_target_access_during_environment_borrow() {
    let mut raw = address_caller(true);
    // The ordinary callee has no callback contract or new helper restriction.
    // This candidate must reach the actual borrow checker, not Unsupported.
    let mut ordinary = Body::new(3);
    let p = ordinary.local(address(Type::I32, true), true);
    let a = ordinary.value(
        address(Type::I32, true),
        InstructionKind::Load(p),
        vec![HoldRoot::Parameter(0)],
    );
    let value = ordinary.value(
        Type::I32,
        InstructionKind::ReadAddress { address: a },
        vec![],
    );
    raw.push(ordinary.finish(value));
    let f = &mut raw.module.functions[2];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    let second = ValueId(f.values.len() as u32);
    f.values.push(address(Type::I32, true));
    raw.holds
        .insert((2, second.0), vec![HoldRoot::Local(LocalId(0))]);
    let result = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(
        at,
        op(
            Some(second),
            InstructionKind::BorrowLocalMut { local: LocalId(0) },
        ),
    );
    f.blocks[0].instructions.insert(
        at + 1,
        op(
            Some(result),
            InstructionKind::Call {
                function: FunctionId(3),
                arguments: vec![second],
            },
        ),
    );
    raw.reject("E-BORROW-CONFLICT");
}

fn package_caller(future_call: bool) -> Raw {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 2, desc());
    let package = Type::Package {
        name: "Window".into(),
        fields: vec![("model".into(), owned()), ("id".into(), Type::I32)],
    };
    let mut caller = Body::new(2);
    caller.operation(InstructionKind::ScopeEnter { scope: ScopeId(0) });
    let local = caller.local(package.clone(), false);
    let n = caller.number(7);
    let token = caller.value(
        owned(),
        InstructionKind::MakeManaged { fields: vec![n] },
        vec![],
    );
    let window = caller.value(
        package.clone(),
        InstructionKind::MakePackage {
            fields: vec![token, n],
        },
        vec![],
    );
    caller.operation(InstructionKind::Store {
        local,
        value: window,
    });
    let d = caller.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 0 },
        vec![],
    );
    let environment = caller.local(env(), false);
    let callback = caller.value(
        env(),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![d],
        },
        vec![],
    );
    caller.operation(InstructionKind::Store {
        local: environment,
        value: callback,
    });
    let borrowed = caller.value(
        address(env(), false),
        InstructionKind::BorrowLocal { local: environment },
        vec![HoldRoot::Local(environment)],
    );
    let result = caller.value(
        Type::I32,
        InstructionKind::Call {
            function: FunctionId(0),
            arguments: vec![borrowed],
        },
        vec![],
    );
    let moved = caller.value(
        package.clone(),
        InstructionKind::TakeLocal { local },
        vec![],
    );
    let destination = caller.local(package, false);
    caller.operation(InstructionKind::Store {
        local: destination,
        value: moved,
    });
    if future_call {
        caller.value(
            Type::I32,
            InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![borrowed],
            },
            vec![],
        );
    }
    caller.operation(InstructionKind::DropPackage { local: destination });
    caller.operation(InstructionKind::DropPackage { local });
    caller.operation(InstructionKind::StorageDead(destination));
    caller.operation(InstructionKind::StorageDead(environment));
    caller.operation(InstructionKind::StorageDead(local));
    caller.operation(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    raw.push(caller.finish(result));
    raw
}
#[test]
fn package_field_description_may_end_at_last_public_helper_use() {
    package_caller(false).accept();
}
#[test]
fn future_public_helper_use_prevents_package_move_through_captured_description() {
    package_caller(true).reject("E-VERIFY-RESOURCE");
}

#[test]
fn public_helper_caller_needs_no_private_effect_rows_after_full_raw_wire_verification() {
    for calls in [0, 1, 2] {
        let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, calls, desc());
        // The extra complete owned parameter may be described and written in
        // the helper; its local root supplies no caller-side borrowed effect.
        let helper = &mut raw.module.functions[0];
        let owned_parameter = LocalId(helper.locals.len() as u32);
        helper.locals.push(LocalDecl {
            id: owned_parameter,
            name: "owned_parameter".into(),
            ty: owned(),
            mutable: false,
            span: span(),
        });
        helper.parameters.push(owned_parameter);
        let local_description = ValueId(helper.values.len() as u32);
        helper.values.push(desc());
        helper.blocks[0].instructions.extend([
            op(
                Some(local_description),
                InstructionKind::DescribeLocal {
                    local: owned_parameter,
                },
            ),
            op(
                None,
                InstructionKind::WriteDesc {
                    desc: local_description,
                    index: 0,
                    value: ValueId(0),
                },
            ),
            op(
                None,
                InstructionKind::DropManaged {
                    local: owned_parameter,
                },
            ),
        ]);
        let mut caller = Body::new(2);
        let parameter = caller.local(desc(), true);
        let d = caller.value(desc(), InstructionKind::Load(parameter), vec![]);
        let environment = caller.local(env(), false);
        let callback = caller.value(
            env(),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![d],
            },
            vec![],
        );
        caller.operation(InstructionKind::Store {
            local: environment,
            value: callback,
        });
        let borrowed = caller.value(
            address(env(), false),
            InstructionKind::BorrowLocal { local: environment },
            vec![HoldRoot::Local(environment)],
        );
        let number = caller.number(9);
        let owned_argument = caller.value(
            owned(),
            InstructionKind::MakeManaged {
                fields: vec![number],
            },
            vec![],
        );
        let result = caller.value(
            Type::I32,
            InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![borrowed, owned_argument],
            },
            vec![],
        );
        caller.operation(InstructionKind::StorageDead(environment));
        raw.push(caller.finish(result));
        for checked in raw.both() {
            // Full Verify checks every definition before the separate internal
            // consumer probe deletes or replaces private helper effects.
            let verified = checked.unwrap();
            resources::assert_public_helper_effect_independence(
                &verified,
                FunctionId(2),
                FunctionId(0),
                calls != 0,
            );
        }
    }
}

fn conditional_owned(trap: bool) -> Raw {
    let mut raw = fixture(CallableKind::Fn, CallableKind::FnOnce, false, 0, owned());
    let f = &mut raw.module.functions[0];
    f.blocks[0].instructions.pop();
    f.values.extend([Type::Bool, env(), Type::I32]);
    f.blocks[0].instructions.push(op(
        Some(ValueId(1)),
        InstructionKind::Constant(Constant::Bool(true)),
    ));
    f.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(1),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![
            op(
                Some(ValueId(2)),
                InstructionKind::TakeLocal { local: LocalId(0) },
            ),
            op(
                Some(ValueId(3)),
                InstructionKind::CallClosureOnce {
                    callee: ValueId(2),
                    arguments: vec![],
                },
            ),
            op(None, InstructionKind::DropClosure { local: LocalId(0) }),
        ],
        terminator: Terminator::Return(Some(ValueId(3))),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: if trap {
            vec![]
        } else {
            vec![op(None, InstructionKind::DropClosure { local: LocalId(0) })]
        },
        terminator: if trap {
            Terminator::Trap
        } else {
            Terminator::Return(Some(ValueId(0)))
        },
    });
    f.callable_contract.as_mut().unwrap().traces = zil_ir::CallableTraces::Finite(vec![
        CallableTrace {
            calls: vec![],
            exit: if trap {
                CallableExit::Trap
            } else {
                CallableExit::Return
            },
        },
        CallableTrace {
            calls: vec![CallableKind::FnOnce],
            exit: CallableExit::Return,
        },
    ]);
    raw
}
#[test]
fn conditional_zero_call_cleanup_and_one_call_consume_are_both_verified() {
    conditional_owned(false).accept();
    conditional_owned(true).accept();
    let mut raw = conditional_owned(false);
    raw.module.functions[0]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
        .as_finite_mut()
        .unwrap()
        .remove(0);
    raw.reject("E-VERIFY-CALLABLE-CONTRACT");
}
#[test]
fn uncalled_owned_environment_cleanup_cannot_be_omitted_on_one_return() {
    let mut raw = conditional_owned(false);
    raw.module.functions[0].blocks[2].instructions.clear();
    raw.reject("E-VERIFY-RESOURCE");
}

fn first_error_stops_second_call() -> Raw {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 2, owned());
    let union = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(Type::I32),
    };
    raw.module.closures[0].result = union.clone();
    let entry = &mut raw.module.functions[1];
    entry.return_type = union.clone();
    entry.values.push(union.clone());
    entry.blocks[0].instructions.insert(
        0,
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
    );
    entry.blocks[0].instructions.push(op(
        Some(ValueId(2)),
        InstructionKind::MakeErrorUnion {
            is_error: false,
            value: ValueId(1),
        },
    ));
    entry.blocks[0].instructions.push(op(
        None,
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Return,
        },
    ));
    entry.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    let f = &mut raw.module.functions[0];
    f.return_type = union.clone();
    f.values[2] = union.clone();
    f.values[3] = union;
    f.values.push(Type::Bool);
    let second = f.blocks[0].instructions.pop().unwrap();
    f.blocks[0].instructions.insert(
        0,
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
    );
    f.blocks[0].instructions.push(op(
        Some(ValueId(4)),
        InstructionKind::ErrorTag { value: ValueId(2) },
    ));
    f.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(4),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    let close = || {
        vec![
            op(None, InstructionKind::StorageDead(LocalId(0))),
            op(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]
    };
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: close(),
        terminator: Terminator::Return(Some(ValueId(2))),
    });
    let mut success = vec![second];
    success.extend(close());
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: success,
        terminator: Terminator::Return(Some(ValueId(3))),
    });
    f.callable_contract.as_mut().unwrap().traces = zil_ir::CallableTraces::Finite(vec![
        CallableTrace {
            calls: vec![CallableKind::Fn],
            exit: CallableExit::Return,
        },
        CallableTrace {
            calls: vec![CallableKind::Fn, CallableKind::Fn],
            exit: CallableExit::Return,
        },
    ]);
    raw
}
#[test]
fn copy_error_return_paths_keep_distinct_one_and_two_call_traces() {
    let raw = first_error_stops_second_call();
    raw.accept();
    let mut changed = raw;
    changed.module.functions[0]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
        .as_finite_mut()
        .unwrap()
        .remove(0);
    changed.reject("E-VERIFY-CALLABLE-CONTRACT");
}
#[test]
fn callback_claim_limits_are_paid_before_fingerprinting_and_no_partial_pass() {
    let mut oversized = fixture(CallableKind::Fn, CallableKind::Fn, true, 1, owned());
    oversized.module.functions[0]
        .callable_contract
        .as_mut()
        .unwrap()
        .hold = vec![0; 4096];
    let limits = Limits {
        max_analysis_steps: 1000,
        ..Limits::default()
    };
    let error = verify_module(
        oversized.module.clone(),
        oversized.plan(),
        &oversized.module.target,
        &limits,
    )
    .unwrap_err();
    assert_eq!(error[0].category, zil_base::ResultCategory::ResourceLimit);
    let raw = address_caller(true);
    let plan = raw.plan();
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let mid = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: mid,
            ..Limits::default()
        };
        match verify_module(
            raw.module.clone(),
            plan.clone(),
            &raw.module.target,
            &limits,
        ) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
                low = mid + 1;
            }
        }
    }
    assert!(low > 0);
    eprintln!("impl066 address_caller raw work boundary N={low}");
    for (work, pass) in [(low - 1, false), (low, true), (low + 1, true)] {
        let limits = Limits {
            max_analysis_steps: work,
            ..Limits::default()
        };
        let result = verify_module(
            raw.module.clone(),
            plan.clone(),
            &raw.module.target,
            &limits,
        );
        assert_eq!(result.is_ok(), pass, "work {work}");
    }
}

#[test]
fn helper_contract_node_quotas_are_resource_limits_before_identity_checks() {
    let raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 65, owned());
    let plan = raw.plan();
    for (nodes, succeeds) in [(65, true), (64, false), (65, true)] {
        let limits = Limits {
            max_nodes: nodes,
            ..Limits::default()
        };
        let checked = verify_module(
            raw.module.clone(),
            plan.clone(),
            &raw.module.target,
            &limits,
        );
        if succeeds {
            checked.unwrap();
        } else {
            let errors = checked.unwrap_err();
            assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
            assert_eq!(errors[0].code, "E-RESOURCE-LIMIT");
        }
    }
    // Do not update the fingerprint. These bounds must be checked before an
    // invalid identity can hide the fact that raw metadata exceeds its quota.
    for mutation in 0..3 {
        let mut changed = raw.module.clone();
        let claim = changed.functions[0].callable_contract.as_mut().unwrap();
        match mutation {
            0 => claim.hold = vec![0; 129],
            1 => {
                claim.traces =
                    zil_ir::CallableTraces::Finite(vec![
                        claim.traces.as_finite_mut().unwrap()[0]
                            .clone();
                        129
                    ])
            }
            _ => claim.traces.as_finite_mut().unwrap()[0].calls = vec![CallableKind::Fn; 20_000],
        }
        let limits = Limits {
            max_nodes: 128,
            ..Limits::default()
        };
        let errors = verify_module(changed, plan.clone(), &raw.module.target, &limits).unwrap_err();
        assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(errors[0].code, "E-RESOURCE-LIMIT");
        assert!(errors[0].message.contains("node allowance"));
    }
}

fn owned_address_caller(drop_before_call: bool) -> Raw {
    let mut raw = address_caller(false);
    raw.module.closures[0].captures.push(owned());
    let f = &mut raw.module.functions[2];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    let resource = ValueId(f.values.len() as u32);
    f.values.push(owned());
    f.blocks[0].instructions.insert(
        at,
        op(
            Some(resource),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
    );
    let InstructionKind::MakeClosure { captures, .. } = &mut f.blocks[0].instructions[at + 1].kind
    else {
        panic!()
    };
    captures.push(resource);
    let call = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    if drop_before_call {
        f.blocks[0].instructions.insert(
            call,
            op(None, InstructionKind::DropClosure { local: LocalId(1) }),
        );
    } else {
        let again = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        f.blocks[0].instructions.insert(
            call + 1,
            op(
                Some(again),
                InstructionKind::CallClosureShared {
                    callee: LocalId(1),
                    arguments: vec![],
                },
            ),
        );
        f.blocks[0].instructions.insert(
            call + 2,
            op(None, InstructionKind::DropClosure { local: LocalId(1) }),
        );
    }
    raw
}
#[test]
fn caller_keeps_owned_environment_after_helper_then_cleans_once() {
    owned_address_caller(false).accept();
}
#[test]
fn caller_cannot_drop_owned_environment_before_its_borrowed_call() {
    owned_address_caller(true).reject("E-BORROW-CONFLICT");
}

#[test]
fn helper_extra_borrowed_parameters_need_a_complete_public_effect_contract() {
    for ty in [desc(), address(Type::I32, false), address(Type::I32, true)] {
        let copy_address = matches!(ty, Type::Address { .. });
        let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 0, owned());
        let f = &mut raw.module.functions[0];
        f.parameters.push(LocalId(1));
        f.locals.push(LocalDecl {
            id: LocalId(1),
            name: "extra".into(),
            ty,
            mutable: false,
            span: span(),
        });
        if copy_address {
            // 189 admitted one explicit Copy address layer. These unused helper
            // parameters do not acquire any hidden capture or lending effect.
            raw.accept();
        } else {
            raw.reject("other callback helper parameters");
        }
    }
}
#[test]
fn helper_resource_result_is_not_silently_treated_as_a_copy_result() {
    let mut raw = fixture(CallableKind::Fn, CallableKind::Fn, true, 0, owned());
    let f = &mut raw.module.functions[0];
    f.return_type = owned();
    f.result_mode = PassingMode::Create;
    f.values.push(owned());
    f.blocks[0].instructions.push(op(
        Some(ValueId(2)),
        InstructionKind::MakeManaged {
            fields: vec![ValueId(0)],
        },
    ));
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    for result in raw.both() {
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].phase, Phase::Verify);
        assert_eq!(errors[0].category, zil_base::ResultCategory::Unsupported);
        assert_eq!(errors[0].code, "E-UNSUPPORTED");
        assert_eq!(
            errors[0].message,
            "concrete callback helper results require finite Copy or a borrowed environment lending result"
        );
    }
}
#[test]
fn fnmut_address_capture_is_consumed_or_ended_under_fnonce_bound() {
    for calls in [0, 1] {
        fixture(
            CallableKind::FnMut,
            CallableKind::FnOnce,
            false,
            calls,
            address(Type::I32, true),
        )
        .accept();
    }
}

//! End-to-end native, source-rejection, and Raw/Wire verification tests for G6.3-D
//! `Task` / `Registration` state-protocol joint semantics:
//! - `TaskCreate` conditional success (`.ok(t)` starts exact `Open` and carries captured
//!   mutable context capabilities into `TaskPoll`; `.err` does not mutate context args);
//! - `Valid == false` (`task_valid` / `async_registration_valid` / `registration_valid`)
//!   is read-only and NEVER proves protocol state `Closed`;
//! - `TaskStart` complete full-pipeline definition (`opspec` -> `zil-semantics` ->
//!   `zil-check` provenance & protocol state -> `zil-backend-llvm` native emission).

mod contract_semantics_support;

use contract_semantics_support::{
    build_and_run_dual_with_c, check_rejected, temp_dir, write_source,
};
use zil_base::ResultCategory;

#[test]
fn task_create_conditional_success_poll_and_readonly_valid_execute_natively_on_dual_backends() {
    let dir = temp_dir("task-reg-protocol-positive-native");
    let src = r#"
fn worker(seed: i32) -> i32 contract { effects(suspend); } {
    task_yield()
    return seed + 30
}

fn require_task_open(t: &Task<i32>)
    contract { requires(state(t, Open)); effects(invalidates()); }
{}

fn check_valid_readonly(t: Task<i32>) -> bool
    contract { effects(invalidates()); }
{
    return task_valid(t)
}

fn close_and_retire_task(give t: Task<i32>)
    contract {
        requires(state(t, Open));
        provides(retired(t));
    }
{
    var cur = give t
    task_request_cancel(&mut cur)
    loop {
        match task_poll_close(&mut cur) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give cur)
}

fn run_task_protocol() -> TaskCreateError!i32
    contract { effects(invalidates()); }
{
    var cur = task_create(worker, 12)?
    // `.ok(cur)` proves `Open` directly!
    require_task_open(&cur)
    let was_valid = check_valid_readonly(cur)
    var out = 0
    if was_valid {
        loop {
            match task_poll(&mut cur) {
                .ok(p) => {
                    match p {
                        Poll.Pending() => { continue },
                        Poll.Ready(v) => { out = v; break }
                    }
                },
                .err(_) => { break }
            }
        }
    }
    // Polling preserves monotonicity (no spurious rollback) and still requires close!
    require_task_open(&cur)
    close_and_retire_task(give cur)
    if was_valid && out == 42 { return 0 }
    return 3
}

fn main() -> i32 {
    match run_task_protocol() {
        .ok(code) => { return code }
        .err(_) => { return 9 }
    }
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn task_and_registration_protocol_negative_controls_are_rejected() {
    let dir = temp_dir("task-reg-protocol-negative");

    // 1. `task_valid == false` MUST NOT prove `Closed` (neither for `provides(state(..., Closed))`
    //    nor for calling `task_finish_destroy` in a contracted function).
    check_rejected(
        &dir,
        "task_valid_false_does_not_prove_closed",
        r#"
fn bad(t: &mut Task<i32>)
    contract {
        requires(state(t, Open, Closing, Closed));
        provides(state(t, Closed));
    }
{
    if !task_valid(t) {
        return
    }
    task_request_cancel(t)
    loop {
        match task_poll_close(t) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
}
fn worker() -> i32 { return 1 }
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. `async_registration_valid == false` MUST NOT prove `Closed`!
    check_rejected(
        &dir,
        "async_registration_valid_false_does_not_prove_closed",
        r#"
fn bad(r: &mut AsyncRegistration<i32>)
    contract {
        requires(state(r, Open, Closing, Closed));
        provides(state(r, Closed));
    }
{
    if !async_registration_valid(r) {
        return
    }
    async_registration_request_close(r)
    loop {
        match async_registration_poll_close(r) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. `task_poll` alone MUST NOT prove `Closed` for `task_finish_destroy`!
    check_rejected(
        &dir,
        "task_poll_does_not_prove_closed_for_destroy",
        r#"
fn worker() -> i32 { return 7 }
fn bad() -> TaskCreateError!i32 contract { effects(invalidates()); } {
    var cur = task_create(worker)?
    let _ = task_poll(&mut cur)
    task_finish_destroy(give cur)
    return 0
}
fn main() -> i32 {
    let _ = bad()
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. `task_create` capturing `&mut counter` followed by `task_poll` inside
    //    `effects(invalidates())` MUST be rejected because polling executes the worker!
    check_rejected(
        &dir,
        "task_create_captured_mut_ref_polled_violates_empty_invalidates",
        r#"
fn bump(p: &mut i32) -> i32 {
    *p = *p + 1
    return *p
}
fn bad(x: &mut i32) -> TaskCreateError!i32 contract { effects(invalidates()); } {
    var cur = task_create(bump, x)?
    let _ = task_poll(&mut cur)
    task_request_cancel(&mut cur)
    loop {
        match task_poll_close(&mut cur) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give cur)
    return 0
}
fn main() -> i32 {
    var v = 0
    let _ = bad(&mut v)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_task_start_definition_valid_semantics_and_native_emission() {
    use zil_ir::body::{
        Block, BlockId, Body, Callee, ConstValue, FnId, FnRole, FnSig, Function, Instruction,
        InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl, Module, Mutability, OpaqueTy,
        Operand, ParamEnv, Place, ProtocolStateSet, Region, ResumeMode, Rvalue,
        StateContractClause, Terminator, TyKind, TyTable, decode_module, encode_module,
    };

    // Helper building a Raw/Wire module with:
    //   FnId(0): worker(x: i32) -> i32
    //   FnId(1): require_open(t: &Managed<Task<i32>>) contract { requires(state(t, Open)) }
    //   FnId(2): caller using `IntrinsicOp::TaskStart`
    let build_task_start_module = |mode: &str| {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let bool_ty = tys.intern(TyKind::Bool);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let worker_fn_ty = tys.intern(TyKind::FnPtr {
            params: vec![i32_ty],
            result: i32_ty,
        });
        let task_inner = tys.intern(TyKind::Opaque(OpaqueTy::Task {
            result: i32_ty,
            context: None,
        }));
        let managed_task = tys.intern(TyKind::Managed(task_inner));
        let ref_task = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: managed_task,
        });
        let ref_mut_task = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: managed_task,
        });
        let poll_adt = tys.add_adt(zil_ir::body::AdtDef {
            name: "Poll".into(),
            kind: zil_ir::body::AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
                zil_ir::body::VariantDef {
                    name: "Pending".into(),
                    fields: vec![],
                },
                zil_ir::body::VariantDef {
                    name: "Ready".into(),
                    fields: vec![],
                },
            ],
            user_drop: false,
        });
        let empty_args = tys.intern_args(zil_ir::body::GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let poll_ty = tys.intern(TyKind::Adt {
            adt: poll_adt,
            args: empty_args,
        });

        let worker = Function {
            name: "worker".into(),
            sig: FnSig {
                params: vec![i32_ty],
                result: i32_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Transfer,
                state_contracts: vec![],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: i32_ty,
                    name: Some("x".into()),
                }],
                captures: vec![],
                closure: None,
                result: i32_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Copy(Place::local(Local(0))),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };

        let require_open = Function {
            name: "require_open".into(),
            sig: FnSig {
                params: vec![ref_task],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::RequiresState {
                    param: 0,
                    states: ProtocolStateSet(ProtocolStateSet::OPEN),
                }],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ref_task,
                    name: Some("t".into()),
                }],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };

        let caller = match mode {
            // Positive: TaskStart -> check task_valid(&t) -> on true arm require_open(&t) ->
            // request_cancel -> poll_close loop -> finish_destroy. On false arm, drop task.
            "valid_true_refines_open" => Function {
                name: "start_and_check_valid".into(),
                sig: FnSig {
                    params: vec![],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![StateContractClause::InvalidationUpperBound],
                },
                body: Body {
                    param_count: 0,
                    locals: vec![
                        LocalDecl {
                            ty: worker_fn_ty,
                            name: Some("w".into()),
                        },
                        LocalDecl {
                            ty: managed_task,
                            name: Some("t".into()),
                        },
                        LocalDecl {
                            ty: ref_task,
                            name: Some("tr".into()),
                        },
                        LocalDecl {
                            ty: bool_ty,
                            name: Some("v".into()),
                        },
                        LocalDecl {
                            ty: ref_mut_task,
                            name: Some("tm".into()),
                        },
                        LocalDecl {
                            ty: poll_ty,
                            name: Some("p".into()),
                        },
                        LocalDecl {
                            ty: i32_ty,
                            name: Some("d".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("u".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        // bb0: w = FnRef(0); t = TaskStart(w, 7)
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(0)),
                                    value: Rvalue::FnRef(FnId(0)),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskStart),
                                args: vec![
                                    Operand::Copy(Place::local(Local(0))),
                                    Operand::Const(ConstValue::Int {
                                        ty: IntTy::I32,
                                        bits: 7,
                                    }),
                                ],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb1: tr = &t; v = TaskValid(tr)
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(2)),
                                    value: Rvalue::Ref {
                                        place: Place::local(Local(1)),
                                        mutability: Mutability::Shared,
                                        region: Region::Static,
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskValid),
                                args: vec![Operand::Copy(Place::local(Local(2)))],
                                dest: Place::local(Local(3)),
                                target: BlockId(2),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb2: switch v { 1 => bb3, otherwise => bb7 }
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Switch {
                                discr: Operand::Copy(Place::local(Local(3))),
                                arms: vec![(1, BlockId(3))],
                                otherwise: BlockId(7),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb3 (valid == true): require_open(tr)
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![Operand::Copy(Place::local(Local(2)))],
                                dest: Place::local(Local(7)),
                                target: BlockId(4),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb4: tm = &mut t; TaskRequestCancel(move tm)
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(4)),
                                    value: Rvalue::Ref {
                                        place: Place::local(Local(1)),
                                        mutability: Mutability::Exclusive,
                                        region: Region::Static,
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskRequestCancel),
                                args: vec![Operand::Move(Place::local(Local(4)))],
                                dest: Place::local(Local(7)),
                                target: BlockId(5),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb5: tm = &mut t; p = TaskPollClose(move tm) -> bb8
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(4)),
                                    value: Rvalue::Ref {
                                        place: Place::local(Local(1)),
                                        mutability: Mutability::Exclusive,
                                        region: Region::Static,
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskPollClose),
                                args: vec![Operand::Move(Place::local(Local(4)))],
                                dest: Place::local(Local(5)),
                                target: BlockId(8),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb6: TaskFinishDestroy(move t) -> bb9
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskFinishDestroy),
                                args: vec![Operand::Move(Place::local(Local(1)))],
                                dest: Place::local(Local(7)),
                                target: BlockId(9),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb7 (valid == false): Drop(t) -> bb9
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Drop {
                                    place: Place::local(Local(1)),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Goto { target: BlockId(9) },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb8: d = Discriminant(p); switch d { 1 => bb6, otherwise => bb5 }
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(6)),
                                    value: Rvalue::Discriminant(Place::local(Local(5))),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Switch {
                                discr: Operand::Copy(Place::local(Local(6))),
                                arms: vec![(1, BlockId(6))],
                                otherwise: BlockId(5),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // bb9: return ()
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            },
            // Negative 1: TaskStart WITHOUT checking `task_valid == true` calls `require_open` -> REJECT!
            "unchecked_task_start_not_open" => Function {
                name: "unchecked_start".into(),
                sig: FnSig {
                    params: vec![],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![],
                },
                body: Body {
                    param_count: 0,
                    locals: vec![
                        LocalDecl {
                            ty: worker_fn_ty,
                            name: Some("w".into()),
                        },
                        LocalDecl {
                            ty: managed_task,
                            name: Some("t".into()),
                        },
                        LocalDecl {
                            ty: ref_task,
                            name: Some("tr".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("u".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(0)),
                                    value: Rvalue::FnRef(FnId(0)),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskStart),
                                args: vec![
                                    Operand::Copy(Place::local(Local(0))),
                                    Operand::Const(ConstValue::Int {
                                        ty: IntTy::I32,
                                        bits: 7,
                                    }),
                                ],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(2)),
                                    value: Rvalue::Ref {
                                        place: Place::local(Local(1)),
                                        mutability: Mutability::Shared,
                                        region: Region::Static,
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![Operand::Copy(Place::local(Local(2)))],
                                dest: Place::local(Local(3)),
                                target: BlockId(2),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Drop {
                                    place: Place::local(Local(1)),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            },
            // Negative 2: TaskStart + `task_valid == false` calling `TaskFinishDestroy` -> REJECT!
            "valid_false_forged_destroy" => Function {
                name: "valid_false_destroy".into(),
                sig: FnSig {
                    params: vec![],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![StateContractClause::InvalidationUpperBound],
                },
                body: Body {
                    param_count: 0,
                    locals: vec![
                        LocalDecl {
                            ty: worker_fn_ty,
                            name: Some("w".into()),
                        },
                        LocalDecl {
                            ty: managed_task,
                            name: Some("t".into()),
                        },
                        LocalDecl {
                            ty: ref_task,
                            name: Some("tr".into()),
                        },
                        LocalDecl {
                            ty: bool_ty,
                            name: Some("v".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("u".into()),
                        },
                    ],
                    captures: vec![],
                    closure: None,
                    result: unit_ty,
                    param_env: ParamEnv::default(),
                    drop_flags: vec![],
                    blocks: vec![
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(0)),
                                    value: Rvalue::FnRef(FnId(0)),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskStart),
                                args: vec![
                                    Operand::Copy(Place::local(Local(0))),
                                    Operand::Const(ConstValue::Int {
                                        ty: IntTy::I32,
                                        bits: 7,
                                    }),
                                ],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(2)),
                                    value: Rvalue::Ref {
                                        place: Place::local(Local(1)),
                                        mutability: Mutability::Shared,
                                        region: Region::Static,
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskValid),
                                args: vec![Operand::Copy(Place::local(Local(2)))],
                                dest: Place::local(Local(3)),
                                target: BlockId(2),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Switch {
                                discr: Operand::Copy(Place::local(Local(3))),
                                arms: vec![(0, BlockId(3))],
                                otherwise: BlockId(4),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        // valid == false arm tries to call TaskFinishDestroy!
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskFinishDestroy),
                                args: vec![Operand::Move(Place::local(Local(1)))],
                                dest: Place::local(Local(4)),
                                target: BlockId(5),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Drop {
                                    place: Place::local(Local(1)),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Goto { target: BlockId(5) },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![],
                            terminator: Terminator::Return {
                                value: Operand::Const(ConstValue::Unit),
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                    ],
                },
            },
            _ => unreachable!(),
        };

        Module {
            tys,
            functions: vec![worker, require_open, caller],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        }
    };

    // 1. Positive: `TaskStart` -> `TaskValid == true` refines to `Open`, closes cleanly,
    //    AND emits valid LLVM IR through `zil-backend-llvm`!
    {
        let m = build_task_start_module("valid_true_refines_open");
        let decoded = decode_module(&encode_module(&m)).expect("decode task_start ok");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve task_start ok");
        let verified =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect("TaskStart with valid == true check must pass verification");
        let ir = zil_backend_llvm::emit_verified_body(&verified, false)
            .expect("TaskStart must emit valid LLVM IR without unimplemented error");
        assert!(
            ir.contains("@__zil_task_entry_f"),
            "TaskStart LLVM IR must emit task entry trampoline"
        );
    }

    // 2. Negative: `TaskStart` without checking `task_valid == true` cannot satisfy `Open`.
    {
        let m = build_task_start_module("unchecked_task_start_not_open");
        let decoded = decode_module(&encode_module(&m)).expect("decode unchecked task_start");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve unchecked task_start");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("unchecked TaskStart must NOT satisfy requires(state(t, Open))");
        assert_eq!(err.function, FnId(2));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // 3. Negative: `TaskStart` with `task_valid == false` MUST NOT satisfy `TaskFinishDestroy`.
    {
        let m = build_task_start_module("valid_false_forged_destroy");
        let decoded = decode_module(&encode_module(&m)).expect("decode valid_false_destroy");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve valid_false_destroy");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("task_valid == false must NOT prove Closed for TaskFinishDestroy");
        assert_eq!(err.function, FnId(2));
        assert_eq!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(
                "finish_destroy called on protocol handle not proved Closed"
            )
        );
    }
}

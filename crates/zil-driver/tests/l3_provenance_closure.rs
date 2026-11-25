//! Path-sensitive CFG provenance closure & Raw/Wire counterexample tests
//! addressing Round 9 review findings on PR #103 (G8.6 Task worker provenance),
//! PR #104 (G8.5 OwnedDyn-erased MutexGuard transfer), and PR #105 (G7.2 ScopedCallback
//! foreign provider provenance & Raw/Wire same-root alias conflict).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::{FileId, ResultCategory, Span};
use zil_check::{CheckBudget, CheckError, CheckErrorKind, Violation, ViolationKind, check_module};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};
use zil_ir::body::{
    AdtDef, AdtKind, ArgsId, Block, BlockId, Body, Callee, Certificate, ClosureDef, ClosureId,
    ClosureKind, ConstValue, DynDef, DynId, DynImpl, DynSlotSig, FieldDef, FnId, FnRole, FnSig,
    ForeignAbi, ForeignCallbackRole, ForeignDecl, ForeignId, Function, FunctionCertificate,
    GenericArgs, Instruction, InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl, Module,
    Mutability, OpaqueTy, Operand, ParamEnv, Place, PlaceRoot, Projection, Region, ResumeMode,
    Rvalue, Terminator, TyKind, TyTable, VariantDef, decode_module, encode_module,
};
use zil_solve::{Budget, SolveError};

fn sp() -> Span {
    Span::new(FileId(0), 0, 0)
}

fn verify_wire_roundtrip(module: &Module) -> Result<(), CheckError> {
    let bytes = encode_module(module);
    let decoded = decode_module(&bytes).expect("wire roundtrip must decode");
    let cert = zil_solve::solve_module(&decoded, Budget { steps: 200_000 })
        .unwrap_or_else(|_| Certificate::default());
    check_module(decoded, &cert, CheckBudget { steps: 200_000 }).map(|_| ())
}

fn assert_rejected_with(res: Result<(), CheckError>, needle: &str) {
    match res {
        Err(error) => match error.kind {
            CheckErrorKind::CertificateShape(msg) => {
                assert!(
                    msg.contains(needle),
                    "expected error containing `{needle}`, got `{msg}`"
                );
            }
            other => panic!("expected CertificateShape containing `{needle}`, got {other:?}"),
        },
        Ok(()) => panic!("expected rejection containing `{needle}`, but module was accepted"),
    }
}

/// Helper: build a minimal module with:
/// - FnId(0): `transfer_worker() -> i32` (`ResumeMode::Transfer`)
/// - FnId(1): `local_worker() -> i32` (`ResumeMode::Local`)
/// - FnId(2): `thread_runner(Managed<Task<i32>>) -> i32`
/// - FnId(3): `parent_closure() -> i32` (0-capture closure)
/// - FnId(4): `main(cond: bool) -> i32` (caller-provided body)
fn build_task_provenance_module(
    build_main: impl FnOnce(
        &mut TyTable,
        /* bool_ty */ zil_ir::body::TyId,
        /* i32_ty */ zil_ir::body::TyId,
        /* worker_fn_ty */ zil_ir::body::TyId,
        /* runner_fn_ty */ zil_ir::body::TyId,
        /* task_managed_ty */ zil_ir::body::TyId,
        /* task_err_union_ty */ zil_ir::body::TyId,
        /* parent_closure_ty */ zil_ir::body::TyId,
        /* thread_res_ty */ zil_ir::body::TyId,
    ) -> Body,
) -> Module {
    let mut tys = TyTable::new();
    let bool_ty = tys.intern(TyKind::Bool);
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let worker_fn_ty = tys.intern(TyKind::FnPtr {
        params: Vec::new(),
        result: i32_ty,
    });
    let task_opaque = tys.intern(TyKind::Opaque(OpaqueTy::Task {
        result: i32_ty,
        context: None,
    }));
    let task_managed_ty = tys.intern(TyKind::Managed(task_opaque));
    let runner_fn_ty = tys.intern(TyKind::FnPtr {
        params: vec![task_managed_ty],
        result: i32_ty,
    });

    // AdtId(0): enum TaskCreateError { OutOfMemory }
    let err_adt = tys.add_adt(AdtDef {
        name: "TaskCreateError".into(),
        kind: AdtKind::Enum,
        type_params: 0,
        region_params: 0,
        user_drop: false,
        variants: vec![VariantDef {
            name: "OutOfMemory".into(),
            fields: Vec::new(),
        }],
    });
    let empty_args = tys.intern_args(GenericArgs {
        tys: Vec::new(),
        regions: Vec::new(),
    });
    let err_ty = tys.intern(TyKind::Adt {
        adt: err_adt,
        args: empty_args,
    });
    let task_err_union_ty = tys.intern(TyKind::ErrorUnion {
        ok: task_managed_ty,
        err: err_ty,
    });

    // AdtId(1): enum TaskThreadScopeResult { Failed(Managed<Task<i32>>), Completed(i32, i32) }
    let res_adt = tys.add_adt(AdtDef {
        name: "TaskThreadScopeResult".into(),
        kind: AdtKind::Enum,
        type_params: 0,
        region_params: 0,
        user_drop: false,
        variants: vec![
            VariantDef {
                name: "Failed".into(),
                fields: vec![FieldDef {
                    name: "0".into(),
                    ty: task_managed_ty,
                }],
            },
            VariantDef {
                name: "Completed".into(),
                fields: vec![
                    FieldDef {
                        name: "0".into(),
                        ty: i32_ty,
                    },
                    FieldDef {
                        name: "1".into(),
                        ty: i32_ty,
                    },
                ],
            },
        ],
    });
    let thread_res_ty = tys.intern(TyKind::Adt {
        adt: res_adt,
        args: empty_args,
    });

    // ClosureId(0): () -> i32
    let cdef_id = tys.add_closure(ClosureDef {
        kind: ClosureKind::Fn,
        type_params: 0,
        captures: Vec::new(),
        params: Vec::new(),
        result: i32_ty,
    });
    let parent_closure_ty = tys.intern(TyKind::Closure {
        def: cdef_id,
        args: empty_args,
    });

    let simple_i32_body = |closure: Option<ClosureId>| Body {
        param_count: 0,
        locals: Vec::new(),
        captures: Vec::new(),
        closure,
        result: i32_ty,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: vec![Block {
            instructions: Vec::new(),
            terminator: Terminator::Return {
                value: Operand::Const(ConstValue::Int {
                    ty: IntTy::I32,
                    bits: 0,
                }),
            },
            terminator_span: sp(),
        }],
    };

    let runner_body = Body {
        param_count: 1,
        locals: vec![LocalDecl {
            ty: task_managed_ty,
            name: Some("t".into()),
        }],
        captures: Vec::new(),
        closure: None,
        result: i32_ty,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: vec![Block {
            instructions: vec![Instruction {
                kind: InstructionKind::Drop {
                    place: Place::local(Local(0)),
                },
                span: sp(),
            }],
            terminator: Terminator::Return {
                value: Operand::Const(ConstValue::Int {
                    ty: IntTy::I32,
                    bits: 0,
                }),
            },
            terminator_span: sp(),
        }],
    };

    let main_body = build_main(
        &mut tys,
        bool_ty,
        i32_ty,
        worker_fn_ty,
        runner_fn_ty,
        task_managed_ty,
        task_err_union_ty,
        parent_closure_ty,
        thread_res_ty,
    );

    Module {
        tys,
        functions: vec![
            Function {
                name: "transfer_worker".into(),
                sig: FnSig {
                    type_params: 0,
                    region_params: 0,
                    params: Vec::new(),
                    result: i32_ty,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Transfer,
                    state_contracts: Vec::new(),
                },
                body: simple_i32_body(None),
            },
            Function {
                name: "local_worker".into(),
                sig: FnSig {
                    type_params: 0,
                    region_params: 0,
                    params: Vec::new(),
                    result: i32_ty,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: Vec::new(),
                },
                body: simple_i32_body(None),
            },
            Function {
                name: "thread_runner".into(),
                sig: FnSig {
                    type_params: 0,
                    region_params: 0,
                    params: vec![task_managed_ty],
                    result: i32_ty,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: Vec::new(),
                },
                body: runner_body,
            },
            Function {
                name: "parent_closure".into(),
                sig: FnSig {
                    type_params: 0,
                    region_params: 0,
                    params: Vec::new(),
                    result: i32_ty,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: Vec::new(),
                },
                body: simple_i32_body(Some(cdef_id)),
            },
            Function {
                name: "main".into(),
                sig: FnSig {
                    type_params: 0,
                    region_params: 0,
                    params: vec![bool_ty],
                    result: i32_ty,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: Vec::new(),
                },
                body: main_body,
            },
        ],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    }
}

#[test]
fn raw_wire_task_worker_branch_join_must_fail_closed() {
    // Block 1 writes Local worker (FnId(1)) into Local(1).
    // Block 2 writes Transfer worker (FnId(0)) into Local(1) (last in block order, which
    // would fool a whole-function HashMap!).
    // Join block 3 creates a Task from Local(1) and transfers it via ThreadScope.
    let module = build_task_provenance_module(
        |_tys,
         bool_ty,
         i32_ty,
         worker_fn_ty,
         runner_fn_ty,
         task_managed_ty,
         task_err_union_ty,
         parent_closure_ty,
         thread_res_ty| Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: bool_ty,
                    name: Some("cond".into()),
                },
                LocalDecl {
                    ty: worker_fn_ty,
                    name: Some("w".into()),
                },
                LocalDecl {
                    ty: task_err_union_ty,
                    name: Some("eu".into()),
                },
                LocalDecl {
                    ty: bool_ty,
                    name: Some("ok_flag".into()),
                },
                LocalDecl {
                    ty: task_managed_ty,
                    name: Some("t".into()),
                },
                LocalDecl {
                    ty: runner_fn_ty,
                    name: Some("runner".into()),
                },
                LocalDecl {
                    ty: parent_closure_ty,
                    name: Some("parent".into()),
                },
                LocalDecl {
                    ty: thread_res_ty,
                    name: Some("tres".into()),
                },
            ],
            captures: Vec::new(),
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![
                // bb0: branch on cond
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::StorageLive(Local(1)),
                        span: sp(),
                    }],
                    terminator: Terminator::Switch {
                        discr: Operand::Copy(Place::local(Local(0))),
                        arms: vec![(0, BlockId(1))],
                        otherwise: BlockId(2),
                    },
                    terminator_span: sp(),
                },
                // bb1: w = local_worker (FnId(1))
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(1)),
                            value: Rvalue::FnRef(FnId(1)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Goto { target: BlockId(3) },
                    terminator_span: sp(),
                },
                // bb2: w = transfer_worker (FnId(0)) — later in block order!
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(1)),
                            value: Rvalue::FnRef(FnId(0)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Goto { target: BlockId(3) },
                    terminator_span: sp(),
                },
                // bb3: join -> TaskCreate(w)
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::StorageLive(Local(2)),
                        span: sp(),
                    }],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::TaskCreate),
                        args: vec![Operand::Copy(Place::local(Local(1)))],
                        dest: Place::local(Local(2)),
                        target: BlockId(4),
                        cancel: None,
                    },
                    terminator_span: sp(),
                },
                // bb4: check ok_flag
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(3)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(3)),
                                value: Rvalue::ErrorArm(Place::local(Local(2))),
                            },
                            span: sp(),
                        },
                    ],
                    terminator: Terminator::Switch {
                        discr: Operand::Copy(Place::local(Local(3))),
                        arms: vec![(1, BlockId(5))],
                        otherwise: BlockId(7),
                    },
                    terminator_span: sp(),
                },
                // bb5: unwrap ok arm -> ThreadScope(runner, t, parent)
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(4)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(4)),
                                value: Rvalue::Use(Operand::Move(Place {
                                    root: PlaceRoot::Local(Local(2)),
                                    projections: vec![Projection::ErrorArm(true)],
                                })),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(5)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(5)),
                                value: Rvalue::FnRef(FnId(2)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(6)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(6)),
                                value: Rvalue::Aggregate {
                                    kind: zil_ir::body::AggregateKind::Closure {
                                        def: ClosureId(0),
                                        args: ArgsId(0),
                                    },
                                    operands: Vec::new(),
                                },
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(7)),
                            span: sp(),
                        },
                    ],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::ThreadScope),
                        args: vec![
                            Operand::Copy(Place::local(Local(5))),
                            Operand::Move(Place::local(Local(4))),
                            Operand::Move(Place::local(Local(6))),
                        ],
                        dest: Place::local(Local(7)),
                        target: BlockId(6),
                        cancel: None,
                    },
                    terminator_span: sp(),
                },
                // bb6: drop tres and return
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Drop {
                            place: Place::local(Local(7)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 0,
                        }),
                    },
                    terminator_span: sp(),
                },
                // bb7: error arm return
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Drop {
                            place: Place::local(Local(2)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 1,
                        }),
                    },
                    terminator_span: sp(),
                },
            ],
        },
    );
    assert_rejected_with(verify_wire_roundtrip(&module), "resume(transfer)");
}

#[test]
fn raw_wire_task_worker_reassignment_after_create_must_fail_closed() {
    // Local(1) is assigned Local worker (FnId(1)), passed to TaskCreate,
    // and THEN reassigned Transfer worker (FnId(0)) later in the same function.
    // A whole-function HashMap would overwrite Local(1) -> FnId(0) and falsely accept!
    let module = build_task_provenance_module(
        |_tys,
         bool_ty,
         i32_ty,
         worker_fn_ty,
         runner_fn_ty,
         task_managed_ty,
         task_err_union_ty,
         parent_closure_ty,
         thread_res_ty| Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: bool_ty,
                    name: Some("cond".into()),
                },
                LocalDecl {
                    ty: worker_fn_ty,
                    name: Some("w".into()),
                },
                LocalDecl {
                    ty: task_err_union_ty,
                    name: Some("eu".into()),
                },
                LocalDecl {
                    ty: bool_ty,
                    name: Some("ok_flag".into()),
                },
                LocalDecl {
                    ty: task_managed_ty,
                    name: Some("t".into()),
                },
                LocalDecl {
                    ty: runner_fn_ty,
                    name: Some("runner".into()),
                },
                LocalDecl {
                    ty: parent_closure_ty,
                    name: Some("parent".into()),
                },
                LocalDecl {
                    ty: thread_res_ty,
                    name: Some("tres".into()),
                },
            ],
            captures: Vec::new(),
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![
                // bb0: w = local_worker (FnId(1)); TaskCreate(w)
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(1)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(1)),
                                value: Rvalue::FnRef(FnId(1)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(2)),
                            span: sp(),
                        },
                    ],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::TaskCreate),
                        args: vec![Operand::Copy(Place::local(Local(1)))],
                        dest: Place::local(Local(2)),
                        target: BlockId(1),
                        cancel: None,
                    },
                    terminator_span: sp(),
                },
                // bb1: overwrite w = transfer_worker (FnId(0)) AFTER TaskCreate!
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(1)),
                                value: Rvalue::FnRef(FnId(0)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(3)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(3)),
                                value: Rvalue::ErrorArm(Place::local(Local(2))),
                            },
                            span: sp(),
                        },
                    ],
                    terminator: Terminator::Switch {
                        discr: Operand::Copy(Place::local(Local(3))),
                        arms: vec![(1, BlockId(2))],
                        otherwise: BlockId(4),
                    },
                    terminator_span: sp(),
                },
                // bb2: unwrap ok arm -> ThreadScope(runner, t, parent)
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(4)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(4)),
                                value: Rvalue::Use(Operand::Move(Place {
                                    root: PlaceRoot::Local(Local(2)),
                                    projections: vec![Projection::ErrorArm(true)],
                                })),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(5)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(5)),
                                value: Rvalue::FnRef(FnId(2)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(6)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(6)),
                                value: Rvalue::Aggregate {
                                    kind: zil_ir::body::AggregateKind::Closure {
                                        def: ClosureId(0),
                                        args: ArgsId(0),
                                    },
                                    operands: Vec::new(),
                                },
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(7)),
                            span: sp(),
                        },
                    ],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::ThreadScope),
                        args: vec![
                            Operand::Copy(Place::local(Local(5))),
                            Operand::Move(Place::local(Local(4))),
                            Operand::Move(Place::local(Local(6))),
                        ],
                        dest: Place::local(Local(7)),
                        target: BlockId(3),
                        cancel: None,
                    },
                    terminator_span: sp(),
                },
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Drop {
                            place: Place::local(Local(7)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 0,
                        }),
                    },
                    terminator_span: sp(),
                },
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Drop {
                            place: Place::local(Local(2)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 1,
                        }),
                    },
                    terminator_span: sp(),
                },
            ],
        },
    );
    assert_rejected_with(verify_wire_roundtrip(&module), "resume(transfer)");
}

/// Build a module for ScopedCallback provider provenance tests:
/// - ForeignId(0): `good_reg` (`ScopedRegister`)
/// - ForeignId(1): `good_close` (`ScopedClose`)
/// - ForeignId(2): `unproved_reg` (`None`)
fn build_scoped_provider_module(mode: &str) -> Module {
    let mut tys = TyTable::new();
    let unit = tys.intern(TyKind::Unit);
    let bool_ty = tys.intern(TyKind::Bool);
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
    let handle = tys.intern(TyKind::Opaque(OpaqueTy::ForeignHandle));
    let cb_ty = tys.intern(TyKind::Opaque(OpaqueTy::ForeignCallback {
        params: vec![handle, i32_ty],
        result: i64_ty,
    }));
    let reg_ty = tys.intern(TyKind::Opaque(OpaqueTy::ForeignCallback {
        params: vec![cb_ty, handle],
        result: handle,
    }));
    let close_ty = tys.intern(TyKind::Opaque(OpaqueTy::ForeignCallback {
        params: vec![handle],
        result: unit,
    }));
    let f_fn_ty = tys.intern(TyKind::FnPtr {
        params: vec![i32_ty],
        result: i32_ty,
    });
    let f_mut_ref = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: f_fn_ty,
    });
    let f_shared_ref = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: f_fn_ty,
    });
    let body_fn_ty = tys.intern(TyKind::FnPtr {
        params: Vec::new(),
        result: i32_ty,
    });
    let out_ty = tys.intern(TyKind::Tuple(vec![bool_ty, i32_ty]));

    let f_body = Body {
        param_count: 1,
        locals: vec![LocalDecl {
            ty: i32_ty,
            name: None,
        }],
        captures: Vec::new(),
        closure: None,
        result: i32_ty,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: vec![Block {
            instructions: Vec::new(),
            terminator: Terminator::Return {
                value: Operand::Copy(Place::local(Local(0))),
            },
            terminator_span: sp(),
        }],
    };
    let b_body = Body {
        param_count: 0,
        locals: Vec::new(),
        captures: Vec::new(),
        closure: None,
        result: i32_ty,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: vec![Block {
            instructions: Vec::new(),
            terminator: Terminator::Return {
                value: Operand::Const(ConstValue::Int {
                    ty: IntTy::I32,
                    bits: 0,
                }),
            },
            terminator_span: sp(),
        }],
    };

    let main_body = match mode {
        "branch_join" => Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: bool_ty,
                    name: None,
                },
                LocalDecl {
                    ty: reg_ty,
                    name: None,
                },
                LocalDecl {
                    ty: close_ty,
                    name: None,
                },
                LocalDecl {
                    ty: f_fn_ty,
                    name: None,
                },
                LocalDecl {
                    ty: f_mut_ref,
                    name: None,
                },
                LocalDecl {
                    ty: body_fn_ty,
                    name: None,
                },
                LocalDecl {
                    ty: out_ty,
                    name: None,
                },
            ],
            captures: Vec::new(),
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::StorageLive(Local(1)),
                        span: sp(),
                    }],
                    terminator: Terminator::Switch {
                        discr: Operand::Copy(Place::local(Local(0))),
                        arms: vec![(0, BlockId(1))],
                        otherwise: BlockId(2),
                    },
                    terminator_span: sp(),
                },
                // bb1: unproved_reg (ForeignId(2))
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(1)),
                            value: Rvalue::ForeignFnRef(ForeignId(2)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Goto { target: BlockId(3) },
                    terminator_span: sp(),
                },
                // bb2: good_reg (ForeignId(0)) — later in block order!
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(1)),
                            value: Rvalue::ForeignFnRef(ForeignId(0)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Goto { target: BlockId(3) },
                    terminator_span: sp(),
                },
                // bb3: join -> ScopedCallback(Local(1), good_close, &mut f, body)
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(2)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(2)),
                                value: Rvalue::ForeignFnRef(ForeignId(1)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(3)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(3)),
                                value: Rvalue::FnRef(FnId(0)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(4)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(4)),
                                value: Rvalue::Ref {
                                    region: Region::Static,
                                    mutability: Mutability::Exclusive,
                                    place: Place::local(Local(3)),
                                },
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(5)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(5)),
                                value: Rvalue::FnRef(FnId(1)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(6)),
                            span: sp(),
                        },
                    ],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::ScopedCallback),
                        args: vec![
                            Operand::Copy(Place::local(Local(1))),
                            Operand::Copy(Place::local(Local(2))),
                            Operand::Copy(Place::local(Local(4))),
                            Operand::Copy(Place::local(Local(5))),
                        ],
                        dest: Place::local(Local(6)),
                        target: BlockId(4),
                        cancel: None,
                    },
                    terminator_span: sp(),
                },
                Block {
                    instructions: Vec::new(),
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 0,
                        }),
                    },
                    terminator_span: sp(),
                },
            ],
        },
        "reassign_after_call" => Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: bool_ty,
                    name: None,
                },
                LocalDecl {
                    ty: reg_ty,
                    name: None,
                },
                LocalDecl {
                    ty: close_ty,
                    name: None,
                },
                LocalDecl {
                    ty: f_fn_ty,
                    name: None,
                },
                LocalDecl {
                    ty: f_mut_ref,
                    name: None,
                },
                LocalDecl {
                    ty: body_fn_ty,
                    name: None,
                },
                LocalDecl {
                    ty: out_ty,
                    name: None,
                },
            ],
            captures: Vec::new(),
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(1)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(1)),
                                value: Rvalue::ForeignFnRef(ForeignId(2)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(2)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(2)),
                                value: Rvalue::ForeignFnRef(ForeignId(1)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(3)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(3)),
                                value: Rvalue::FnRef(FnId(0)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(4)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(4)),
                                value: Rvalue::Ref {
                                    region: Region::Static,
                                    mutability: Mutability::Exclusive,
                                    place: Place::local(Local(3)),
                                },
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(5)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(5)),
                                value: Rvalue::FnRef(FnId(1)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(6)),
                            span: sp(),
                        },
                    ],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::ScopedCallback),
                        args: vec![
                            Operand::Copy(Place::local(Local(1))),
                            Operand::Copy(Place::local(Local(2))),
                            Operand::Copy(Place::local(Local(4))),
                            Operand::Copy(Place::local(Local(5))),
                        ],
                        dest: Place::local(Local(6)),
                        target: BlockId(1),
                        cancel: None,
                    },
                    terminator_span: sp(),
                },
                // bb1: overwrite Local(1) with good_reg AFTER ScopedCallback!
                Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Assign {
                            dest: Place::local(Local(1)),
                            value: Rvalue::ForeignFnRef(ForeignId(0)),
                        },
                        span: sp(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 0,
                        }),
                    },
                    terminator_span: sp(),
                },
            ],
        },
        "same_root_alias" | "disjoint_alias_control" => {
            // Identical call & lifetime structure; the only difference is whether
            // shared borrow `Local(4)` borrows the same root `Local(3)` as the
            // exclusive `&mut Local(3)` published to `ScopedCallback`, or a
            // provably disjoint local `Local(9)`.
            let shared_target = if mode == "same_root_alias" {
                Local(3)
            } else {
                Local(9)
            };
            Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: bool_ty,
                        name: None,
                    },
                    LocalDecl {
                        ty: reg_ty,
                        name: None,
                    },
                    LocalDecl {
                        ty: close_ty,
                        name: None,
                    },
                    LocalDecl {
                        ty: f_fn_ty,
                        name: None,
                    },
                    LocalDecl {
                        ty: f_shared_ref,
                        name: None,
                    },
                    LocalDecl {
                        ty: f_mut_ref,
                        name: None,
                    },
                    LocalDecl {
                        ty: body_fn_ty,
                        name: None,
                    },
                    LocalDecl {
                        ty: out_ty,
                        name: None,
                    },
                    LocalDecl {
                        ty: f_fn_ty,
                        name: None,
                    },
                    LocalDecl {
                        ty: f_fn_ty,
                        name: None,
                    },
                ],
                captures: Vec::new(),
                closure: None,
                result: i32_ty,
                param_env: ParamEnv::default(),
                drop_flags: Vec::new(),
                blocks: vec![
                    Block {
                        instructions: vec![
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(1)),
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(1)),
                                    value: Rvalue::ForeignFnRef(ForeignId(0)),
                                },
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(2)),
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(2)),
                                    value: Rvalue::ForeignFnRef(ForeignId(1)),
                                },
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(3)),
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(3)),
                                    value: Rvalue::FnRef(FnId(0)),
                                },
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(9)),
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(9)),
                                    value: Rvalue::FnRef(FnId(0)),
                                },
                                span: sp(),
                            },
                            // Create shared borrow Local(4) = &shared_target
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(4)),
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(4)),
                                    value: Rvalue::Ref {
                                        region: Region::Static,
                                        mutability: Mutability::Shared,
                                        place: Place::local(shared_target),
                                    },
                                },
                                span: sp(),
                            },
                            // Create exclusive borrow Local(5) = &mut Local(3) while Local(4) is live
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(5)),
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(5)),
                                    value: Rvalue::Ref {
                                        region: Region::Static,
                                        mutability: Mutability::Exclusive,
                                        place: Place::local(Local(3)),
                                    },
                                },
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(6)),
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(6)),
                                    value: Rvalue::FnRef(FnId(1)),
                                },
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(7)),
                                span: sp(),
                            },
                        ],
                        terminator: Terminator::Call {
                            callee: Callee::Intrinsic(IntrinsicOp::ScopedCallback),
                            args: vec![
                                Operand::Copy(Place::local(Local(1))),
                                Operand::Copy(Place::local(Local(2))),
                                Operand::Move(Place::local(Local(5))),
                                Operand::Copy(Place::local(Local(6))),
                            ],
                            dest: Place::local(Local(7)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: sp(),
                    },
                    // bb1: read through shared alias Local(4) AFTER exclusive publication!
                    Block {
                        instructions: vec![
                            Instruction {
                                kind: InstructionKind::StorageLive(Local(8)),
                                span: sp(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(8)),
                                    value: Rvalue::Use(Operand::Copy(Place {
                                        root: PlaceRoot::Local(Local(4)),
                                        projections: vec![Projection::Deref],
                                    })),
                                },
                                span: sp(),
                            },
                        ],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Int {
                                ty: IntTy::I32,
                                bits: 0,
                            }),
                        },
                        terminator_span: sp(),
                    },
                ],
            }
        }
        other => unreachable!("unknown mode {other}"),
    };

    Module {
        tys,
        functions: vec![
            Function {
                name: "cb_fn".into(),
                sig: FnSig {
                    type_params: 0,
                    region_params: 0,
                    params: vec![i32_ty],
                    result: i32_ty,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: Vec::new(),
                },
                body: f_body,
            },
            Function {
                name: "body_fn".into(),
                sig: FnSig {
                    type_params: 0,
                    region_params: 0,
                    params: Vec::new(),
                    result: i32_ty,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: Vec::new(),
                },
                body: b_body,
            },
            Function {
                name: "main".into(),
                sig: FnSig {
                    type_params: 0,
                    region_params: 0,
                    params: vec![bool_ty],
                    result: i32_ty,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: Vec::new(),
                },
                body: main_body,
            },
        ],
        foreigns: vec![
            ForeignDecl {
                symbol: "good_reg".into(),
                abi: ForeignAbi::C,
                params: vec![cb_ty, handle],
                result: handle,
                noescape_params: Vec::new(),
                callback_role: Some(ForeignCallbackRole::ScopedRegister),
                thread_async_driver_transfer: false,
            },
            ForeignDecl {
                symbol: "good_close".into(),
                abi: ForeignAbi::C,
                params: vec![handle],
                result: unit,
                noescape_params: Vec::new(),
                callback_role: Some(ForeignCallbackRole::ScopedClose),
                thread_async_driver_transfer: false,
            },
            ForeignDecl {
                symbol: "unproved_reg".into(),
                abi: ForeignAbi::C,
                params: vec![cb_ty, handle],
                result: handle,
                noescape_params: Vec::new(),
                callback_role: None,
                thread_async_driver_transfer: false,
            },
        ],
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    }
}

#[test]
fn raw_wire_scoped_provider_branch_join_must_fail_closed() {
    let module = build_scoped_provider_module("branch_join");
    assert_rejected_with(verify_wire_roundtrip(&module), "path-unique");
}

#[test]
fn raw_wire_scoped_provider_reassignment_after_call_must_fail_closed() {
    let module = build_scoped_provider_module("reassign_after_call");
    assert_rejected_with(verify_wire_roundtrip(&module), "role mismatch");
}

#[test]
fn raw_wire_scoped_callback_same_root_alias_conflict_is_rejected_by_checker() {
    // 1. Disjoint control: identical ScopedCallback call & bb0->bb1 lifetime shape,
    //    but shared borrow Local(4) = &Local(9) is provably disjoint from exclusive
    //    borrow Local(5) = &mut Local(3). Both Solver and Verify MUST accept.
    let disjoint_module = build_scoped_provider_module("disjoint_alias_control");
    let disjoint_bytes = encode_module(&disjoint_module);
    let disjoint_decoded = decode_module(&disjoint_bytes).expect("disjoint wire decode");
    let disjoint_cert = zil_solve::solve_module(&disjoint_decoded, Budget { steps: 200_000 })
        .expect("disjoint alias control must solve without violation");
    assert_eq!(
        disjoint_cert.functions.len(),
        3,
        "disjoint control certificate must cover all 3 functions"
    );
    check_module(
        disjoint_decoded.clone(),
        &disjoint_cert,
        CheckBudget { steps: 200_000 },
    )
    .expect("zil-check must accept disjoint alias control with solved certificate");

    // Also prove that a structurally complete 3-function certificate (no missing
    // function certificates, matching the join-free bb0->bb1 CFG) is accepted by
    // zil-check on the disjoint control — so rejection below cannot be caused by
    // CertificateShape("one function certificate per function").
    let structural_cert = Certificate {
        functions: (0..3)
            .map(|i| FunctionCertificate {
                function: FnId::from_index(i),
                invariants: Vec::new(),
            })
            .collect(),
    };
    check_module(
        disjoint_decoded,
        &structural_cert,
        CheckBudget { steps: 200_000 },
    )
    .expect("zil-check must accept disjoint alias control with structurally complete certificate");

    // 2. Same-root alias conflict: identical module, changing ONLY Local(4)'s
    //    borrowed target from Local(9) to Local(3) (same root as `&mut Local(3)`).
    let conflict_module = build_scoped_provider_module("same_root_alias");
    let conflict_bytes = encode_module(&conflict_module);
    let conflict_decoded = decode_module(&conflict_bytes).expect("conflict wire decode");

    // 2a. Solver must reject specifically with ViolationKind::LoanConflict on Local(3) in FnId(2).
    let solve_errors = zil_solve::solve_module(&conflict_decoded, Budget { steps: 200_000 })
        .expect_err("solver must reject live shared alias across exclusive &mut f publication");
    assert!(
        solve_errors.iter().any(|err| matches!(
            err,
            SolveError::Violation {
                function,
                violation: Violation {
                    kind: ViolationKind::LoanConflict { place, .. },
                    ..
                },
            } if *function == FnId(2) && *place == Place::local(Local(3))
        )),
        "expected Solver SolveError::Violation(LoanConflict on Local(3) in FnId(2)), got: {solve_errors:?}"
    );

    // 2b. Verify must independently reject with CheckErrorKind::Violation(LoanConflict on Local(3))
    //     when given a structurally valid 3-function certificate (both `structural_cert` and the
    //     certificate solved from the isomorphic disjoint control).
    for (label, cert) in [
        ("structural_cert", &structural_cert),
        ("disjoint_solved_cert", &disjoint_cert),
    ] {
        let check_err = check_module(
            conflict_decoded.clone(),
            cert,
            CheckBudget { steps: 200_000 },
        )
        .expect_err("zil-check must independently reject same-root alias conflict");
        assert_eq!(
            check_err.function,
            FnId(2),
            "expected rejection in FnId(2) with {label}, got {check_err:?}"
        );
        assert!(
            matches!(
                check_err.kind,
                CheckErrorKind::Violation(Violation {
                    kind: ViolationKind::LoanConflict { ref place, .. },
                    ..
                }) if *place == Place::local(Local(3))
            ),
            "expected zil-check Violation(LoanConflict on Local(3)) with {label}, got: {check_err:?}"
        );
    }
}

#[test]
fn raw_wire_owned_dyn_erased_mutex_guard_cannot_cross_thread_scope() {
    // Erase MutexGuard<i32> behind `OwnedDyn<dyn Locker>` and attempt to pass it across `ThreadScope`.
    let mut tys = TyTable::new();
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let bool_ty = tys.intern(TyKind::Bool);
    let guard_ty = tys.intern(TyKind::Opaque(OpaqueTy::MutexGuard {
        region: Region::Static,
        payload: i32_ty,
    }));
    let dyn_id: DynId = tys.add_dyn(DynDef {
        name: "Locker".into(),
        slots: vec![DynSlotSig {
            name: "ping".into(),
            receiver: Some(Mutability::Shared),
            params: Vec::new(),
            result: i32_ty,
        }],
    });
    let owned_dyn_ty = tys.intern(TyKind::OwnedDyn {
        def: dyn_id,
        regions: Vec::new(),
        context: None,
    });
    let worker_fn_ty = tys.intern(TyKind::FnPtr {
        params: vec![owned_dyn_ty],
        result: i32_ty,
    });
    let empty_args = tys.intern_args(GenericArgs {
        tys: Vec::new(),
        regions: Vec::new(),
    });
    let cdef_id = tys.add_closure(ClosureDef {
        kind: ClosureKind::Fn,
        type_params: 0,
        captures: Vec::new(),
        params: Vec::new(),
        result: i32_ty,
    });
    let parent_closure_ty = tys.intern(TyKind::Closure {
        def: cdef_id,
        args: empty_args,
    });
    let out_ty = tys.intern(TyKind::Tuple(vec![bool_ty, i32_ty, i32_ty]));

    let guard_ref_ty = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: guard_ty,
    });

    // FnId(0): ping(&MutexGuard<i32>) -> i32
    let ping_fn = Function {
        name: "guard_ping".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 0,
            params: vec![guard_ref_ty],
            result: i32_ty,
            role: FnRole::DynSlotImpl {
                dyn_id,
                concrete_ty: guard_ty,
                slot: 0,
            },
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: guard_ref_ty,
                name: None,
            }],
            captures: Vec::new(),
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![Block {
                instructions: Vec::new(),
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: sp(),
            }],
        },
    };

    // FnId(1): worker(d: OwnedDyn<dyn Locker>) -> i32
    let worker_fn = Function {
        name: "worker".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 0,
            params: vec![owned_dyn_ty],
            result: i32_ty,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: owned_dyn_ty,
                name: None,
            }],
            captures: Vec::new(),
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::Drop {
                        place: Place::local(Local(0)),
                    },
                    span: sp(),
                }],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: sp(),
            }],
        },
    };

    // FnId(2): parent_closure() -> i32
    let parent_fn = Function {
        name: "parent".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 0,
            params: Vec::new(),
            result: i32_ty,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 0,
            locals: Vec::new(),
            captures: Vec::new(),
            closure: Some(cdef_id),
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![Block {
                instructions: Vec::new(),
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: 0,
                    }),
                },
                terminator_span: sp(),
            }],
        },
    };

    // FnId(3): bad_transfer(d: OwnedDyn<dyn Locker>) -> i32
    let bad_fn = Function {
        name: "bad_transfer".into(),
        sig: FnSig {
            type_params: 0,
            region_params: 0,
            params: vec![owned_dyn_ty],
            result: i32_ty,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: Vec::new(),
        },
        body: Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: owned_dyn_ty,
                    name: None,
                },
                LocalDecl {
                    ty: worker_fn_ty,
                    name: None,
                },
                LocalDecl {
                    ty: parent_closure_ty,
                    name: None,
                },
                LocalDecl {
                    ty: out_ty,
                    name: None,
                },
            ],
            captures: Vec::new(),
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![
                Block {
                    instructions: vec![
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(1)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(1)),
                                value: Rvalue::FnRef(FnId(1)),
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(2)),
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(2)),
                                value: Rvalue::Aggregate {
                                    kind: zil_ir::body::AggregateKind::Closure {
                                        def: cdef_id,
                                        args: empty_args,
                                    },
                                    operands: Vec::new(),
                                },
                            },
                            span: sp(),
                        },
                        Instruction {
                            kind: InstructionKind::StorageLive(Local(3)),
                            span: sp(),
                        },
                    ],
                    terminator: Terminator::Call {
                        callee: Callee::Intrinsic(IntrinsicOp::ThreadScope),
                        args: vec![
                            Operand::Copy(Place::local(Local(1))),
                            Operand::Move(Place::local(Local(0))),
                            Operand::Move(Place::local(Local(2))),
                        ],
                        dest: Place::local(Local(3)),
                        target: BlockId(1),
                        cancel: None,
                    },
                    terminator_span: sp(),
                },
                Block {
                    instructions: Vec::new(),
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 0,
                        }),
                    },
                    terminator_span: sp(),
                },
            ],
        },
    };

    let module = Module {
        tys,
        functions: vec![ping_fn, worker_fn, parent_fn, bad_fn],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: vec![DynImpl {
            dyn_id,
            concrete_ty: guard_ty,
            methods: vec![FnId(0)],
        }],
    };
    assert_rejected_with(verify_wire_roundtrip(&module), "MutexGuard");
}

#[test]
fn deep_propagation_chain_over_4_hops_executes_natively_on_dual_backends() {
    // 6-hop local forwarding chain of a `resume(transfer)` Task across `thread_scope`:
    // previously a fixed 4-round propagation loop would fail on >4 hops.
    let dir = std::env::temp_dir().join(format!("zil-l3-prov-deep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = r#"
fn worker() -> i32 contract { effects(suspend); resume(transfer) } {
    task_yield()
    return 41
}
fn runner(give t: Task<i32>) -> i32 {
    var task = give t
    var done = 0
    loop {
        match task_poll(&mut task) {
            .ok(poll) => {
                match poll {
                    Poll.Pending() => { continue },
                    Poll.Ready(v) => { done = v; break }
                }
            },
            .err(_) => { break }
        }
    }
    task_request_cancel(&mut task)
    loop {
        match task_poll_close(&mut task) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give task)
    return done + 1
}
fn main() -> TaskCreateError!i32 {
    let t0 = task_create(worker)?
    let t1 = give t0
    let t2 = give t1
    let t3 = give t2
    let t4 = give t3
    let t5 = give t4
    let t6 = give t5
    match thread_scope(runner, give t6, () => 0) {
        TaskThreadScopeResult.Completed(c, p) => { return c + p - 42 },
        TaskThreadScopeResult.Failed(f) => {
            var ft = give f
            task_request_cancel(&mut ft)
            loop {
                match task_poll_close(&mut ft) {
                    Poll.Pending() => { continue },
                    Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give ft)
            return 2
        }
    }
}
"#;
    let entry: PathBuf = dir.join("deep.zil");
    std::fs::write(&entry, src).expect("write");

    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }
    for (label, backend, opt) in configs {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(&entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "deep 6-hop transfer failed on {label}: {:?}",
            report.diagnostics
        );
        let out = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("run");
        assert_eq!(out.status.code(), Some(0), "exit mismatch on {label}");
    }
    let _ = Path::new(&dir);
}

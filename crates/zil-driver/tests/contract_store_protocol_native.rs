//! End-to-end native, source-rejection, and Raw/Wire verification tests for G6.3-C
//! `OwnedStore` state-protocol joint semantics:
//! - `StorePush` distinguishes `.ok` success from `.err(elem)` OOM failure recovery
//!   (restoring pre-push store state & invalidation while preserving exact returned handle state);
//! - `StorePop` distinguishes `.ok(elem)` from `.err(())` empty-container recovery
//!   (failing closed on empty store pop, proving store empty & uninvalidated on `.err`);
//! - `StoreBorrow` (`store_at` / `store_at_mut`) does NOT equal actual modification
//!   (borrowing `&mut elem` without writing does not invalidate the store or widen its summary;
//!   actual mutation through `&mut elem` invalidates the store and updates its summary).

mod contract_semantics_support;

use contract_semantics_support::{
    build_and_run_dual_with_c, check_rejected, temp_dir, write_source,
};
use zil_base::ResultCategory;

#[test]
fn store_push_pop_recovery_and_readonly_mut_borrow_execute_natively_on_dual_backends() {
    let dir = temp_dir("store-protocol-positive-native");
    let src = r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState, id: i32 }

fn require_open(ch: &Channel)
    contract { requires(state(ch, ChanState.Open)); effects(invalidates()); }
{}

fn require_closed(ch: &Channel)
    contract { requires(state(ch, ChanState.Closed)); effects(invalidates()); }
{}

fn close_channel(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Closed));
        effects(invalidates(ch));
    }
{
    ch.state = ChanState.Closed
}

// Borrowing `&mut Channel` from a store and ONLY reading it must NOT violate `effects(invalidates())`!
fn inspect_mut_borrow_without_writing(s: Store<Channel>) -> i32
    contract { effects(invalidates()); }
{
    let b = store_at_mut(s, 0)
    return b.id
}

// Pushing an Open handle into a store that only holds Closed handles:
// - With capacity 1, the second push deterministically hits `.err(returned)` at runtime!
// - On `.err(returned)` (OOM), `returned` keeps exact Open state AND `s` stays Closed-only!
// - On `.ok`, `s` holds Open | Closed, and draining until `.err` proves `s` empty again.
fn push_open_with_oom_recovery(reserve_cap: i64) -> i32 {
    let s = store_new()
    if !store_reserve(s, reserve_cap) {
        return 1
    }
    match store_push(s, Channel { state: ChanState.Closed, id: 10 }) {
        .err(_) => { return 2 }
        .ok(_) => {}
    }
    let incoming = Channel { state: ChanState.Open, id: 20 }
    match store_push(s, incoming) {
        .err(returned) => {
            // OOM recovery: returned handle is still proved Open, and store still holds only Closed!
            require_open(&returned)
            match store_pop(s) {
                .ok(prev) => {
                    require_closed(&prev)
                    if prev.id + returned.id == 30 {
                        return 0
                    }
                    return 3
                }
                .err(_) => { return 4 }
            }
        }
        .ok(_) => {
            // Success path: pop returns the second item (id 20), then first item (id 10).
            match store_pop(s) {
                .ok(ch) => {
                    if ch.id != 20 { return 5 }
                }
                .err(_) => { return 6 }
            }
            match store_pop(s) {
                .ok(ch) => {
                    if ch.id != 10 { return 7 }
                }
                .err(_) => { return 8 }
            }
            // Empty-container recovery: on `.err`, store is proved empty so pushing Closed
            // and popping yields exact Closed!
            match store_pop(s) {
                .ok(_) => { return 9 }
                .err(_) => {
                    match store_push(s, Channel { state: ChanState.Closed, id: 30 }) {
                        .err(_) => { return 10 }
                        .ok(_) => {}
                    }
                    match store_pop(s) {
                        .ok(last) => {
                            require_closed(&last)
                            return last.id - 30
                        }
                        .err(_) => { return 11 }
                    }
                }
            }
        }
    }
}

fn push_pop_exact_open_and_readonly_borrow() -> i32 {
    let s = store_new()
    if !store_reserve(s, 2) {
        return 12
    }
    match store_push(s, Channel { state: ChanState.Open, id: 42 }) {
        .err(_) => { return 13 }
        .ok(_) => {}
    }
    let seen = inspect_mut_borrow_without_writing(s)
    if seen != 42 { return 14 }
    // Read-only `&mut` borrow did not widen `s`: popping still proves Open!
    match store_pop(s) {
        .ok(ch) => {
            var c = ch
            require_open(&c)
            close_channel(&mut c)
            require_closed(&c)
            return 0
        }
        .err(_) => { return 15 }
    }
}

fn main() -> i32 {
    // Run BOTH deterministic OOM recovery path (cap=1) AND success + empty-pop recovery (cap=4)!
    let a = push_open_with_oom_recovery(1)
    if a != 0 { return a }
    let b = push_open_with_oom_recovery(4)
    if b != 0 { return b + 100 }
    return push_pop_exact_open_and_readonly_borrow()
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn store_protocol_negative_controls_are_rejected() {
    let dir = temp_dir("store-protocol-negative");

    // 1. StorePush `.ok` violates `effects(invalidates())`.
    check_rejected(
        &dir,
        "store_push_ok_violates_empty_invalidates",
        r#"
fn bad(s: Store<i32>) contract { effects(invalidates()); } {
    match store_push(s, 7) {
        .ok(_) => {}
        .err(_) => {}
    }
}
fn main() -> i32 {
    let s = store_new()
    bad(s)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. StorePush `.err(returned)` preserves Open state; falsely requiring Closed must reject!
    check_rejected(
        &dir,
        "store_push_err_returned_wrong_state_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_closed(ch: &Channel) contract { requires(state(ch, ChanState.Closed)); } {}
fn bad() {
    let s = store_new()
    match store_push(s, Channel { state: ChanState.Open }) {
        .ok(_) => {}
        .err(returned) => {
            require_closed(&returned)
        }
    }
}
fn main() -> i32 {
    bad()
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. StorePop on a proved-empty store MUST fail closed (never vacuously satisfy Closed!).
    check_rejected(
        &dir,
        "store_pop_empty_must_fail_closed",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_closed(ch: &Channel) contract { requires(state(ch, ChanState.Closed)); } {}
fn bad() {
    let s = store_new()
    match store_pop(s) {
        .ok(ch) => {
            require_closed(&ch)
        }
        .err(_) => {}
    }
}
fn main() -> i32 {
    bad()
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. StorePop from mixed store (Open + Closed) falsely claiming Open must reject!
    check_rejected(
        &dir,
        "store_pop_mixed_state_claim_open_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_open(ch: &Channel) contract { requires(state(ch, ChanState.Open)); } {}
fn bad() {
    let s = store_new()
    match store_push(s, Channel { state: ChanState.Open }) {
        .ok(_) => {}
        .err(_) => {}
    }
    match store_push(s, Channel { state: ChanState.Closed }) {
        .ok(_) => {}
        .err(_) => {}
    }
    match store_pop(s) {
        .ok(ch) => { require_open(&ch) }
        .err(_) => {}
    }
}
fn main() -> i32 {
    bad()
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 5. StorePop from unknown entry store parameter claiming Closed must reject!
    check_rejected(
        &dir,
        "store_pop_unknown_param_store_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_closed(ch: &Channel) contract { requires(state(ch, ChanState.Closed)); } {}
fn bad(s: Store<Channel>) {
    match store_pop(s) {
        .ok(ch) => { require_closed(&ch) }
        .err(_) => {}
    }
}
fn main() -> i32 {
    let s = store_new()
    bad(s)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 6. StoreBorrow Exclusive WITH actual write MUST reject `effects(invalidates())`!
    check_rejected(
        &dir,
        "store_borrow_mut_actual_write_violates_empty_invalidates",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn bad(s: Store<Channel>) contract { effects(invalidates()); } {
    let b = store_at_mut(s, 0)
    b.state = ChanState.Closed
}
fn main() -> i32 {
    let s = store_new()
    match store_push(s, Channel { state: ChanState.Open }) {
        .ok(_) => {}
        .err(_) => {}
    }
    bad(s)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 7. StoreBorrow Exclusive WITH actual state transition updates store summary:
    //    subsequent `store_pop` cannot claim elements are still Open!
    check_rejected(
        &dir,
        "store_borrow_mut_transition_widens_store_summary",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_open(ch: &Channel) contract { requires(state(ch, ChanState.Open)); } {}
fn bad() {
    let s = store_new()
    match store_push(s, Channel { state: ChanState.Open }) {
        .ok(_) => {}
        .err(_) => {}
    }
    let b = store_at_mut(s, 0)
    b.state = ChanState.Closed
    match store_pop(s) {
        .ok(ch) => { require_open(&ch) }
        .err(_) => {}
    }
}
fn main() -> i32 {
    bad()
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_store_protocol_forged_certificates_are_rejected() {
    use zil_ir::body::{
        Block, BlockId, Body, Callee, ConstValue, FnId, FnRole, FnSig, Function, Instruction,
        InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl, Module, Mutability, OpaqueTy,
        Operand, ParamEnv, Place, Projection, ProtocolStateSet, Region, ResumeMode, Rvalue,
        StateContractClause, Terminator, TyKind, TyTable, decode_module, encode_module,
    };

    // Case 1: Raw/Wire `StorePop` on empty `OwnedStore<Managed<Task>>` claiming `TaskFinishDestroy`
    // on the `.ok` branch MUST be rejected (empty store fails closed, never vacuously Closed).
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let bool_ty = tys.intern(TyKind::Bool);
        let task_inner = tys.intern(TyKind::Opaque(OpaqueTy::Task {
            result: unit_ty,
            context: None,
        }));
        let managed_task = tys.intern(TyKind::Managed(task_inner));
        let store_inner = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore {
            element: managed_task,
        }));
        let managed_store = tys.intern(TyKind::Managed(store_inner));
        let desc_store = tys.intern(TyKind::Desc(store_inner));
        let pop_eu = tys.intern(TyKind::ErrorUnion {
            ok: managed_task,
            err: unit_ty,
        });

        let func = Function {
            name: "empty_pop_forged_destroy".into(),
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
                        ty: managed_store,
                        name: Some("s".into()),
                    },
                    LocalDecl {
                        ty: desc_store,
                        name: Some("d".into()),
                    },
                    LocalDecl {
                        ty: pop_eu,
                        name: Some("eu".into()),
                    },
                    LocalDecl {
                        ty: bool_ty,
                        name: Some("arm".into()),
                    },
                    LocalDecl {
                        ty: managed_task,
                        name: Some("popped".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("tmp".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::Intrinsic(IntrinsicOp::StoreNew),
                            args: vec![],
                            dest: Place::local(Local(0)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(1)),
                                value: Rvalue::Describe {
                                    place: Place::local(Local(0)),
                                },
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Call {
                            callee: Callee::Intrinsic(IntrinsicOp::StorePop),
                            args: vec![Operand::Copy(Place::local(Local(1)))],
                            dest: Place::local(Local(2)),
                            target: BlockId(2),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(3)),
                                value: Rvalue::ErrorArm(Place::local(Local(2))),
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Switch {
                            discr: Operand::Copy(Place::local(Local(3))),
                            arms: vec![(1, BlockId(3)), (0, BlockId(4))],
                            otherwise: BlockId(5),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(4)),
                                value: Rvalue::Use(Operand::Move(Place {
                                    root: zil_ir::body::PlaceRoot::Local(Local(2)),
                                    projections: vec![Projection::ErrorArm(true)],
                                })),
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Call {
                            callee: Callee::Intrinsic(IntrinsicOp::TaskFinishDestroy),
                            args: vec![Operand::Move(Place::local(Local(4)))],
                            dest: Place::local(Local(5)),
                            target: BlockId(6),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![
                            Instruction {
                                kind: InstructionKind::Drop {
                                    place: Place::local(Local(2)),
                                },
                                span: zil_base::Span::default(),
                            },
                            Instruction {
                                kind: InstructionKind::Drop {
                                    place: Place::local(Local(0)),
                                },
                                span: zil_base::Span::default(),
                            },
                        ],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Unreachable,
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Drop {
                                place: Place::local(Local(0)),
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
        };
        let m = Module {
            tys,
            functions: vec![func],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        };
        let decoded = decode_module(&encode_module(&m)).expect("decode empty pop raw wire");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve empty pop raw wire");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject TaskFinishDestroy on task popped from empty store",
                );
        assert_eq!(err.function, FnId(0));
        assert_eq!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(
                "finish_destroy called on protocol handle not proved Closed"
            )
        );
    }

    // Case 2: Raw/Wire `StorePush` followed by an intervening `StoreReserve` on the same store
    // BEFORE branching on the push's `.err` arm must NOT falsely retract the `StoreReserve`
    // invalidation when declaring `effects(invalidates())`.
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let bool_ty = tys.intern(TyKind::Bool);
        let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
        let store_inner = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore { element: i64_ty }));
        let desc_store = tys.intern(TyKind::Desc(store_inner));
        let push_eu = tys.intern(TyKind::ErrorUnion {
            ok: unit_ty,
            err: i64_ty,
        });

        let func = Function {
            name: "intervening_reserve_not_retracted_by_push_err".into(),
            sig: FnSig {
                params: vec![desc_store],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::InvalidationUpperBound],
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: desc_store,
                        name: Some("s".into()),
                    },
                    LocalDecl {
                        ty: push_eu,
                        name: Some("eu".into()),
                    },
                    LocalDecl {
                        ty: bool_ty,
                        name: Some("res_ok".into()),
                    },
                    LocalDecl {
                        ty: bool_ty,
                        name: Some("arm".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::Intrinsic(IntrinsicOp::StorePush),
                            args: vec![
                                Operand::Copy(Place::local(Local(0))),
                                Operand::Const(ConstValue::Int {
                                    ty: IntTy::I64,
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
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::Intrinsic(IntrinsicOp::StoreReserve),
                            args: vec![
                                Operand::Copy(Place::local(Local(0))),
                                Operand::Const(ConstValue::Int {
                                    ty: IntTy::I64,
                                    bits: 16,
                                }),
                            ],
                            dest: Place::local(Local(2)),
                            target: BlockId(2),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(3)),
                                value: Rvalue::ErrorArm(Place::local(Local(1))),
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Switch {
                            discr: Operand::Copy(Place::local(Local(3))),
                            arms: vec![(0, BlockId(3))],
                            otherwise: BlockId(4),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Return {
                            value: Operand::Const(ConstValue::Unit),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Trap(zil_ir::body::TrapKind::Explicit),
                        terminator_span: zil_base::Span::default(),
                    },
                ],
            },
        };
        let m = Module {
            tys,
            functions: vec![func],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        };
        let decoded = decode_module(&encode_module(&m)).expect("decode intervening reserve");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve intervening reserve");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject empty invalidates() when StoreReserve ran before switching on StorePush .err",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // Case 3: Raw/Wire `StoreBorrow { Exclusive }` followed by `TaskRequestCancel` on the
    // borrowed handle inside `effects(invalidates())` MUST be rejected, whereas borrowing
    // without writing MUST be accepted!
    let build_borrow_mod = |do_mutate: bool| {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let _i64_ty = tys.intern(TyKind::Int(IntTy::I64));
        let task_inner = tys.intern(TyKind::Opaque(OpaqueTy::Task {
            result: unit_ty,
            context: None,
        }));
        let managed_task = tys.intern(TyKind::Managed(task_inner));
        let ref_mut_task = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: managed_task,
        });
        let store_inner = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore {
            element: managed_task,
        }));
        let desc_store = tys.intern(TyKind::Desc(store_inner));
        let mut blocks = vec![Block {
            instructions: vec![],
            terminator: Terminator::Call {
                callee: Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                    mutability: Mutability::Exclusive,
                }),
                args: vec![
                    Operand::Copy(Place::local(Local(0))),
                    Operand::Const(ConstValue::Int {
                        ty: IntTy::I64,
                        bits: 0,
                    }),
                ],
                dest: Place::local(Local(1)),
                target: BlockId(1),
                cancel: None,
            },
            terminator_span: zil_base::Span::default(),
        }];
        if do_mutate {
            blocks.push(Block {
                instructions: vec![],
                terminator: Terminator::Call {
                    callee: Callee::Direct(FnId(1)),
                    args: vec![Operand::Move(Place::local(Local(1)))],
                    dest: Place::local(Local(2)),
                    target: BlockId(2),
                    cancel: None,
                },
                terminator_span: zil_base::Span::default(),
            });
            blocks.push(Block {
                instructions: vec![],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Unit),
                },
                terminator_span: zil_base::Span::default(),
            });
        } else {
            blocks.push(Block {
                instructions: vec![],
                terminator: Terminator::Return {
                    value: Operand::Const(ConstValue::Unit),
                },
                terminator_span: zil_base::Span::default(),
            });
        }
        let caller = Function {
            name: "borrow_task_from_store".into(),
            sig: FnSig {
                params: vec![desc_store],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::InvalidationUpperBound],
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: desc_store,
                        name: Some("s".into()),
                    },
                    LocalDecl {
                        ty: ref_mut_task,
                        name: Some("b".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("tmp".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks,
            },
        };
        let mutator = Function {
            name: "cancel_task".into(),
            sig: FnSig {
                params: vec![ref_mut_task],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::ProvidesState {
                    param: 0,
                    states: ProtocolStateSet(ProtocolStateSet::CLOSING | ProtocolStateSet::CLOSED),
                }],
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: ref_mut_task,
                        name: Some("t".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("tmp".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::Intrinsic(IntrinsicOp::TaskRequestCancel),
                            args: vec![Operand::Move(Place::local(Local(0)))],
                            dest: Place::local(Local(1)),
                            target: BlockId(1),
                            cancel: None,
                        },
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
        };
        Module {
            tys,
            functions: vec![caller, mutator],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        }
    };

    // 3a. Read-only exclusive borrow without mutation satisfies `effects(invalidates())`.
    {
        let m = build_borrow_mod(false);
        let decoded = decode_module(&encode_module(&m)).expect("decode borrow readonly");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve borrow readonly");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("read-only StoreBorrow Exclusive must not invalidate store");
    }

    // 3b. Exclusive borrow followed by mutating call MUST reject `effects(invalidates())`.
    {
        let m = build_borrow_mod(true);
        let decoded = decode_module(&encode_module(&m)).expect("decode borrow mutate");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve borrow mutate");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("mutating through StoreBorrow Exclusive must invalidate store");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
}

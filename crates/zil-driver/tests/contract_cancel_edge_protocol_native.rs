//! End-to-end native, source-rejection, and Raw/Wire verification tests for
//! call cancellation-edge (`Terminator::Call { cancel: Some(_) }`) state-protocol
//! joint semantics:
//! - On a cancel edge, the verifier neither assumes the pre-call state survived
//!   untouched nor applies unreached normal-return `provides(...)` postconditions;
//! - All possible pre-cancel side effects, invalidations, hidden rollbacks, and
//!   `Operand::Move` ownership transfers are preserved on the cancel edge;
//! - Pure read-only `effects(invalidates())` suspending calls preserve shared
//!   handle states across cancel;
//! - Nested suspending calls attach real `cancel` unwind edges in `zil-typeck`
//!   and propagate cancellation unwind cleanly on both native backends.

mod contract_semantics_support;

use contract_semantics_support::{
    build_and_run_dual_with_c, check_rejected, temp_dir, write_source,
};
use zil_base::ResultCategory;

#[test]
fn cancel_edge_unwinds_nested_suspending_calls_and_preserves_readonly_protocol_native() {
    let dir = temp_dir("cancel-edge-protocol-positive-native");
    let src = r#"
enum ConnState { Open, Closing, Closed }
struct Conn { state: ConnState, steps: i32 }

fn require_open(c: &Conn)
    contract { requires(state(c, ConnState.Open)); effects(invalidates()); }
{}

fn require_any_and_close(c: &mut Conn, log: &mut i32)
    contract {
        requires(state(c, ConnState.Open, ConnState.Closing, ConnState.Closed));
        provides(state(c, ConnState.Closed));
        effects(invalidates(c, log));
    }
{
    match c.state {
        ConnState.Open() => { *log = *log * 10 + 1 }
        ConnState.Closing() => { *log = *log * 10 + 2 }
        ConnState.Closed() => { *log = *log * 10 + 3 }
    }
    c.state = ConnState.Closed
}

// Suspending helper that mutates `c` from Open -> Closing BEFORE suspending,
// and only reaches Closed on normal return.
fn transition_and_yield(c: &mut Conn, log: &mut i32)
    contract {
        requires(state(c, ConnState.Open));
        provides(state(c, ConnState.Closed));
        effects(invalidates(c, log));
    }
{
    defer { *log = *log * 10 + 7 }
    c.state = ConnState.Closing
    c.steps += 1
    task_yield()
    c.state = ConnState.Closed
    c.steps += 10
}

// Pure read-only suspending helper: does not mutate `c`, so `Open` holds even on cancel.
fn peek_and_yield(c: &Conn) -> i32
    contract {
        requires(state(c, ConnState.Open));
        provides(state(c, ConnState.Open));
        effects(invalidates());
    }
{
    let s = c.steps
    task_yield()
    return s + 1
}

// Consuming suspending helper: receives ownership of a ByteBuffer by move before yielding.
fn consume_buf_and_yield(buf: ByteBuffer, log: &mut i32) {
    defer { *log = *log * 10 + 5 }
    if bytes_len(buf) > 0 {
        task_yield()
    }
    *log = *log * 10 + 9
}

// Worker 1: calls mutating suspending helper, gets cancelled mid-flight.
// Outer defer accepts any state (Open|Closing|Closed), sees Closing (digit 2),
// and post-call code (*log = ... + 9) MUST NOT execute after nested cancel!
fn nested_mutating_worker(log: &mut i32) -> i32 {
    var c = Conn { state: ConnState.Open, steps: 0 }
    defer { require_any_and_close(&mut c, log) }
    transition_and_yield(&mut c, log)
    *log = *log * 10 + 9
    return c.steps
}

// Worker 2: calls read-only `effects(invalidates())` suspending helper.
// Outer defer requiring `Open` is sound because `peek_and_yield` cannot mutate `c`.
fn nested_readonly_worker(log: &mut i32) -> i32 {
    var c = Conn { state: ConnState.Open, steps: 4 }
    defer {
        require_open(&c)
        *log = *log * 10 + 4
    }
    let v = peek_and_yield(&c)
    *log = *log * 10 + 9
    return v
}

// Worker 3: moves owned ByteBuffer into suspending callee before cancel.
// Ownership transferred to callee; callee unwinds once (digit 5), caller runs
// its own defer (digit 6) without double-dropping `buf` or running post-call code!
fn moved_owner_worker(log: &mut i32) -> i32 {
    let b = bytes_new()
    bytes_reserve(b, 4)
    bytes_push(b, 65u8)
    defer { *log = *log * 10 + 6 }
    consume_buf_and_yield(give b, log)
    *log = *log * 10 + 9
    return 1
}

fn drive_mutating(log: &mut i32) -> std.task.TaskCreateError!i32 {
    var t = task_create(nested_mutating_worker, log)?
    match task_poll(&mut t) {
        .ok(p) => {
            match p {
                std.task.Poll.Pending() => {}
                std.task.Poll.Ready(_) => {
                    task_finish_destroy(give t)
                    return -1
                }
            }
        }
        .err(_) => {
            task_finish_destroy(give t)
            return -2
        }
    }
    task_request_cancel(&mut t)
    loop {
        match task_poll_close(&mut t) {
            std.task.Poll.Pending() => { continue }
            std.task.Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give t)
    return *log
}

fn drive_readonly(log: &mut i32) -> std.task.TaskCreateError!i32 {
    var t = task_create(nested_readonly_worker, log)?
    match task_poll(&mut t) {
        .ok(p) => {
            match p {
                std.task.Poll.Pending() => {}
                std.task.Poll.Ready(_) => {
                    task_finish_destroy(give t)
                    return -1
                }
            }
        }
        .err(_) => {
            task_finish_destroy(give t)
            return -2
        }
    }
    task_request_cancel(&mut t)
    loop {
        match task_poll_close(&mut t) {
            std.task.Poll.Pending() => { continue }
            std.task.Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give t)
    return *log
}

fn drive_moved(log: &mut i32) -> std.task.TaskCreateError!i32 {
    var t = task_create(moved_owner_worker, log)?
    match task_poll(&mut t) {
        .ok(p) => {
            match p {
                std.task.Poll.Pending() => {}
                std.task.Poll.Ready(_) => {
                    task_finish_destroy(give t)
                    return -1
                }
            }
        }
        .err(_) => {
            task_finish_destroy(give t)
            return -2
        }
    }
    task_request_cancel(&mut t)
    loop {
        match task_poll_close(&mut t) {
            std.task.Poll.Pending() => { continue }
            std.task.Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give t)
    return *log
}

fn main() -> i32 {
    var log1 = 0
    match drive_mutating(&mut log1) {
        .err(_) => { return 1 }
        .ok(v) => {
            // Inner defer ran (7), then outer defer saw ConnState.Closing (2).
            // Post-call digit (9) MUST NOT run!
            if v != 72 { return 10 + v }
        }
    }
    var log2 = 0
    match drive_readonly(&mut log2) {
        .err(_) => { return 2 }
        .ok(v) => {
            // Outer defer ran require_open(&c) and logged 4; post-call (9) skipped.
            if v != 4 { return 100 + v }
        }
    }
    var log3 = 0
    match drive_moved(&mut log3) {
        .err(_) => { return 3 }
        .ok(v) => {
            // Callee defer logged 5 (and dropped moved ByteBuffer once),
            // then caller defer logged 6; post-call (9) skipped.
            if v != 56 { return 200 + v }
        }
    }
    return 0
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
}

#[test]
fn cancel_edge_protocol_source_negative_controls_are_rejected() {
    let dir = temp_dir("cancel-edge-protocol-negatives");

    // 1. Pre-call state (`Open`) MUST NOT be assumed to survive cancellation of a
    //    mutating suspending call (the callee already transitioned `c` to `Closing`
    //    before `task_yield()`).
    let neg_assume_pre_state = r#"
enum ConnState { Open, Closing, Closed }
struct Conn { state: ConnState }

fn require_open(c: &Conn)
    contract { requires(state(c, ConnState.Open)); effects(invalidates()); }
{}

fn transition_and_yield(c: &mut Conn)
    contract {
        requires(state(c, ConnState.Open));
        provides(state(c, ConnState.Closed));
        effects(invalidates(c));
    }
{
    c.state = ConnState.Closing
    task_yield()
    c.state = ConnState.Closed
}

fn bad_worker() {
    var c = Conn { state: ConnState.Open }
    defer { require_open(&c) }
    transition_and_yield(&mut c)
}

fn main() -> i32 {
    match task_create(bad_worker) {
        .err(_) => { return 1 }
        .ok(t) => {
            var owned = give t
            task_request_cancel(&mut owned)
            loop {
                match task_poll_close(&mut owned) {
                    std.task.Poll.Pending() => { continue }
                    std.task.Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give owned)
        }
    }
    return 0
}
"#;
    check_rejected(
        &dir,
        "neg_assume_pre_state",
        neg_assume_pre_state,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Normal-return postcondition (`Closed`) MUST NOT be assumed on cancellation
    //    of a mutating suspending call!
    let neg_assume_post_state = r#"
enum ConnState { Open, Closing, Closed }
struct Conn { state: ConnState }

fn require_closed(c: &Conn)
    contract { requires(state(c, ConnState.Closed)); effects(invalidates()); }
{}

fn transition_and_yield(c: &mut Conn)
    contract {
        requires(state(c, ConnState.Open));
        provides(state(c, ConnState.Closed));
        effects(invalidates(c));
    }
{
    c.state = ConnState.Closing
    task_yield()
    c.state = ConnState.Closed
}

fn bad_worker() {
    var c = Conn { state: ConnState.Open }
    defer { require_closed(&c) }
    transition_and_yield(&mut c)
}

fn main() -> i32 {
    match task_create(bad_worker) {
        .err(_) => { return 1 }
        .ok(t) => {
            var owned = give t
            task_request_cancel(&mut owned)
            loop {
                match task_poll_close(&mut owned) {
                    std.task.Poll.Pending() => { continue }
                    std.task.Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give owned)
        }
    }
    return 0
}
"#;
    check_rejected(
        &dir,
        "neg_assume_post_state",
        neg_assume_post_state,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Uncontracted suspending callee taking `&mut Conn` MUST also widen on cancel,
    //    rejecting `defer { require_open(&c) }`.
    let neg_uncontracted_mut_cancel = r#"
enum ConnState { Open, Closing, Closed }
struct Conn { state: ConnState }

fn require_open(c: &Conn)
    contract { requires(state(c, ConnState.Open)); effects(invalidates()); }
{}

fn uncontracted_mut_yield(c: &mut Conn) {
    c.state = ConnState.Closing
    task_yield()
}

fn bad_worker() {
    var c = Conn { state: ConnState.Open }
    defer { require_open(&c) }
    uncontracted_mut_yield(&mut c)
}

fn main() -> i32 {
    match task_create(bad_worker) {
        .err(_) => { return 1 }
        .ok(t) => {
            var owned = give t
            task_request_cancel(&mut owned)
            loop {
                match task_poll_close(&mut owned) {
                    std.task.Poll.Pending() => { continue }
                    std.task.Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give owned)
        }
    }
    return 0
}
"#;
    check_rejected(
        &dir,
        "neg_uncontracted_mut_cancel",
        neg_uncontracted_mut_cancel,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Built-in `Task` handle passed `&mut` to a suspending helper that cancels it
    //    before yielding: outer `defer` MUST NOT assume the child task is `Closed`
    //    or still `Open` on cancel!
    let neg_builtin_task_cancel = r#"
fn dummy_child() {
    task_yield()
}

fn require_task_open(t: &mut Task<()>)
    contract { requires(state(t, Open)); effects(invalidates()); }
{}

fn cancel_child_and_yield(t: &mut Task<()>)
    contract {
        requires(state(t, Open));
        provides(state(t, Closing));
        effects(invalidates(t));
    }
{
    task_request_cancel(t)
    task_yield()
}

fn bad_parent() -> std.task.TaskCreateError!() {
    var child = task_create(dummy_child)?
    defer {
        require_task_open(&mut child)
        task_request_cancel(&mut child)
        loop {
            match task_poll_close(&mut child) {
                std.task.Poll.Pending() => { continue }
                std.task.Poll.Ready(_) => { break }
            }
        }
        task_finish_destroy(give child)
    }
    cancel_child_and_yield(&mut child)
    return ()
}

fn main() -> i32 {
    match task_create(bad_parent) {
        .err(_) => { return 1 }
        .ok(t) => {
            var owned = give t
            task_request_cancel(&mut owned)
            loop {
                match task_poll_close(&mut owned) {
                    std.task.Poll.Pending() => { continue }
                    std.task.Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give owned)
        }
    }
    return 0
}
"#;
    check_rejected(
        &dir,
        "neg_builtin_task_cancel",
        neg_builtin_task_cancel,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );
}

#[test]
fn raw_wire_forged_cancel_edge_protocol_attacks_rejected() {
    use zil_ir::body::{
        Block, BlockId, Body, Callee, ConstValue, FnId, FnRole, FnSig, Function, IntrinsicOp,
        Local, LocalDecl, Module, Mutability, OpaqueTy, Operand, ParamEnv, Place, ProtocolStateSet,
        Region, ResumeMode, StateContractClause, Terminator, TyKind, TyTable, decode_module,
        encode_module,
    };

    let open = ProtocolStateSet(ProtocolStateSet::OPEN);
    let closed = ProtocolStateSet(ProtocolStateSet::CLOSED);
    let all = ProtocolStateSet::all_variants(3);

    // Helper building a 2-function module:
    //   FnId(0) = callee(t: &mut Managed<Task<()>> or &Managed<Task<()>> or Managed<Task<()>>)
    //   FnId(1) = caller(t: same) with `Terminator::Call { callee: Direct(FnId(0)), target: bb1, cancel: Some(bb2) }`
    //             where BOTH `bb1` (normal return) and `bb2` (cancel edge) execute `Terminator::Return`,
    //             testing whether the verifier catches forged postconditions along the cancel edge!
    let run_cancel_case = |param_mode: u8,
                           callee_contracts: Vec<StateContractClause>,
                           caller_contracts: Vec<StateContractClause>| {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let task_inner = tys.intern(TyKind::Opaque(OpaqueTy::Task {
            result: unit_ty,
            context: None,
        }));
        let managed_task = tys.intern(TyKind::Managed(task_inner));
        let param_ty = match param_mode {
            0 => tys.intern(TyKind::Ref {
                region: Region::Param(0),
                mutability: Mutability::Exclusive,
                pointee: managed_task,
            }),
            1 => tys.intern(TyKind::Ref {
                region: Region::Param(0),
                mutability: Mutability::Shared,
                pointee: managed_task,
            }),
            _ => managed_task,
        };
        let region_params = if param_mode == 2 { 0 } else { 1 };
        let call_arg = if param_mode == 1 {
            Operand::Copy(Place::local(Local(0)))
        } else {
            Operand::Move(Place::local(Local(0)))
        };

        let (callee_locals, callee_blocks) = if param_mode == 2 {
            (
                vec![
                    LocalDecl {
                        ty: param_ty,
                        name: Some("h".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: None,
                    },
                ],
                vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::Intrinsic(IntrinsicOp::TaskFinishDestroy),
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
            )
        } else {
            (
                vec![LocalDecl {
                    ty: param_ty,
                    name: Some("h".into()),
                }],
                vec![Block {
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            )
        };

        let callee_fn = Function {
            name: "callee".into(),
            sig: FnSig {
                params: vec![param_ty],
                result: unit_ty,
                type_params: 0,
                region_params,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: callee_contracts,
            },
            body: Body {
                locals: callee_locals,
                param_count: 1,
                result: unit_ty,
                closure: None,
                captures: vec![],
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: callee_blocks,
            },
        };

        let caller_fn = Function {
            name: "caller".into(),
            sig: FnSig {
                params: vec![param_ty],
                result: unit_ty,
                type_params: 0,
                region_params,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: caller_contracts,
            },
            body: Body {
                locals: vec![
                    LocalDecl {
                        ty: param_ty,
                        name: Some("h".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: None,
                    },
                ],
                param_count: 1,
                result: unit_ty,
                closure: None,
                captures: vec![],
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![
                    Block {
                        instructions: vec![],
                        terminator: Terminator::Call {
                            callee: Callee::Direct(FnId(0)),
                            args: vec![call_arg],
                            dest: Place::local(Local(1)),
                            target: BlockId(1),
                            cancel: Some(BlockId(2)),
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
            functions: vec![callee_fn, caller_fn],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        };
        let decoded = decode_module(&encode_module(&m)).expect("wire roundtrip");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
    };

    // Note: callee_fn's body is a trivial Return, so its own contract must be satisfied
    // by its body OR we check only caller's error (FnId(1)). To keep FnId(0) valid on
    // its own body while having a mutating signature, use `requires(state(0, all))` +
    // `provides(state(0, all))` + `effects(invalidates(0))` (or uncontracted callee)!

    // Attack 1: Uncontracted `&mut` callee cancels -> caller forges `provides(state(0, Open))`
    // (claiming pre-call state survived cancel). MUST REJECT at FnId(1)!
    let err1 = run_cancel_case(
        0,
        vec![],
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: open,
            },
            StateContractClause::ProvidesState {
                param: 0,
                states: open,
            },
        ],
    )
    .expect_err("must reject claiming pre-call Open survives cancel");
    assert_eq!(err1.function, FnId(1));

    // Attack 2: Callee with `effects(invalidates(0))` + `provides(state(0, Closed))`
    // (body satisfies `requires(state(0, Closed))` -> `provides(state(0, Closed))`),
    // caller requires `Closed` and claims `provides(state(0, Closed))` without rollback.
    // On cancel, `on_cancel_possible_mutation` widens `0` to `all` and sets `rolled_back = true`,
    // so claiming `provides(state(0, Closed))` MUST REJECT at FnId(1)!
    let err2 = run_cancel_case(
        0,
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: closed,
            },
            StateContractClause::ProvidesState {
                param: 0,
                states: closed,
            },
            StateContractClause::InvalidationUpperBound,
            StateContractClause::Invalidates {
                param: 0,
                path: zil_ir::body::ContractFieldPath::from_slice(&[]).unwrap(),
            },
        ],
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: closed,
            },
            StateContractClause::ProvidesState {
                param: 0,
                states: closed,
            },
        ],
    )
    .expect_err("must reject claiming normal-return Closed holds on cancel edge");
    assert_eq!(err2.function, FnId(1));

    // Attack 3: Ownership transfer (`Operand::Move`) on cancel -> even when `callee`
    // promises `provides(retired(0))` on normal return, `caller` forging
    // `provides(retired(0))` MUST REJECT at FnId(1) (normal retirement was not reached)!
    let err3 = run_cancel_case(
        2,
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: closed,
            },
            StateContractClause::ProvidesRetired { param: 0 },
        ],
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: closed,
            },
            StateContractClause::ProvidesRetired { param: 0 },
        ],
    )
    .expect_err("must reject claiming provides(retired) along cancel edge of moved handle");
    assert_eq!(err3.function, FnId(1));

    // Attack 4: Mutating callee (`invalidates(0)`) cancels -> caller forges
    // `effects(invalidates())` (claiming no invalidation on cancel). MUST REJECT at FnId(1)!
    let err4 = run_cancel_case(
        0,
        vec![
            StateContractClause::InvalidationUpperBound,
            StateContractClause::Invalidates {
                param: 0,
                path: zil_ir::body::ContractFieldPath::from_slice(&[]).unwrap(),
            },
        ],
        vec![
            StateContractClause::ProvidesState {
                param: 0,
                states: all,
            },
            StateContractClause::AllowsRollback { param: 0 },
            StateContractClause::InvalidationUpperBound,
        ],
    )
    .expect_err("must reject empty invalidates() when mutating call cancels");
    assert_eq!(err4.function, FnId(1));

    // Attack 5: Mutating callee cancels -> caller claims `provides(state(0, all))`
    // WITHOUT `effects(rollback(0))` (hiding rollback H on cancel). MUST REJECT at FnId(1)!
    let err5 = run_cancel_case(
        0,
        vec![],
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: open,
            },
            StateContractClause::ProvidesState {
                param: 0,
                states: all,
            },
        ],
    )
    .expect_err("must reject missing effects(rollback) when mutating call cancels");
    assert_eq!(err5.function, FnId(1));

    // Positive control 1: Caller accepts `provides(state(0, all))` + `effects(rollback(0))`
    // + `effects(invalidates(0))` -> ACCEPTED!
    run_cancel_case(
        0,
        vec![],
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: open,
            },
            StateContractClause::ProvidesState {
                param: 0,
                states: all,
            },
            StateContractClause::AllowsRollback { param: 0 },
            StateContractClause::InvalidationUpperBound,
            StateContractClause::Invalidates {
                param: 0,
                path: zil_ir::body::ContractFieldPath::from_slice(&[]).unwrap(),
            },
        ],
    )
    .expect("sound cancel contract declaring widened state, rollback, and invalidates must pass");

    // Positive control 2: Pure read-only shared `&Managed<Task<()>>` callee declaring
    // `effects(invalidates())` preserves `Open` and empty `invalidates()` even on cancel!
    run_cancel_case(
        1,
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: open,
            },
            StateContractClause::ProvidesState {
                param: 0,
                states: open,
            },
            StateContractClause::InvalidationUpperBound,
        ],
        vec![
            StateContractClause::RequiresState {
                param: 0,
                states: open,
            },
            StateContractClause::ProvidesState {
                param: 0,
                states: open,
            },
            StateContractClause::InvalidationUpperBound,
        ],
    )
    .expect("pure read-only shared callee must preserve Open and invalidates() across cancel");
}

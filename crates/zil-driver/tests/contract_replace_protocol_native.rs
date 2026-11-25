//! End-to-end native, source-rejection, and Raw/Wire verification tests for `Replace`
//! protocol history isolation, `old` obligation transfer, retirement clearing, and CFG alias joins.

mod contract_semantics_support;

#[allow(unused_imports)]
use contract_semantics_support::{
    build_and_run_dual_with_c, check_rejected, check_rejected_with_reason, temp_dir, write_source,
};
#[allow(unused_imports)]
use zil_base::ResultCategory;

#[test]
fn owning_replace_and_branch_rollback_controls_execute_natively_on_dual_backends() {
    let dir = temp_dir("replace-and-rollback-native");
    let src = r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState, code: i32 }
package Holder { manages payload: ByteBuffer; pub tag: i32 }

fn swap_payload(h: &mut Holder)
    contract { effects(invalidates(h.payload)); }
{
    let fresh = bytes_new()
    bytes_push(fresh, 42u8)
    h.payload = give fresh
}

fn branch_reconnect(ch: &mut Channel, do_reconnect: bool)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
        effects(rollback(ch));
    }
{
    if do_reconnect {
        ch.state = ChanState.Closed
        ch.state = ChanState.Open
    } else {
        ch.state = ChanState.Closed
    }
}

fn main() -> i32 {
    var h = Holder { payload: bytes_new(), tag: 7 }
    swap_payload(&mut h)
    var ch = Channel { state: ChanState.Open, code: 1 }
    branch_reconnect(&mut ch, true)
    let b = bytes_view(h.payload)[0]
    if b == 42u8 && h.tag == 7 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verifier_soundness_replace_cfg_alias_join_and_path_rollback_are_rejected() {
    let dir = temp_dir("verifier-soundness-reject");

    // P0-1a: Owning field Replace (`h.payload = bytes_new()`) rejected under empty invalidates().
    check_rejected(
        &dir,
        "replace_owning_field_violates_empty_invalidates",
        r#"
package Holder { manages payload: ByteBuffer; pub tag: i32 }
fn bad(h: &mut Holder) contract { effects(invalidates()); } {
    h.payload = bytes_new()
}
fn main() -> i32 {
    var h = Holder { payload: bytes_new(), tag: 0 }
    bad(&mut h)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-1b: Owning field Replace on `h.payload` rejected when only `h.tag` is declared invalidated.
    check_rejected(
        &dir,
        "replace_owning_field_violates_disjoint_invalidates",
        r#"
package Holder { manages payload: ByteBuffer; pub tag: i32 }
fn bad(h: &mut Holder) contract { effects(invalidates(h.tag)); } {
    h.payload = bytes_new()
}
fn main() -> i32 {
    var h = Holder { payload: bytes_new(), tag: 0 }
    bad(&mut h)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-2a: CFG join of `alias` (Open on then-branch, Closed on else-branch) must fail closed.
    check_rejected(
        &dir,
        "cfg_join_alias_open_then_closed_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_open(ch: &Channel) contract { requires(state(ch, ChanState.Open)); } {}
fn bad(flag: bool) {
    let a = Channel { state: ChanState.Open }
    let b = Channel { state: ChanState.Closed }
    var r = &a
    if flag {
        r = &a
    } else {
        r = &b
    }
    require_open(r)
}
fn main() -> i32 {
    bad(false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-2b: Reversed branch order (Closed on then-branch, Open on else-branch) must also fail closed.
    check_rejected(
        &dir,
        "cfg_join_alias_closed_then_open_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn require_open(ch: &Channel) contract { requires(state(ch, ChanState.Open)); } {}
fn bad(flag: bool) {
    let a = Channel { state: ChanState.Open }
    let b = Channel { state: ChanState.Closed }
    var r = &b
    if flag {
        r = &b
    } else {
        r = &a
    }
    require_open(r)
}
fn main() -> i32 {
    bad(true)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-2c: CFG join of `param_alias` (p.a on then-branch, p.b on else-branch) must invalidate both.
    check_rejected(
        &dir,
        "cfg_join_param_alias_a_then_b_rejected",
        r#"
struct Pair { a: i32, b: i32 }
fn bad(p: &mut Pair, flag: bool) contract { effects(invalidates(p.a)); } {
    if flag {
        let r = &mut p.a
        *r = 1
    } else {
        let r = &mut p.b
        *r = 2
    }
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p, false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-2d: Merged mutable ref to p.a vs p.b written after join (both branch orders).
    check_rejected(
        &dir,
        "cfg_join_merged_param_ref_a_then_b_rejected",
        r#"
struct Pair { a: i32, b: i32 }
fn bad(p: &mut Pair, flag: bool) contract { effects(invalidates(p.a)); } {
    var r = &mut p.a
    if flag {
        r = &mut p.a
    } else {
        r = &mut p.b
    }
    *r = 9
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p, false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    check_rejected(
        &dir,
        "cfg_join_merged_param_ref_b_then_a_rejected",
        r#"
struct Pair { a: i32, b: i32 }
fn bad(p: &mut Pair, flag: bool) contract { effects(invalidates(p.a)); } {
    var r = &mut p.b
    if flag {
        r = &mut p.b
    } else {
        r = &mut p.a
    }
    *r = 9
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p, true)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-3a: Path-correlated rollback H: Branch A does Open->Closed->Open, Branch B does Open->Closed.
    check_rejected(
        &dir,
        "branch_rollback_hidden_by_other_branch_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn sneaky(ch: &mut Channel, flag: bool)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    if flag {
        ch.state = ChanState.Closed
        ch.state = ChanState.Open
    } else {
        ch.state = ChanState.Closed
    }
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    sneaky(&mut ch, true)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // P0-3b: Reversed branch order (Branch A normal Open->Closed, Branch B rollback Open->Closed->Open).
    check_rejected(
        &dir,
        "branch_rollback_hidden_reversed_order_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn sneaky(ch: &mut Channel, flag: bool)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    if flag {
        ch.state = ChanState.Closed
    } else {
        ch.state = ChanState.Closed
        ch.state = ChanState.Open
    }
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    sneaky(&mut ch, false)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_replace_cfg_alias_join_and_branch_rollback_are_rejected_by_checker() {
    use zil_ir::body::{
        AdtDef, AdtKind, AggregateKind, Block, BlockId, Body, ConstValue, FieldDef, FieldIdx,
        FieldRef, FnId, FnRole, FnSig, Function, GenericArgs, Instruction, InstructionKind, IntTy,
        Local, LocalDecl, Module, Mutability, Operand, ParamEnv, Place, PlaceRoot, Projection,
        ProtocolStateSet, Region, ResumeMode, Rvalue, StateContractClause, Terminator, TyKind,
        TyTable, VariantDef, VariantIdx, decode_module, encode_module,
    };

    // Case 1 (P0-1 Raw/Wire): `InstructionKind::Replace` on `(*p).a` under empty `InvalidationUpperBound`.
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let box_adt = tys.add_adt(AdtDef {
            name: "Boxed".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Boxed".into(),
                fields: vec![FieldDef {
                    name: "a".into(),
                    ty: i32_ty,
                }],
            }],
            user_drop: false,
        });
        let box_ty = tys.intern(TyKind::Adt {
            adt: box_adt,
            args: empty_args,
        });
        let mut_box = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: box_ty,
        });
        let func = Function {
            name: "forged_replace".into(),
            sig: FnSig {
                params: vec![mut_box],
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
                        ty: mut_box,
                        name: Some("p".into()),
                    },
                    LocalDecl {
                        ty: i32_ty,
                        name: Some("old".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: vec![Instruction {
                        kind: InstructionKind::Replace {
                            place: Place {
                                root: PlaceRoot::Local(Local(0)),
                                projections: vec![
                                    Projection::Deref,
                                    Projection::Field(FieldRef {
                                        adt: box_adt,
                                        args: empty_args,
                                        variant: VariantIdx(0),
                                        field: FieldIdx(0),
                                    }),
                                ],
                            },
                            value: Operand::Const(ConstValue::Int {
                                ty: IntTy::I32,
                                bits: 7,
                            }),
                            old: Local(1),
                        },
                        span: zil_base::Span::default(),
                    }],
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
            },
        };
        let module = Module {
            tys,
            functions: vec![func],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        };
        let bytes = encode_module(&module);
        let decoded = decode_module(&bytes).expect("wire decode Replace module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solver produces valid certificate for Replace");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject Replace under empty invalidation upper bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // Case 2 (P0-3 Raw/Wire): Branch A does Open->Closed->Open, Branch B does Open->Closed,
    // merging to Return with RequiresState(Open) & ProvidesState(Open | Closed), NO AllowsRollback.
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let bool_ty = tys.intern(TyKind::Bool);
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let st_adt = tys.add_adt(AdtDef {
            name: "ChanState".into(),
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
                VariantDef {
                    name: "Init".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Open".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Closed".into(),
                    fields: vec![],
                },
            ],
            user_drop: false,
        });
        let st_ty = tys.intern(TyKind::Adt {
            adt: st_adt,
            args: empty_args,
        });
        let ch_adt = tys.add_adt(AdtDef {
            name: "Channel".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Channel".into(),
                fields: vec![FieldDef {
                    name: "state".into(),
                    ty: st_ty,
                }],
            }],
            user_drop: false,
        });
        let ch_ty = tys.intern(TyKind::Adt {
            adt: ch_adt,
            args: empty_args,
        });
        let mut_ch = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: ch_ty,
        });
        let state_place = || Place {
            root: PlaceRoot::Local(Local(0)),
            projections: vec![
                Projection::Deref,
                Projection::Field(FieldRef {
                    adt: ch_adt,
                    args: empty_args,
                    variant: VariantIdx(0),
                    field: FieldIdx(0),
                }),
            ],
        };
        let mk_variant = |v: u32| Rvalue::Aggregate {
            kind: AggregateKind::Adt {
                adt: st_adt,
                args: empty_args,
                variant: VariantIdx(v),
            },
            operands: vec![],
        };
        let func = Function {
            name: "forged_branch_rollback".into(),
            sig: FnSig {
                params: vec![mut_ch, bool_ty],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: ProtocolStateSet::single(1),
                    },
                    StateContractClause::ProvidesState {
                        param: 0,
                        states: ProtocolStateSet::single(1).union(ProtocolStateSet::single(2)),
                    },
                ],
            },
            body: Body {
                param_count: 2,
                locals: vec![
                    LocalDecl {
                        ty: mut_ch,
                        name: Some("ch".into()),
                    },
                    LocalDecl {
                        ty: bool_ty,
                        name: Some("flag".into()),
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
                        terminator: Terminator::Switch {
                            discr: Operand::Copy(Place::local(Local(1))),
                            arms: vec![(1, BlockId(1))],
                            otherwise: BlockId(2),
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    // Branch A: Open -> Closed -> Open (hidden rollback!)
                    Block {
                        instructions: vec![
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: state_place(),
                                    value: mk_variant(2),
                                },
                                span: zil_base::Span::default(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: state_place(),
                                    value: mk_variant(1),
                                },
                                span: zil_base::Span::default(),
                            },
                        ],
                        terminator: Terminator::Goto { target: BlockId(3) },
                        terminator_span: zil_base::Span::default(),
                    },
                    // Branch B: Open -> Closed (normal)
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: state_place(),
                                value: mk_variant(2),
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Goto { target: BlockId(3) },
                        terminator_span: zil_base::Span::default(),
                    },
                    // Join + Return
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
        let module = Module {
            tys,
            functions: vec![func],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        };
        let bytes = encode_module(&module);
        let decoded = decode_module(&bytes).expect("wire decode branch rollback module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solver produces valid certificate for branch rollback");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject path-correlated hidden rollback H across CFG merge",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
}

#[test]
fn replace_state_protocol_history_isolation_and_retirement_execute_natively_on_dual_backends() {
    let dir = temp_dir("replace-protocol-native");
    let src = r#"
enum ChanState { Init, Open, Closed }
package OwnedChannel { pub state: ChanState; manages buf: ByteBuffer; pub tag: i32 }

fn open_channel(ch: &mut OwnedChannel)
    contract {
        requires(state(ch, ChanState.Init));
        provides(state(ch, ChanState.Open));
    }
{
    ch.state = ChanState.Open
}

fn require_open(ch: &OwnedChannel) -> i32
    contract { requires(state(ch, ChanState.Open)); }
{
    return ch.tag
}

// 1. Old handle transitions Open -> Closed, then `*ch = fresh` replaces the whole owning
//    protocol handle with a fresh Open instance. Old and new do NOT share history `H`,
//    so no `effects(rollback(ch))` is required.
fn replace_after_close_no_shared_history(ch: &mut OwnedChannel, fresh: OwnedChannel)
    contract {
        requires(state(ch, ChanState.Open));
        requires(state(fresh, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    ch.state = ChanState.Closed
    *ch = fresh
}

// 2. Incoming handle starts at Init, monotonically advances Init -> Open, then replaces
//    a closed Open handle. New instance keeps its own origin/history (no spurious rollback).
fn replace_with_freshly_opened(ch: &mut OwnedChannel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let b = bytes_new()
    bytes_push(b, 77u8)
    var fresh = OwnedChannel { state: ChanState.Init, buf: give b, tag: 99 }
    open_channel(&mut fresh)
    ch.state = ChanState.Closed
    *ch = fresh
}

// 3. Replacing with an Init handle lets the caller advance the new handle from Init -> Open.
fn replace_with_init_then_open(ch: &mut OwnedChannel, fresh: OwnedChannel)
    contract {
        requires(state(ch, ChanState.Open));
        requires(state(fresh, ChanState.Init));
        provides(state(ch, ChanState.Open));
    }
{
    *ch = fresh
    open_channel(ch)
}

// 4. Built-in Task protocol: close a Task, replace `*slot` with a fresh Open Task,
//    then close the new Task and satisfy `provides(state(slot, Closed))`.
fn worker(v: i32) -> i32 contract { effects(suspend) } {
    task_yield()
    return v + 1
}

fn close_task(t: &mut Task<i32>)
    contract {
        requires(state(t, Open, Closing, Closed));
        provides(state(t, Closed));
    }
{
    task_request_cancel(t)
    loop {
        match task_poll_close(t) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
}

fn close_replace_and_close_new_task(slot: &mut Task<i32>, give fresh: Task<i32>)
    contract {
        requires(state(slot, Open, Closing, Closed));
        requires(state(fresh, Open, Closing, Closed));
        provides(state(slot, Closed));
    }
{
    close_task(slot)
    *slot = give fresh
    close_task(slot)
}

fn main() -> TaskCreateError!i32 {
    let b0 = bytes_new()
    bytes_push(b0, 10u8)
    var ch = OwnedChannel { state: ChanState.Open, buf: give b0, tag: 1 }

    let b1 = bytes_new()
    bytes_push(b1, 20u8)
    let fresh1 = OwnedChannel { state: ChanState.Open, buf: give b1, tag: 2 }
    replace_after_close_no_shared_history(&mut ch, fresh1)
    let t1 = require_open(&ch)

    replace_with_freshly_opened(&mut ch)
    let t2 = require_open(&ch)
    let byte2 = bytes_view(ch.buf)[0]

    let b3 = bytes_new()
    bytes_push(b3, 30u8)
    let fresh3 = OwnedChannel { state: ChanState.Init, buf: give b3, tag: 3 }
    replace_with_init_then_open(&mut ch, fresh3)
    let t3 = require_open(&ch)

    var task0 = task_create(worker, 10)?
    let task1 = task_create(worker, 20)?
    close_replace_and_close_new_task(&mut task0, give task1)
    task_finish_destroy(give task0)

    if t1 == 2 && t2 == 99 && byte2 == 77u8 && t3 == 3 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replace_state_protocol_retired_clearing_state_and_rollback_violations_are_rejected() {
    let dir = temp_dir("replace-protocol-reject");

    // 1. Closing a Task (`Closed`), then replacing `*slot = give fresh` with an unclosed Task,
    //    must replace the slot's Closed state so calling `task_finish_destroy` afterwards is rejected.
    check_rejected_with_reason(
        &dir,
        "replace_closed_task_then_finish_destroy_violation",
        r#"
fn worker(v: i32) -> i32 contract { effects(suspend) } {
    task_yield()
    return v
}

fn close_task(t: &mut Task<i32>)
    contract {
        requires(state(t, Open, Closing, Closed));
        provides(state(t, Closed));
    }
{
    task_request_cancel(t)
    loop {
        match task_poll_close(t) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
}

fn main() -> TaskCreateError!i32 {
    var t0 = task_create(worker, 1)?
    close_task(&mut t0)
    let t1 = task_create(worker, 2)?
    let slot = &mut t0
    *slot = give t1
    task_finish_destroy(give t0)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "Closed",
    );

    // 2. Replacing an Open slot with an Init handle updates the slot to Init,
    //    so requiring Open afterwards must be rejected.
    check_rejected_with_reason(
        &dir,
        "replace_inherits_incoming_init_state_violation",
        r#"
enum ChanState { Init, Open, Closed }
package OwnedChannel { pub state: ChanState; manages buf: ByteBuffer; pub tag: i32 }

fn require_open(ch: &OwnedChannel)
    contract { requires(state(ch, ChanState.Open)); }
{}

fn bad(ch: &mut OwnedChannel, fresh: OwnedChannel)
    contract {
        requires(state(ch, ChanState.Open));
        requires(state(fresh, ChanState.Init));
    }
{
    *ch = fresh
    require_open(ch)
}

fn main() -> i32 {
    var ch = OwnedChannel { state: ChanState.Open, buf: bytes_new(), tag: 1 }
    let fresh = OwnedChannel { state: ChanState.Init, buf: bytes_new(), tag: 2 }
    bad(&mut ch, fresh)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "precondition",
    );

    // 3. Incoming handle that itself underwent a hidden rollback (Open -> Closed -> Open)
    //    carries its own rollback history into the slot; replacing without `effects(rollback(ch))`
    //    must be rejected.
    check_rejected_with_reason(
        &dir,
        "replace_preserves_incoming_own_rollback_violation",
        r#"
enum ChanState { Init, Open, Closed }
package OwnedChannel { pub state: ChanState; manages buf: ByteBuffer; pub tag: i32 }

fn bad(ch: &mut OwnedChannel, fresh: OwnedChannel)
    contract {
        requires(state(ch, ChanState.Open));
        requires(state(fresh, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    var dirty = fresh
    dirty.state = ChanState.Closed
    dirty.state = ChanState.Open
    *ch = dirty
}

fn main() -> i32 {
    var ch = OwnedChannel { state: ChanState.Open, buf: bytes_new(), tag: 1 }
    let fresh = OwnedChannel { state: ChanState.Open, buf: bytes_new(), tag: 2 }
    bad(&mut ch, fresh)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 4. Replacing a Closed Task slot with an unclosed Task must NOT retain Closed on `*slot`.
    check_rejected_with_reason(
        &dir,
        "replace_closed_task_with_open_loses_closed_violation",
        r#"
fn worker(v: i32) -> i32 contract { effects(suspend) } {
    task_yield()
    return v
}

fn bad(slot: &mut Task<i32>, give fresh: Task<i32>)
    contract {
        requires(state(slot, Closed));
        requires(state(fresh, Open, Closing, Closed));
        provides(state(slot, Closed));
    }
{
    *slot = give fresh
}

fn main() -> TaskCreateError!i32 {
    var t0 = task_create(worker, 1)?
    task_request_cancel(&mut t0)
    loop {
        match task_poll_close(&mut t0) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    let t1 = task_create(worker, 2)?
    bad(&mut t0, give t1)
    task_finish_destroy(give t0)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state",
    );

    // 5. Ambiguous merged `&mut` reference weak-replace with an Init handle widens
    //    candidate targets to Open|Init, so claiming `provides(state(a, Open))` fails.
    check_rejected_with_reason(
        &dir,
        "replace_ambiguous_ref_widens_candidate_state_violation",
        r#"
enum ChanState { Init, Open, Closed }
package OwnedChannel { pub state: ChanState; manages buf: ByteBuffer; pub tag: i32 }

fn bad(a: &mut OwnedChannel, b: &mut OwnedChannel, fresh: OwnedChannel, flag: bool)
    contract {
        requires(state(a, ChanState.Open));
        requires(state(b, ChanState.Open));
        requires(state(fresh, ChanState.Init));
        provides(state(a, ChanState.Open));
    }
{
    let r = if flag { a } else { b }
    *r = fresh
}

fn main() -> i32 {
    var a = OwnedChannel { state: ChanState.Open, buf: bytes_new(), tag: 1 }
    var b = OwnedChannel { state: ChanState.Open, buf: bytes_new(), tag: 2 }
    let fresh = OwnedChannel { state: ChanState.Init, buf: bytes_new(), tag: 3 }
    bad(&mut a, &mut b, fresh, true)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_replace_old_transfer_history_isolation_and_retirement_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, FieldDef, FnId, FnRole, FnSig,
        Function, GenericArgs, Instruction, InstructionKind, IntrinsicOp, Local, LocalDecl, Module,
        Mutability, OpaqueTy, Operand, ParamEnv, Place, PlaceRoot, Projection, ProtocolStateSet,
        Region, ResumeMode, Rvalue, StateContractClause, Terminator, TyKind, TyTable, VariantDef,
        decode_module, encode_module,
    };

    // Part 1: User-defined protocol handle `Replace { place: *slot, value: Move(fresh), old }`
    // verifying exact old transfer to `old` and incoming transfer to `*slot`.
    let build_user_replace_mod = |slot_init: ProtocolStateSet,
                                  fresh_init: ProtocolStateSet,
                                  require_old_state: ProtocolStateSet,
                                  require_slot_state: ProtocolStateSet|
     -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let state_adt = tys.add_adt(AdtDef {
            name: "ChanState".into(),
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
                VariantDef {
                    name: "Init".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Open".into(),
                    fields: vec![],
                },
                VariantDef {
                    name: "Closed".into(),
                    fields: vec![],
                },
            ],
            user_drop: false,
        });
        let state_ty = tys.intern(TyKind::Adt {
            adt: state_adt,
            args: empty_args,
        });
        let ch_adt = tys.add_adt(AdtDef {
            name: "Channel".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Channel".into(),
                fields: vec![FieldDef {
                    name: "state".into(),
                    ty: state_ty,
                }],
            }],
            user_drop: false,
        });
        let ch_ty = tys.intern(TyKind::Adt {
            adt: ch_adt,
            args: empty_args,
        });
        let mut_ch = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: ch_ty,
        });
        let ref_ch = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: ch_ty,
        });

        // FnId(1): check_old(&Channel) requires(state(0, require_old_state))
        let check_old_fn = Function {
            name: "check_old".into(),
            sig: FnSig {
                params: vec![ref_ch],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::RequiresState {
                    param: 0,
                    states: require_old_state,
                }],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ref_ch,
                    name: Some("c".into()),
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
        // FnId(2): check_slot(&Channel) requires(state(0, require_slot_state))
        let check_slot_fn = Function {
            name: "check_slot".into(),
            sig: FnSig {
                params: vec![ref_ch],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![StateContractClause::RequiresState {
                    param: 0,
                    states: require_slot_state,
                }],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ref_ch,
                    name: Some("c".into()),
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

        let driver = Function {
            name: "replace_driver".into(),
            sig: FnSig {
                params: vec![mut_ch, ch_ty],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: slot_init,
                    },
                    StateContractClause::RequiresState {
                        param: 1,
                        states: fresh_init,
                    },
                ],
            },
            body: Body {
                param_count: 2,
                locals: vec![
                    LocalDecl {
                        ty: mut_ch,
                        name: Some("slot".into()),
                    },
                    LocalDecl {
                        ty: ch_ty,
                        name: Some("fresh".into()),
                    },
                    LocalDecl {
                        ty: ch_ty,
                        name: Some("old".into()),
                    },
                    LocalDecl {
                        ty: ref_ch,
                        name: Some("old_ref".into()),
                    },
                    LocalDecl {
                        ty: ref_ch,
                        name: Some("slot_ref".into()),
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
                        instructions: vec![
                            Instruction {
                                kind: InstructionKind::Replace {
                                    place: Place {
                                        root: PlaceRoot::Local(Local(0)),
                                        projections: vec![Projection::Deref],
                                    },
                                    value: Operand::Move(Place::local(Local(1))),
                                    old: Local(2),
                                },
                                span: zil_base::Span::default(),
                            },
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(3)),
                                    value: Rvalue::Ref {
                                        region: Region::Static,
                                        mutability: Mutability::Shared,
                                        place: Place::local(Local(2)),
                                    },
                                },
                                span: zil_base::Span::default(),
                            },
                        ],
                        terminator: Terminator::Call {
                            callee: Callee::Direct(FnId(1)),
                            args: vec![Operand::Copy(Place::local(Local(3)))],
                            dest: Place::local(Local(5)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![Instruction {
                            kind: InstructionKind::Assign {
                                dest: Place::local(Local(4)),
                                value: Rvalue::Ref {
                                    region: Region::Static,
                                    mutability: Mutability::Shared,
                                    place: Place {
                                        root: PlaceRoot::Local(Local(0)),
                                        projections: vec![Projection::Deref],
                                    },
                                },
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Call {
                            callee: Callee::Direct(FnId(2)),
                            args: vec![Operand::Copy(Place::local(Local(4)))],
                            dest: Place::local(Local(5)),
                            target: BlockId(2),
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
            functions: vec![driver, check_old_fn, check_slot_fn],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        }
    };

    let open = ProtocolStateSet::single(1);
    let closed = ProtocolStateSet::single(2);

    // 1a. Positive: slot(Closed) replaced by fresh(Open) -> old is Closed, slot is Open.
    {
        let m = build_user_replace_mod(closed, open, closed, open);
        let decoded = decode_module(&encode_module(&m)).expect("decode user replace ok");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve user replace ok");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept exact old(Closed) and slot(Open) after Replace");
    }

    // 1b. Negative: slot(Open) replaced by fresh(Closed) -> old is Open, NOT Closed!
    {
        let m = build_user_replace_mod(open, closed, closed, closed);
        let decoded = decode_module(&encode_module(&m)).expect("decode user replace old mismatch");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve user replace old mismatch");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject treating old(Open) as Closed after Replace");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // 1c. Negative: slot(Closed) replaced by fresh(Open) -> slot is Open, NOT Closed!
    {
        let m = build_user_replace_mod(closed, open, closed, closed);
        let decoded = decode_module(&encode_module(&m)).expect("decode user replace slot mismatch");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve user replace slot mismatch");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject treating slot(Open) as Closed after Replace");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // Part 2: Built-in `Managed<Task<unit>>` Replace -> `TaskFinishDestroy(old)`.
    // Verifies old resource's Closed/Open state transfers completely to `old`.
    let build_task_replace_mod =
        |slot_init: ProtocolStateSet, fresh_init: ProtocolStateSet| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let task_inner = tys.intern(TyKind::Opaque(OpaqueTy::Task {
                result: unit_ty,
                context: None,
            }));
            let managed_task = tys.intern(TyKind::Managed(task_inner));
            let mut_task = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Exclusive,
                pointee: managed_task,
            });
            let func = Function {
                name: "replace_task_and_destroy_old".into(),
                sig: FnSig {
                    params: vec![mut_task, managed_task],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![
                        StateContractClause::RequiresState {
                            param: 0,
                            states: slot_init,
                        },
                        StateContractClause::RequiresState {
                            param: 1,
                            states: fresh_init,
                        },
                    ],
                },
                body: Body {
                    param_count: 2,
                    locals: vec![
                        LocalDecl {
                            ty: mut_task,
                            name: Some("slot".into()),
                        },
                        LocalDecl {
                            ty: managed_task,
                            name: Some("fresh".into()),
                        },
                        LocalDecl {
                            ty: managed_task,
                            name: Some("old".into()),
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
                            instructions: vec![Instruction {
                                kind: InstructionKind::Replace {
                                    place: Place {
                                        root: PlaceRoot::Local(Local(0)),
                                        projections: vec![Projection::Deref],
                                    },
                                    value: Operand::Move(Place::local(Local(1))),
                                    old: Local(2),
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::TaskFinishDestroy),
                                args: vec![Operand::Move(Place::local(Local(2)))],
                                dest: Place::local(Local(3)),
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
                functions: vec![func],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

    let task_open = ProtocolStateSet(ProtocolStateSet::OPEN);
    let task_closed = ProtocolStateSet(ProtocolStateSet::CLOSED);

    // 2a. Positive: Closed Task in `*slot` replaced by Open Task -> `TaskFinishDestroy(old)` succeeds.
    {
        let m = build_task_replace_mod(task_closed, task_open);
        let decoded = decode_module(&encode_module(&m)).expect("decode task replace ok");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve task replace ok");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must allow TaskFinishDestroy on Closed old task moved out by Replace");
    }

    // 2b. Negative: Open Task in `*slot` replaced by Closed Task -> `TaskFinishDestroy(old)` MUST reject!
    {
        let m = build_task_replace_mod(task_open, task_closed);
        let decoded = decode_module(&encode_module(&m)).expect("decode task replace bad");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve task replace bad");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject TaskFinishDestroy on Open old task moved out by Replace",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // Part 3: Raw/Wire stale-retired clearing when replacing a retired owning handle parameter
    // with a fresh unretired handle -> MUST reject `provides(retired(slot))` at FnId(0)!
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let task_inner = tys.intern(TyKind::Opaque(OpaqueTy::Task {
            result: unit_ty,
            context: None,
        }));
        let managed_task = tys.intern(TyKind::Managed(task_inner));
        let func = Function {
            name: "retire_then_reassign_unretired".into(),
            sig: FnSig {
                params: vec![managed_task, managed_task],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: task_closed,
                    },
                    StateContractClause::RequiresState {
                        param: 1,
                        states: task_open,
                    },
                    StateContractClause::ProvidesRetired { param: 0 },
                ],
            },
            body: Body {
                param_count: 2,
                locals: vec![
                    LocalDecl {
                        ty: managed_task,
                        name: Some("slot".into()),
                    },
                    LocalDecl {
                        ty: managed_task,
                        name: Some("fresh".into()),
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
                            callee: Callee::Intrinsic(IntrinsicOp::TaskFinishDestroy),
                            args: vec![Operand::Move(Place::local(Local(0)))],
                            dest: Place::local(Local(2)),
                            target: BlockId(1),
                            cancel: None,
                        },
                        terminator_span: zil_base::Span::default(),
                    },
                    Block {
                        instructions: vec![
                            Instruction {
                                kind: InstructionKind::Assign {
                                    dest: Place::local(Local(0)),
                                    value: Rvalue::Use(Operand::Move(Place::local(Local(1)))),
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
        let decoded = decode_module(&encode_module(&m)).expect("decode stale retired reassign");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve stale retired reassign");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject stale retired after replacing retired handle with fresh unretired handle",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
}

//! End-to-end native, source-rejection, and Raw/Wire verification tests for projection
//! invalidation upper bounds, history rollback `H`, unified call effects, and entry capability scans.

mod contract_semantics_support;

#[allow(unused_imports)]
use contract_semantics_support::{
    build_and_run_dual_with_c, check_rejected, check_rejected_with_reason, temp_dir, write_source,
};
#[allow(unused_imports)]
use zil_base::ResultCategory;

#[test]
fn projection_invalidation_and_history_rollback_execute_natively_on_dual_backends() {
    let dir = temp_dir("projection-invalidation-native");
    let src = r#"
extern "C" fn putchar(code: i32) -> i32;

enum ChanState { Init, Open, Closed }

struct Channel {
    state: ChanState,
    counter: i32,
    tag: i32,
}

fn open(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Init));
        provides(state(ch, ChanState.Open));
        effects(invalidates(ch.state));
    }
{
    ch.state = ChanState.Open
}

fn send(ch: &mut Channel, byte: i32)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
        effects(invalidates());
    }
{
    unsafe { putchar(byte) }
}

fn bump_counter(ch: &mut Channel)
    contract {
        effects(invalidates(ch.counter));
    }
{
    ch.counter = ch.counter + 1
}

fn reset_all(ch: &mut Channel)
    contract {
        effects(invalidates(ch));
    }
{
    ch.counter = 0
    ch.tag = 42
}

fn reconnect(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
        effects(rollback(ch));
    }
{
    ch.state = ChanState.Closed
    ch.state = ChanState.Open
}

fn read_tag(ch: &Channel) -> i32
    contract {
        effects(invalidates());
    }
{
    return ch.tag
}

fn main() -> i32 {
    var ch = Channel { state: ChanState.Init, counter: 0, tag: 7 }
    open(&mut ch)
    // Projection-scoped invalidation on `ch.counter` preserves `ch.state == Open`!
    bump_counter(&mut ch)
    send(&mut ch, 65)
    // Explicit history rollback `effects(rollback(ch))` permits Open -> Closed -> Open.
    reconnect(&mut ch)
    send(&mut ch, 66)
    // Hierarchical upper bound `effects(invalidates(ch))` covers all subfields.
    reset_all(&mut ch)
    let t = read_tag(&ch)
    if t == 42 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert_eq!(stdout.as_slice(), b"AB");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn projection_invalidation_and_rollback_violations_are_rejected() {
    let dir = temp_dir("projection-invalidation-reject");

    // 1. Under-declared upper bound: declares invalidates(ch.counter) but mutates ch.tag.
    check_rejected(
        &dir,
        "under_declared_projection_invalidation_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState, counter: i32, tag: i32 }
fn bad(ch: &mut Channel) contract { effects(invalidates(ch.counter)); } {
    ch.tag = 99
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Init, counter: 0, tag: 0 }
    bad(&mut ch)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Empty upper bound effects(invalidates()) cannot mutate any parameter field.
    check_rejected(
        &dir,
        "empty_invalidation_upper_bound_rejects_mutation",
        r#"
struct Boxed { x: i32 }
fn bad(b: &mut Boxed) contract { effects(invalidates()); } {
    b.x = 1
}
fn main() -> i32 {
    var b = Boxed { x: 0 }
    bad(&mut b)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Hidden history rollback H (Open -> Closed -> Open) without effects(rollback(ch)).
    check_rejected(
        &dir,
        "hidden_history_rollback_rejected",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn sneaky_reset(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    ch.state = ChanState.Closed
    ch.state = ChanState.Open
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    sneaky_reset(&mut ch)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Calling an uncontracted mutable helper cannot satisfy empty effects(invalidates()).
    check_rejected(
        &dir,
        "uncontracted_callee_violates_empty_invalidation_upper_bound",
        r#"
struct Boxed { x: i32 }
fn unknown_mut(b: &mut Boxed) {
    b.x = 42
}
fn bad(b: &mut Boxed) contract { effects(invalidates()); } {
    unknown_mut(b)
}
fn main() -> i32 {
    var b = Boxed { x: 0 }
    bad(&mut b)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 5. Unknown field name in effects(invalidates(ch.nonexistent)) rejected at typecheck.
    check_rejected(
        &dir,
        "unknown_field_in_invalidates_rejected",
        r#"
struct Boxed { x: i32 }
fn bad(b: &mut Boxed) contract { effects(invalidates(b.nonexistent)); } {}
fn main() -> i32 { return 0 }
"#,
        ResultCategory::SafeReject,
        "E-RESOURCE-RESULT-MODE",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_forged_projection_invalidation_upper_bound_is_rejected_by_checker() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, Body, ConstValue, ContractFieldPath, FieldDef, FieldIdx, FieldRef,
        FnId, FnRole, FnSig, Function, GenericArgs, Instruction, InstructionKind, IntTy, Local,
        LocalDecl, Module, Mutability, Operand, ParamEnv, Place, PlaceRoot, Projection, Region,
        ResumeMode, Rvalue, StateContractClause, Terminator, TyKind, TyTable, VariantDef,
        VariantIdx, decode_module, encode_module,
    };

    let mut tys = TyTable::new();
    let unit_ty = tys.intern(TyKind::Unit);
    let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
    let empty_args = tys.intern_args(GenericArgs {
        tys: vec![],
        regions: vec![],
    });
    let pair_adt = tys.add_adt(AdtDef {
        name: "Pair".into(),
        kind: AdtKind::Record,
        type_params: 0,
        region_params: 0,
        variants: vec![VariantDef {
            name: "Pair".into(),
            fields: vec![
                FieldDef {
                    name: "a".into(),
                    ty: i32_ty,
                },
                FieldDef {
                    name: "b".into(),
                    ty: i32_ty,
                },
            ],
        }],
        user_drop: false,
    });
    let pair_ty = tys.intern(TyKind::Adt {
        adt: pair_adt,
        args: empty_args,
    });
    let mut_pair = tys.intern(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: pair_ty,
    });

    // Function declares `effects(invalidates(p.a))` (FieldIdx(0)),
    // but body writes `(*p).b = 99` (FieldIdx(1)).
    let forged = Function {
        name: "forged_invalidation_bound".into(),
        sig: FnSig {
            params: vec![mut_pair],
            result: unit_ty,
            type_params: 0,
            region_params: 0,
            role: FnRole::Normal,
            resume_mode: ResumeMode::Local,
            state_contracts: vec![
                StateContractClause::InvalidationUpperBound,
                StateContractClause::Invalidates {
                    param: 0,
                    path: ContractFieldPath::from_slice(&[FieldIdx(0)]).unwrap(),
                },
            ],
        },
        body: Body {
            param_count: 1,
            locals: vec![LocalDecl {
                ty: mut_pair,
                name: Some("p".into()),
            }],
            captures: vec![],
            closure: None,
            result: unit_ty,
            param_env: ParamEnv::default(),
            drop_flags: vec![],
            blocks: vec![Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place {
                            root: PlaceRoot::Local(Local(0)),
                            projections: vec![
                                Projection::Deref,
                                Projection::Field(FieldRef {
                                    adt: pair_adt,
                                    args: empty_args,
                                    variant: VariantIdx(0),
                                    field: FieldIdx(1),
                                }),
                            ],
                        },
                        value: Rvalue::Use(Operand::Const(ConstValue::Int {
                            ty: IntTy::I32,
                            bits: 99,
                        })),
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
        functions: vec![forged],
        foreigns: vec![],
        statics: vec![],
        dyn_impls: vec![],
    };
    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire decode forged invalidation module");
    let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
        .expect("solver produces valid borrow certificate");
    let err = zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
        .expect_err("checker must reject forged projection invalidation upper bound");
    assert_eq!(err.function, FnId(0));
    assert!(
        matches!(err.kind, zil_check::CheckErrorKind::CertificateShape(_)),
        "expected CertificateShape rejection, got {:?}",
        err.kind
    );
}

#[test]
fn unified_call_effects_and_apart_desc_execute_natively_on_dual_backends() {
    let dir = temp_dir("unified-call-effects-native");
    let src = r#"
struct Pair { a: i32, b: i32 }

trait Reader {
    fn read(self: &Self) -> i32;
}

impl Reader for Pair {
    fn read(self: &Self) -> i32 {
        return self.a + self.b
    }
}

fn read_via_closure(p: &Pair) -> i32
    contract { effects(invalidates()); }
{
    let f = () -> i32 => p.a + p.b
    return f()
}

fn read_via_dyn(r: &OwnedDyn<dyn Reader>) -> i32
    contract { effects(invalidates()); }
{
    return r.read()
}

fn push_apart_store(a: Store<i64>, b: Store<i64>) -> i64
    contract {
        requires(apart(a, b));
        effects(invalidates(a));
    }
{
    store_push(a, 7)
    return store_len(a) + store_len(b)
}

fn main() -> i32 {
    var p = Pair { a: 10, b: 32 }
    let c = read_via_closure(&p)
    var d1: i32 = 0
    match owned_dyn::<dyn Reader>(Pair { a: 10, b: 32 }) {
        .err(_) => { return 89 }
        .ok(owner) => {
            d1 = read_via_dyn(&owner)
        }
    }
    let s1: Store<i64> = store_new()
    let s2: Store<i64> = store_new()
    if !store_reserve(s1, 2) || !store_reserve(s2, 2) {
        return 90
    }
    let total = push_apart_store(s1, s2)
    if c == 42 && d1 == 42 && total == 1 {
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
fn unified_call_effects_closure_dynslot_and_desc_alias_are_rejected() {
    let dir = temp_dir("unified-call-effects-reject");

    // 1. Closure capturing `&mut Pair` mutating `p.a` rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "closure_mut_capture_violates_empty_invalidates",
        r#"
struct Pair { a: i32, b: i32 }
fn bad(p: &mut Pair) contract { effects(invalidates()); } {
    var f = () -> i32 => {
        p.a = 1
        return p.a
    }
    let _ = f()
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Closure capturing `&mut Channel` hiding rollback Open->Closed->Open rejected.
    check_rejected(
        &dir,
        "closure_mut_capture_hides_protocol_rollback",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
fn sneaky(ch: &mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    var f = () -> i32 => {
        ch.state = ChanState.Closed
        ch.state = ChanState.Open
        return 0
    }
    let _ = f()
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    sneaky(&mut ch)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Direct `&mut OwnedDyn<dyn Mutator>` slot call rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "dynslot_mut_param_violates_empty_invalidates",
        r#"
struct Counter { n: i32 }
trait Mutator {
    fn bump(self: &mut Self) -> i32;
}
impl Mutator for Counter {
    fn bump(self: &mut Self) -> i32 {
        self.n = self.n + 1
        return self.n
    }
}
fn bad(m: &mut OwnedDyn<dyn Mutator>) contract { effects(invalidates()); } {
    let _ = m.bump()
}
fn main() -> i32 {
    match owned_dyn::<dyn Mutator>(Counter { n: 0 }) {
        .err(_) => { return 1 }
        .ok(c) => {
            var mc = c
            bad(&mut mc)
        }
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Borrowed provider `OwnedDyn<dyn Mutator, Context<'a>>` wrapping `p: &'a mut Pair`
    //    rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "dynslot_borrowed_provider_mut_param_violates_empty_invalidates",
        r#"
struct Pair { a: i32, b: i32 }
struct Context<life a> { leaf: &'a mut Pair }
struct Provider<life a> { target: &'a mut Pair }
trait Mutator {
    fn bump(self: &mut Self) -> i32;
}
impl<life a> Mutator for Provider<'a> {
    fn bump(self: &mut Self) -> i32 {
        self.target.a = self.target.a + 1
        return self.target.a
    }
}
fn bad<life a>(p: &'a mut Pair) contract { effects(invalidates()); } {
    match owned_dyn::<dyn Mutator, Context<'a>>(Provider::<'a> { target: p }) {
        .err(_) => {}
        .ok(m) => {
            var mm = m
            let _ = mm.bump()
        }
    }
}
fn main() -> i32 {
    var p = Pair { a: 0, b: 0 }
    bad(&mut p)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 5. Borrowed provider `OwnedDyn<dyn Mutator, Context<'a>>` wrapping `ch: &'a mut Channel`
    //    violates protocol state preservation / rollback.
    check_rejected(
        &dir,
        "dynslot_borrowed_provider_protocol_handle_violates_state_preservation",
        r#"
enum ChanState { Init, Open, Closed }
struct Channel { state: ChanState }
struct Context<life a> { leaf: &'a mut Channel }
struct ChanProvider<life a> { target: &'a mut Channel }
trait Mutator {
    fn flip(self: &mut Self) -> i32;
}
impl<life a> Mutator for ChanProvider<'a> {
    fn flip(self: &mut Self) -> i32 {
        self.target.state = ChanState.Closed
        self.target.state = ChanState.Open
        return 0
    }
}
fn bad<life a>(ch: &'a mut Channel)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    match owned_dyn::<dyn Mutator, Context<'a>>(ChanProvider::<'a> { target: ch }) {
        .err(_) => {}
        .ok(m) => {
            var mm = m
            let _ = mm.flip()
        }
    }
}
fn main() -> i32 {
    var ch = Channel { state: ChanState.Open }
    bad(&mut ch)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 6. Cross-parameter `Desc` aliasing without `apart(a, b)`: mutating `a` also invalidates `b`.
    check_rejected(
        &dir,
        "cross_desc_alias_without_apart_violates_invalidates_a_only",
        r#"
fn bad(a: Store<i64>, b: Store<i64>)
    contract { effects(invalidates(a)); }
{
    store_push(a, 1)
}
fn main() -> i32 {
    let s: Store<i64> = store_new()
    if !store_reserve(s, 2) { return 1 }
    bad(s, s)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_unified_call_effects_closure_dynslot_and_desc_alias_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, AggregateKind, Block, BlockId, Body, Callee, ClosureDef, ClosureKind,
        ConstValue, ContractFieldPath, DynDef, DynSlotSig, FieldDef, FieldIdx, FieldRef, FnId,
        FnRole, FnSig, Function, GenericArgs, Instruction, InstructionKind, IntTy, Local,
        LocalDecl, Module, Mutability, Operand, ParamEnv, Place, PlaceRoot, Projection,
        ProtocolStateSet, Region, ResumeMode, Rvalue, StateContractClause, Terminator, TyKind,
        TyTable, VariantDef, VariantIdx, decode_module, encode_module,
    };

    // --- Part A: Cross-parameter Desc aliasing (positive with RequiresApart, negative without,
    //     and negative hidden rollback through aliased Desc) ---
    let build_desc_alias_module = |with_apart: bool, do_rollback: bool| -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
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
        let desc_ch = tys.intern(TyKind::Desc(ch_ty));
        let a_state = Place {
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
        let mut contracts = vec![
            StateContractClause::RequiresState {
                param: 1,
                states: ProtocolStateSet::single(1),
            },
            StateContractClause::ProvidesState {
                param: 1,
                states: ProtocolStateSet::single(1),
            },
            StateContractClause::InvalidationUpperBound,
            StateContractClause::Invalidates {
                param: 0,
                path: ContractFieldPath::from_slice(&[FieldIdx(0)]).unwrap(),
            },
        ];
        if with_apart {
            contracts.push(StateContractClause::RequiresApart {
                param_a: 0,
                param_b: 1,
            });
        }
        let mut insts = vec![Instruction {
            kind: InstructionKind::Assign {
                dest: a_state.clone(),
                value: mk_variant(2),
            },
            span: zil_base::Span::default(),
        }];
        if do_rollback {
            // Allow invalidating both `a.state` and `b.state` so rejection specifically proves
            // hidden rollback detection on aliased `b`.
            contracts.push(StateContractClause::Invalidates {
                param: 1,
                path: ContractFieldPath::from_slice(&[FieldIdx(0)]).unwrap(),
            });
            insts.push(Instruction {
                kind: InstructionKind::Assign {
                    dest: a_state,
                    value: mk_variant(1),
                },
                span: zil_base::Span::default(),
            });
        }
        let func = Function {
            name: "desc_alias_probe".into(),
            sig: FnSig {
                params: vec![desc_ch, desc_ch],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: contracts,
            },
            body: Body {
                param_count: 2,
                locals: vec![
                    LocalDecl {
                        ty: desc_ch,
                        name: Some("a".into()),
                    },
                    LocalDecl {
                        ty: desc_ch,
                        name: Some("b".into()),
                    },
                ],
                captures: vec![],
                closure: None,
                result: unit_ty,
                param_env: ParamEnv::default(),
                drop_flags: vec![],
                blocks: vec![Block {
                    instructions: insts,
                    terminator: Terminator::Return {
                        value: Operand::Const(ConstValue::Unit),
                    },
                    terminator_span: zil_base::Span::default(),
                }],
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

    // A1. Positive: with RequiresApart(a, b), mutating a.state preserves b=Open.
    {
        let m = build_desc_alias_module(true, false);
        let decoded = decode_module(&encode_module(&m)).expect("decode apart desc module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve apart desc module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept disjoint Desc pair with RequiresApart");
    }
    // A2. Negative: without RequiresApart(a, b), mutating a.state violates b=Open and invalidates(a.state).
    {
        let m = build_desc_alias_module(false, false);
        let decoded = decode_module(&encode_module(&m)).expect("decode aliased desc module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve aliased desc module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject aliased Desc mutation without RequiresApart");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
    // A3. Negative: without RequiresApart(a, b), a.state = Closed; a.state = Open triggers rollback on b.
    {
        let m = build_desc_alias_module(false, true);
        let decoded = decode_module(&encode_module(&m)).expect("decode aliased desc rollback");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve aliased desc rollback");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject hidden rollback through aliased Desc parameter");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // --- Part B: Closure capture effects (positive read-only vs negative mutable capture) ---
    let build_closure_module = |capture_mut: bool| -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let cap_mut = if capture_mut {
            Mutability::Exclusive
        } else {
            Mutability::Shared
        };
        let ref_i32 = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: cap_mut,
            pointee: i32_ty,
        });
        let clo_kind = if capture_mut {
            ClosureKind::FnMut
        } else {
            ClosureKind::Fn
        };
        let clo_def = tys.add_closure(ClosureDef {
            kind: clo_kind,
            type_params: 0,
            captures: vec![ref_i32],
            params: vec![],
            result: unit_ty,
        });
        let clo_ty = tys.intern(TyKind::Closure {
            def: clo_def,
            args: empty_args,
        });
        let caller = Function {
            name: "closure_effect_probe".into(),
            sig: FnSig {
                params: vec![ref_i32],
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
                        ty: ref_i32,
                        name: Some("p".into()),
                    },
                    LocalDecl {
                        ty: clo_ty,
                        name: Some("cb".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("res".into()),
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
                                dest: Place::local(Local(1)),
                                value: Rvalue::Aggregate {
                                    kind: AggregateKind::Closure {
                                        def: clo_def,
                                        args: empty_args,
                                    },
                                    operands: vec![Operand::Move(Place::local(Local(0)))],
                                },
                            },
                            span: zil_base::Span::default(),
                        }],
                        terminator: Terminator::Call {
                            callee: Callee::Closure {
                                closure: Place::local(Local(1)),
                                mode: if capture_mut {
                                    zil_ir::body::ClosureCallMode::Mut
                                } else {
                                    zil_ir::body::ClosureCallMode::Shared
                                },
                            },
                            args: vec![],
                            dest: Place::local(Local(2)),
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
        let clo_body = Function {
            name: "closure_body".into(),
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
                locals: vec![],
                captures: vec![ref_i32],
                closure: Some(clo_def),
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
        Module {
            tys,
            functions: vec![caller, clo_body],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        }
    };

    // B1. Positive: read-only closure capturing `&i32` accepted under empty InvalidationUpperBound.
    {
        let m = build_closure_module(false);
        let decoded = decode_module(&encode_module(&m)).expect("decode readonly closure module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve readonly closure module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept read-only closure under empty InvalidationUpperBound");
    }
    // B2. Negative: mutating closure capturing `&mut i32` rejected under empty InvalidationUpperBound.
    {
        let m = build_closure_module(true);
        let decoded = decode_module(&encode_module(&m)).expect("decode mut closure module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve mut closure module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject closure with &mut capture under empty upper bound",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // --- Part C: DynSlot receiver effects (positive pure &self vs negative &mut self) ---
    let build_dynslot_module = |slot_mut: bool| -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let recv_mut = if slot_mut {
            Mutability::Exclusive
        } else {
            Mutability::Shared
        };
        let dyn_id = tys.add_dyn(DynDef {
            name: "Op".into(),
            slots: vec![DynSlotSig {
                name: "run".into(),
                receiver: Some(recv_mut),
                params: vec![],
                result: unit_ty,
            }],
        });
        let dyn_ty = tys.intern(TyKind::Dyn {
            def: dyn_id,
            regions: Vec::new(),
        });
        let ref_dyn = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: recv_mut,
            pointee: dyn_ty,
        });
        let ref_i32 = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: recv_mut,
            pointee: i32_ty,
        });
        let func = Function {
            name: "dynslot_effect_probe".into(),
            sig: FnSig {
                params: vec![ref_dyn],
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
                        ty: ref_dyn,
                        name: Some("obj".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("res".into()),
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
                            callee: Callee::DynSlot {
                                object: if slot_mut {
                                    Operand::Move(Place::local(Local(0)))
                                } else {
                                    Operand::Copy(Place::local(Local(0)))
                                },
                                slot: 0,
                            },
                            args: vec![],
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
        let slot_impl = Function {
            name: "op_run_i32".into(),
            sig: FnSig {
                params: vec![ref_i32],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::DynSlotImpl {
                    dyn_id,
                    concrete_ty: i32_ty,
                    slot: 0,
                },
                resume_mode: ResumeMode::Local,
                state_contracts: vec![],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ref_i32,
                    name: Some("self".into()),
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
        Module {
            tys,
            functions: vec![func, slot_impl],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![zil_ir::body::DynImpl {
                dyn_id,
                concrete_ty: i32_ty,
                methods: vec![FnId(1)],
            }],
        }
    };

    // C1. Positive: `&dyn Op` (`&self` slot) accepted under empty InvalidationUpperBound.
    {
        let m = build_dynslot_module(false);
        let decoded = decode_module(&encode_module(&m)).expect("decode shared dynslot module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve shared dynslot module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept shared &dyn slot under empty InvalidationUpperBound");
    }
    // C2. Negative: `&mut dyn Op` (`&mut self` slot) rejected under empty InvalidationUpperBound.
    {
        let m = build_dynslot_module(true);
        let decoded = decode_module(&encode_module(&m)).expect("decode mut dynslot module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve mut dynslot module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &mut dyn slot under empty InvalidationUpperBound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
}

#[test]
fn preconstructed_aggregate_mut_cap_controls_execute_natively_on_dual_backends() {
    let dir = temp_dir("entry-aggregate-mut-cap-native");
    let src = r#"
struct Pair { a: i32, b: i32 }
struct ReadContext<life a> { ptr: &'a Pair, tag: i32 }
struct MutContext<life a> { ptr: &'a mut Pair, tag: i32 }

fn inspect_readonly<life a>(rc: ReadContext<'a>) -> i32 {
    return rc.ptr.a + rc.tag
}

fn read_only_ok<life a>(rc: ReadContext<'a>) -> i32
    contract { effects(invalidates()); }
{
    return inspect_readonly(rc)
}

fn change<life a>(c: MutContext<'a>) {
    c.ptr.a = c.ptr.a + c.tag
}

fn precise_projection_ok<life a>(c: MutContext<'a>)
    contract { effects(invalidates(c.ptr)); }
{
    change(c)
}

fn main() -> i32 {
    var p = Pair { a: 10, b: 20 }
    let r = read_only_ok(ReadContext { ptr: &p, tag: 5 })
    precise_projection_ok(MutContext { ptr: &mut p, tag: 32 })
    if r == 15 && p.a == 42 && p.b == 20 {
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
fn preconstructed_aggregate_mut_cap_violations_are_rejected() {
    let dir = temp_dir("entry-aggregate-mut-cap-reject");

    // 1. Pre-constructed `Context<'a>` containing `&'a mut Pair` passed to unknown Direct `change(c)`
    //    MUST be rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "preconstructed_context_direct_call_violates_empty_invalidates",
        r#"
struct Pair { a: i32 }
struct Context<life a> { ptr: &'a mut Pair, tag: i32 }
fn change<life a>(c: Context<'a>) { c.ptr.a = 1 }
fn bad<life a>(c: Context<'a>) contract { effects(invalidates()); } {
    change(c)
}
fn main() -> i32 {
    var p = Pair { a: 0 }
    bad(Context { ptr: &mut p, tag: 7 })
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Disjoint field upper bound `effects(invalidates(c.tag))` must still reject mutating `c.ptr`.
    check_rejected(
        &dir,
        "preconstructed_context_direct_call_violates_disjoint_tag_invalidates",
        r#"
struct Pair { a: i32 }
struct Context<life a> { ptr: &'a mut Pair, tag: i32 }
fn change<life a>(c: Context<'a>) { c.ptr.a = 1 }
fn bad<life a>(c: Context<'a>) contract { effects(invalidates(c.tag)); } {
    change(c)
}
fn main() -> i32 {
    var p = Pair { a: 0 }
    bad(Context { ptr: &mut p, tag: 7 })
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Pre-constructed `Context<'a>` captured mutably by a closure and modified
    //    MUST be rejected by independent `zil-check` under empty upper bound.
    check_rejected(
        &dir,
        "preconstructed_context_closure_capture_violates_empty_invalidates",
        r#"
struct Pair { a: i32 }
struct Context<life a> { ptr: &'a mut Pair, tag: i32 }
fn bad<life a>(c: Context<'a>) contract { effects(invalidates()); } {
    let p = c.ptr
    var f = () -> i32 => {
        p.a = 1
        return p.a
    }
    let _ = f()
}
fn main() -> i32 {
    var p = Pair { a: 0 }
    bad(Context { ptr: &mut p, tag: 7 })
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 4. Pre-constructed `Context<'a>` passed as Dyn provider input into `owned_dyn` MUST propagate
    //    mutation effect on `DynSlot` call and reject under `effects(invalidates())`.
    check_rejected(
        &dir,
        "preconstructed_context_dyn_provider_violates_empty_invalidates",
        r#"
struct Pair { a: i32 }
struct Context<life a> { ptr: &'a mut Pair, tag: i32 }
trait Mutator {
    fn bump(self: &mut Self) -> i32;
}
impl<life a> Mutator for Context<'a> {
    fn bump(self: &mut Self) -> i32 {
        self.ptr.a = self.ptr.a + 1
        return self.ptr.a
    }
}
fn bad<life a>(c: Context<'a>) contract { effects(invalidates()); } {
    match owned_dyn::<dyn Mutator, Context<'a>>(c) {
        .err(_) => {}
        .ok(m) => {
            var mm = m
            let _ = mm.bump()
        }
    }
}
fn main() -> i32 {
    var p = Pair { a: 0 }
    bad(Context { ptr: &mut p, tag: 7 })
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_preconstructed_aggregate_mut_cap_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, ContractFieldPath, FieldDef,
        FieldIdx, FnId, FnRole, FnSig, Function, GenericArgs, IntTy, Local, LocalDecl, Module,
        Mutability, Operand, ParamEnv, Place, Region, ResumeMode, StateContractClause, Terminator,
        TyKind, TyTable, VariantDef, decode_module, encode_module,
    };

    let build_module = |declared_field: Option<u32>| -> Module {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let mut_i32 = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            pointee: i32_ty,
        });
        let ctx_adt = tys.add_adt(AdtDef {
            name: "Context".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Context".into(),
                fields: vec![
                    FieldDef {
                        name: "ptr".into(),
                        ty: mut_i32,
                    },
                    FieldDef {
                        name: "tag".into(),
                        ty: i32_ty,
                    },
                ],
            }],
            user_drop: false,
        });
        let ctx_ty = tys.intern(TyKind::Adt {
            adt: ctx_adt,
            args: empty_args,
        });
        let mut contracts = vec![StateContractClause::InvalidationUpperBound];
        if let Some(fidx) = declared_field {
            contracts.push(StateContractClause::Invalidates {
                param: 0,
                path: ContractFieldPath::from_slice(&[FieldIdx(fidx)]).unwrap(),
            });
        }
        let caller = Function {
            name: "context_caller".into(),
            sig: FnSig {
                params: vec![ctx_ty],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: contracts,
            },
            body: Body {
                param_count: 1,
                locals: vec![
                    LocalDecl {
                        ty: ctx_ty,
                        name: Some("c".into()),
                    },
                    LocalDecl {
                        ty: unit_ty,
                        name: Some("res".into()),
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
                            callee: Callee::Direct(FnId(1)),
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
        let callee = Function {
            name: "uncontracted_change".into(),
            sig: FnSig {
                params: vec![ctx_ty],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: vec![],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ctx_ty,
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
        Module {
            tys,
            functions: vec![caller, callee],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![],
        }
    };

    // 1. Forged empty upper bound MUST be rejected by independent zil-check.
    {
        let m = build_module(None);
        let decoded = decode_module(&encode_module(&m)).expect("decode empty upper ctx module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve empty upper ctx module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject pre-constructed Context under empty upper bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // 2. Disjoint field `c.tag` (field 1) MUST be rejected because `c.ptr` (field 0) is invalidated.
    {
        let m = build_module(Some(1));
        let decoded = decode_module(&encode_module(&m)).expect("decode tag upper ctx module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve tag upper ctx module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject disjoint c.tag upper bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // 3. Exact field `c.ptr` (field 0) MUST be accepted without widening to all of `c`.
    {
        let m = build_module(Some(0));
        let decoded = decode_module(&encode_module(&m)).expect("decode ptr upper ctx module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve ptr upper ctx module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept exact c.ptr invalidation upper bound");
    }
}

#[test]
fn complete_type_graph_dyn_desc_and_readonly_controls_checked() {
    let dir = temp_dir("complete-type-graph-source");

    // 1. Negative Source: `&OwnedDyn<dyn Reader>` (`&self` slot) where a concrete provider manages
    //    `Store<i64>` and mutates it via descriptor inside `read(&self)` MUST be rejected by
    //    `zil-check` under `effects(invalidates())`.
    check_rejected(
        &dir,
        "shared_dyn_with_desc_provider_violates_empty_invalidates",
        r#"
package StoreWriter { manages target: Store<i64>; }

trait Reader {
    fn read(self: &Self) -> i64;
}

impl Reader for StoreWriter {
    fn read(self: &Self) -> i64 {
        store_push(self.target, 1)
        return store_len(self.target)
    }
}

fn bad(r: &OwnedDyn<dyn Reader>) -> i64
    contract { effects(invalidates()); }
{
    return r.read()
}

fn main() -> i32 {
    let s: Store<i64> = store_new()
    if !store_reserve(s, 2) { return 1 }
    match owned_dyn::<dyn Reader>(StoreWriter { target: give s }) {
        .err(_) => { return 2 }
        .ok(owner) => {
            let _ = bad(&owner)
        }
    }
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 2. Negative Source: shared reference `&DescBox` managing `Store<i64>` passed to
    //    an uncontracted `Direct` callee MUST be rejected under `effects(invalidates())`.
    check_rejected(
        &dir,
        "shared_ref_to_desc_box_violates_empty_invalidates",
        r#"
package DescBox { manages s: Store<i64>; pub tag: i32; }

fn touch_store(b: &DescBox) {
    store_push(b.s, 7)
}

fn bad(b: &DescBox)
    contract { effects(invalidates()); }
{
    touch_store(b)
}

fn main() -> i32 {
    let s: Store<i64> = store_new()
    if !store_reserve(s, 2) { return 1 }
    let box_val = DescBox { s: give s, tag: 3 }
    bad(&box_val)
    return 0
}
"#,
        ResultCategory::SafeReject,
        "E-SAFETY-CHECK",
    );

    // 3. Positive dual-backend O0/O2 control:
    //    - Pure `&OwnedDyn<dyn PureReader>` (all impls have no `Desc`/`&mut`/mutable `Managed`) under `effects(invalidates())`.
    //    - Exact projection `effects(invalidates(b.s))` on `&DescBox` (does not widen to `b.tag`).
    let ok_src = r#"
struct PureBox { x: i64 }
package DescBox { manages s: Store<i64>; pub tag: i32; }

trait PureReader {
    fn read(self: &Self) -> i64;
}

impl PureReader for PureBox {
    fn read(self: &Self) -> i64 {
        return self.x
    }
}

fn read_pure_dyn(r: &OwnedDyn<dyn PureReader>) -> i64
    contract { effects(invalidates()); }
{
    return r.read()
}

fn push_via_box(b: &DescBox) {
    store_push(b.s, 42)
}

fn precise_desc_box_projection(b: &DescBox)
    contract { effects(invalidates(b.s)); }
{
    push_via_box(b)
}

fn main() -> i32 {
    var got: i64 = 0
    match owned_dyn::<dyn PureReader>(PureBox { x: 10 }) {
        .err(_) => { return 90 }
        .ok(owner) => {
            got = read_pure_dyn(&owner)
        }
    }
    let s: Store<i64> = store_new()
    if !store_reserve(s, 2) { return 91 }
    let b = DescBox { s: give s, tag: 5 }
    precise_desc_box_projection(&b)
    if got == 10 && store_len(b.s) == 1 && b.tag == 5 {
        return 0
    }
    return 1
}
"#;
    let entry = write_source(&dir, "ok_main.zil", ok_src);
    let (code, stdout) = build_and_run_dual_with_c(&dir, &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_complete_type_graph_param_dyn_and_opaque_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, ContractFieldPath, DynDef,
        DynImpl, DynSlotSig, FieldDef, FnId, FnRole, FnSig, Function, GenericArgs, IntTy, Local,
        LocalDecl, Module, Mutability, OpaqueTy, Operand, ParamEnv, Place, Region, ResumeMode,
        StateContractClause, Terminator, TyKind, TyTable, VariantDef, decode_module, encode_module,
    };

    // --- 1. Raw/Wire: `&Param(0)` behind shared reference passed to indirect call ---
    {
        let build_param_module = |allow_invalidation: bool| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let t_param = tys.intern(TyKind::Param(0));
            let shared_t = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Shared,
                pointee: t_param,
            });
            let fn_ptr_ty = tys.intern(TyKind::FnPtr {
                params: vec![shared_t],
                result: unit_ty,
            });
            let mut contracts = vec![StateContractClause::InvalidationUpperBound];
            if allow_invalidation {
                contracts.push(StateContractClause::Invalidates {
                    param: 0,
                    path: ContractFieldPath::from_slice(&[]).unwrap(),
                });
            }
            let caller = Function {
                name: "generic_shared_caller".into(),
                sig: FnSig {
                    params: vec![shared_t, fn_ptr_ty],
                    result: unit_ty,
                    type_params: 1,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: contracts,
                },
                body: Body {
                    param_count: 2,
                    locals: vec![
                        LocalDecl {
                            ty: shared_t,
                            name: Some("v".into()),
                        },
                        LocalDecl {
                            ty: fn_ptr_ty,
                            name: Some("f".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
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
                                callee: Callee::Indirect(Operand::Copy(Place::local(Local(1)))),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(2)),
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
                functions: vec![caller],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 1a. Negative: `&Param(0)` under empty upper bound MUST be rejected (`CertificateShape`).
        let m_bad = build_param_module(false);
        let decoded = decode_module(&encode_module(&m_bad)).expect("decode &Param bad module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve &Param bad module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &Param(0) under empty invalidation upper bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1b. Positive: `&Param(0)` with declared `Invalidates(0, [])` MUST be accepted.
        let m_ok = build_param_module(true);
        let decoded = decode_module(&encode_module(&m_ok)).expect("decode &Param ok module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve &Param ok module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept &Param(0) when invalidation is declared");
    }

    // --- 2. Raw/Wire: `&dyn Trait` + `&self` with Desc-carrying provider or empty impls ---
    {
        #[derive(Clone, Copy)]
        enum DynImplCase {
            DescProvider,
            EmptyImpls,
            PureProvider,
        }
        let build_shared_dyn_module = |case: DynImplCase| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
            let empty_args = tys.intern_args(GenericArgs {
                tys: vec![],
                regions: vec![],
            });
            let store_ty = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore { element: i64_ty }));
            let desc_store = tys.intern(TyKind::Desc(store_ty));
            let provider_adt = tys.add_adt(AdtDef {
                name: "Provider".into(),
                kind: AdtKind::Record,
                type_params: 0,
                region_params: 0,
                variants: vec![VariantDef {
                    name: "Provider".into(),
                    fields: vec![FieldDef {
                        name: "payload".into(),
                        ty: match case {
                            DynImplCase::DescProvider => desc_store,
                            DynImplCase::EmptyImpls | DynImplCase::PureProvider => i64_ty,
                        },
                    }],
                }],
                user_drop: false,
            });
            let provider_ty = tys.intern(TyKind::Adt {
                adt: provider_adt,
                args: empty_args,
            });
            let ref_provider = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Shared,
                pointee: provider_ty,
            });
            let dyn_id = tys.add_dyn(DynDef {
                name: "Act".into(),
                slots: vec![DynSlotSig {
                    name: "run".into(),
                    receiver: Some(Mutability::Shared),
                    params: vec![],
                    result: unit_ty,
                }],
            });
            let dyn_ty = tys.intern(TyKind::Dyn {
                def: dyn_id,
                regions: Vec::new(),
            });
            let ref_dyn = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Shared,
                pointee: dyn_ty,
            });
            let caller = Function {
                name: "shared_dyn_caller".into(),
                sig: FnSig {
                    params: vec![ref_dyn],
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
                            ty: ref_dyn,
                            name: Some("obj".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
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
                                callee: Callee::DynSlot {
                                    object: Operand::Copy(Place::local(Local(0))),
                                    slot: 0,
                                },
                                args: vec![],
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
            let mut functions = vec![caller];
            let mut dyn_impls = Vec::new();
            if !matches!(case, DynImplCase::EmptyImpls) {
                functions.push(Function {
                    name: "act_run_provider".into(),
                    sig: FnSig {
                        params: vec![ref_provider],
                        result: unit_ty,
                        type_params: 0,
                        region_params: 0,
                        role: FnRole::DynSlotImpl {
                            dyn_id,
                            concrete_ty: provider_ty,
                            slot: 0,
                        },
                        resume_mode: ResumeMode::Local,
                        state_contracts: vec![],
                    },
                    body: Body {
                        param_count: 1,
                        locals: vec![LocalDecl {
                            ty: ref_provider,
                            name: Some("self".into()),
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
                });
                dyn_impls.push(DynImpl {
                    dyn_id,
                    concrete_ty: provider_ty,
                    methods: vec![FnId(1)],
                });
            }
            Module {
                tys,
                functions,
                foreigns: vec![],
                statics: vec![],
                dyn_impls,
            }
        };

        // 2a. Negative: `&dyn Act` (&self) with Desc-carrying Provider MUST be rejected.
        let m_desc = build_shared_dyn_module(DynImplCase::DescProvider);
        let decoded = decode_module(&encode_module(&m_desc)).expect("decode dyn desc module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve dyn desc module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &dyn (&self) when provider holds Desc");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 2b. Negative: `&dyn Act` (&self) with empty dyn_impls MUST fail closed.
        let m_empty = build_shared_dyn_module(DynImplCase::EmptyImpls);
        let decoded = decode_module(&encode_module(&m_empty)).expect("decode dyn empty module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve dyn empty module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &dyn (&self) when no concrete impls are known");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 2c. Positive: `&dyn Act` (&self) with verifiably pure Provider MUST be accepted.
        let m_pure = build_shared_dyn_module(DynImplCase::PureProvider);
        let decoded = decode_module(&encode_module(&m_pure)).expect("decode dyn pure module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve dyn pure module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept &dyn (&self) when all concrete impls are pure");
    }

    // --- 3. Raw/Wire: Opaque payloads (`Task<Unit, Context<&mut>>` & `&Mutex<Context<Desc>>`) ---
    {
        #[derive(Clone, Copy)]
        enum OpaqueCase {
            TaskWithMutContext,
            MutexWithDescContext,
            MutexPureI64,
        }
        let build_opaque_module = |case: OpaqueCase| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
            let empty_args = tys.intern_args(GenericArgs {
                tys: vec![],
                regions: vec![],
            });
            let mut_i64 = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Exclusive,
                pointee: i64_ty,
            });
            let store_ty = tys.intern(TyKind::Opaque(OpaqueTy::OwnedStore { element: i64_ty }));
            let desc_store = tys.intern(TyKind::Desc(store_ty));
            let param_ty = match case {
                OpaqueCase::TaskWithMutContext => {
                    let ctx_adt = tys.add_adt(AdtDef {
                        name: "TaskCtx".into(),
                        kind: AdtKind::Record,
                        type_params: 0,
                        region_params: 0,
                        variants: vec![VariantDef {
                            name: "TaskCtx".into(),
                            fields: vec![FieldDef {
                                name: "ptr".into(),
                                ty: mut_i64,
                            }],
                        }],
                        user_drop: false,
                    });
                    let ctx_ty = tys.intern(TyKind::Adt {
                        adt: ctx_adt,
                        args: empty_args,
                    });
                    let task_ty = tys.intern(TyKind::Opaque(OpaqueTy::Task {
                        result: unit_ty,
                        context: Some(ctx_ty),
                    }));
                    tys.intern(TyKind::Ref {
                        region: Region::Static,
                        mutability: Mutability::Shared,
                        pointee: task_ty,
                    })
                }
                OpaqueCase::MutexWithDescContext => {
                    let ctx_adt = tys.add_adt(AdtDef {
                        name: "MutexCtx".into(),
                        kind: AdtKind::Record,
                        type_params: 0,
                        region_params: 0,
                        variants: vec![VariantDef {
                            name: "MutexCtx".into(),
                            fields: vec![FieldDef {
                                name: "s".into(),
                                ty: desc_store,
                            }],
                        }],
                        user_drop: false,
                    });
                    let ctx_ty = tys.intern(TyKind::Adt {
                        adt: ctx_adt,
                        args: empty_args,
                    });
                    let mutex_ty = tys.intern(TyKind::Opaque(OpaqueTy::Mutex { payload: ctx_ty }));
                    tys.intern(TyKind::Ref {
                        region: Region::Static,
                        mutability: Mutability::Shared,
                        pointee: mutex_ty,
                    })
                }
                OpaqueCase::MutexPureI64 => {
                    let mutex_ty = tys.intern(TyKind::Opaque(OpaqueTy::Mutex { payload: i64_ty }));
                    tys.intern(TyKind::Ref {
                        region: Region::Static,
                        mutability: Mutability::Shared,
                        pointee: mutex_ty,
                    })
                }
            };
            let caller = Function {
                name: "opaque_payload_caller".into(),
                sig: FnSig {
                    params: vec![param_ty],
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
                            ty: param_ty,
                            name: Some("h".into()),
                        },
                        LocalDecl {
                            ty: unit_ty,
                            name: Some("res".into()),
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
                                callee: Callee::Direct(FnId(1)),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
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
            let callee = Function {
                name: "unknown_opaque_callee".into(),
                sig: FnSig {
                    params: vec![param_ty],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![LocalDecl {
                        ty: param_ty,
                        name: Some("h".into()),
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
            Module {
                tys,
                functions: vec![caller, callee],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 3a. Negative: `&Task<Unit, Context<&mut>>` under empty upper bound MUST be rejected.
        let m_task = build_opaque_module(OpaqueCase::TaskWithMutContext);
        let decoded = decode_module(&encode_module(&m_task)).expect("decode Task ctx module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve Task ctx module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject Task carrying &mut context under empty bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 3b. Negative: `&Mutex<Context<Desc>>` under empty upper bound MUST be rejected.
        let m_mutex = build_opaque_module(OpaqueCase::MutexWithDescContext);
        let decoded = decode_module(&encode_module(&m_mutex)).expect("decode Mutex desc module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve Mutex desc module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject Mutex carrying Desc payload under empty bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 3c. Positive: `&Mutex<i64>` (no external Desc/&mut) accepted under empty upper bound.
        let m_pure_mutex = build_opaque_module(OpaqueCase::MutexPureI64);
        let decoded =
            decode_module(&encode_module(&m_pure_mutex)).expect("decode pure Mutex module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve pure Mutex module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept pure Mutex<i64> read-only control");
    }
}

//! End-to-end native, source-rejection, and Raw/Wire verification tests for `Managed`/describe
//! auto-capabilities and nested resource cross-parameter aliasing/`apart` contracts.

mod contract_semantics_support;

#[allow(unused_imports)]
use contract_semantics_support::{
    build_and_run_dual_with_c, check_rejected, check_rejected_with_reason, temp_dir, write_source,
};
#[allow(unused_imports)]
use zil_base::ResultCategory;

#[test]
fn managed_describe_protocol_and_projection_controls_execute_natively_on_dual_backends() {
    let dir = temp_dir("managed-describe-native");
    let src = r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }

fn flip_via_desc(ch: &Managed<Channel>) {
    let d: Channel = ch
    d.state = ChanState.Closed
    d.state = ChanState.Open
}

fn allowed_rollback(ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
        effects(rollback(ch));
    }
{
    flip_via_desc(ch)
}

fn mutate_context_handle(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}

fn precise_handle_projection(c: &Context)
    contract { effects(invalidates(c.handle)); }
{
    mutate_context_handle(c)
}

fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    allowed_rollback(&ch)
    let ctx = Context { handle: give ch, tag: 7 }
    precise_handle_projection(&ctx)
    match ctx.handle.state {
        ChanState.Closed() => {
            if ctx.tag == 7 {
                return 0
            }
        }
        ChanState.Open() => {}
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
fn managed_describe_protocol_dyn_and_projection_violations_are_rejected() {
    let dir = temp_dir("managed-describe-reject");

    // 1. `&Managed<Channel>` passed to uncontracted Direct `change(ch)` violates `provides(state(ch, Open))`.
    check_rejected_with_reason(
        &dir,
        "managed_shared_ref_direct_call_violates_state_preservation",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
fn change(ch: &Managed<Channel>) {
    let d: Channel = ch
    d.state = ChanState.Closed
}
fn bad(ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    change(ch)
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    bad(&ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 2. `&Managed<Channel>` Open -> Closed -> Open via uncontracted Direct call triggers `H` rollback rejection
    //    even when `provides(state(ch, Open, Closed))` allows both states.
    check_rejected_with_reason(
        &dir,
        "managed_shared_ref_direct_call_triggers_history_rollback_h",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
fn bounce(ch: &Managed<Channel>) {
    let d: Channel = ch
    d.state = ChanState.Closed
    d.state = ChanState.Open
}
fn bad(ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    bounce(ch)
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    bad(&ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 3. `Provider { manages channel: Channel }` via `&self` `DynSlot` mutating state MUST be rejected
    //    under `effects(invalidates())`.
    check_rejected_with_reason(
        &dir,
        "managed_dyn_provider_shared_slot_violates_empty_invalidates",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Provider { manages channel: Channel; }
trait Inspector {
    fn inspect(self: &Self) -> i32;
}
impl Inspector for Provider {
    fn inspect(self: &Self) -> i32 {
        let d: Channel = self.channel
        d.state = ChanState.Closed
        return 1
    }
}
fn bad(r: &OwnedDyn<dyn Inspector>) -> i32
    contract { effects(invalidates()); }
{
    return r.inspect()
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    match owned_dyn::<dyn Inspector>(Provider { channel: give ch }) {
        .err(_) => { return 1 }
        .ok(owner) => {
            let _ = bad(&owner)
        }
    }
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "invalidates",
    );

    // 4. Borrowed provider wrapping `ch: &'a Managed<Channel>` via `&self` `DynSlot` MUST invalidate
    //    `ch`'s protocol state and trigger rejection under `provides(state(ch, Open))`.
    check_rejected_with_reason(
        &dir,
        "managed_borrowed_dyn_provider_shared_slot_violates_protocol_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
struct Context<life a> { leaf: &'a Managed<Channel> }
struct ChanProvider<life a> { channel: &'a Managed<Channel> }
trait Inspector {
    fn inspect(self: &Self) -> i32;
}
impl<life a> Inspector for ChanProvider<'a> {
    fn inspect(self: &Self) -> i32 {
        let d: Channel = self.channel
        d.state = ChanState.Closed
        return 1
    }
}
fn bad<life a>(ch: &'a Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    match owned_dyn::<dyn Inspector, Context<'a>>(ChanProvider::<'a> { channel: ch }) {
        .err(_) => {}
        .ok(owner) => {
            let _ = owner.inspect()
        }
    }
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    bad(&ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 5. `Context { manages handle: Channel; pub tag: i32 }` under `effects(invalidates())`
    //    and under disjoint `effects(invalidates(c.tag))` MUST be rejected.
    check_rejected_with_reason(
        &dir,
        "managed_context_violates_empty_invalidates",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn change(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}
fn bad(c: &Context) contract { effects(invalidates()); } {
    change(c)
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    let ctx = Context { handle: give ch, tag: 7 }
    bad(&ctx)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "invalidates",
    );

    check_rejected_with_reason(
        &dir,
        "managed_context_violates_disjoint_tag_invalidates",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn change(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}
fn bad(c: &Context) contract { effects(invalidates(c.tag)); } {
    change(c)
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    let ctx = Context { handle: give ch, tag: 7 }
    bad(&ctx)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "invalidates",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_managed_describe_protocol_dyn_and_projection_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, Callee, ConstValue, ContractFieldPath, DynDef,
        DynImpl, DynSlotSig, FieldDef, FieldIdx, FnId, FnRole, FnSig, Function, GenericArgs, IntTy,
        Local, LocalDecl, Module, Mutability, Operand, ParamEnv, Place, ProtocolStateSet, Region,
        ResumeMode, StateContractClause, Terminator, TyKind, TyTable, VariantDef, decode_module,
        encode_module,
    };

    let make_channel_types = |tys: &mut TyTable| {
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
            kind: AdtKind::Resource,
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
        let managed_ch = tys.intern(TyKind::Managed(ch_ty));
        let ref_managed_ch = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: managed_ch,
        });
        (empty_args, managed_ch, ref_managed_ch)
    };

    // --- 1. Raw/Wire: `&Managed<Channel>` uncontracted Direct call (state preservation vs H rollback) ---
    {
        #[derive(Clone, Copy)]
        enum ManagedCallContract {
            PreserveOpen,
            WideNoRollback,
            WideWithRollback,
        }
        let build_managed_call_module = |mode: ManagedCallContract| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let (_, _, ref_managed_ch) = make_channel_types(&mut tys);
            let open = ProtocolStateSet::single(0);
            let both = open.union(ProtocolStateSet::single(1));
            let contracts = match mode {
                ManagedCallContract::PreserveOpen => vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 0,
                        states: open,
                    },
                ],
                ManagedCallContract::WideNoRollback => vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 0,
                        states: both,
                    },
                ],
                ManagedCallContract::WideWithRollback => vec![
                    StateContractClause::RequiresState {
                        param: 0,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 0,
                        states: both,
                    },
                    StateContractClause::AllowsRollback { param: 0 },
                ],
            };
            let caller = Function {
                name: "managed_caller".into(),
                sig: FnSig {
                    params: vec![ref_managed_ch],
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
                            ty: ref_managed_ch,
                            name: Some("ch".into()),
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
                name: "opaque_change".into(),
                sig: FnSig {
                    params: vec![ref_managed_ch],
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
                        ty: ref_managed_ch,
                        name: Some("ch".into()),
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

        // 1a. Negative: preserving Open across uncontracted `&Managed<Channel>` call rejected.
        let m1 = build_managed_call_module(ManagedCallContract::PreserveOpen);
        let decoded = decode_module(&encode_module(&m1)).expect("decode managed preserve module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed preserve module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject preserving Open across &Managed<Channel> call");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1b. Negative: widening to Open|Closed without AllowsRollback rejected by rollback H check.
        let m2 = build_managed_call_module(ManagedCallContract::WideNoRollback);
        let decoded = decode_module(&encode_module(&m2)).expect("decode managed rollback module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed rollback module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &Managed<Channel> call lacking AllowsRollback");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1c. Positive: Open|Closed WITH explicit AllowsRollback(0) accepted.
        let m3 = build_managed_call_module(ManagedCallContract::WideWithRollback);
        let decoded = decode_module(&encode_module(&m3)).expect("decode managed ok module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed ok module");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect(
                "checker must accept &Managed<Channel> call with ProvidesState + AllowsRollback",
            );
    }

    // --- 2. Raw/Wire: `Provider { channel: Managed<Channel> }` behind `&self` `DynSlot` ---
    {
        let mut tys = TyTable::new();
        let unit_ty = tys.intern(TyKind::Unit);
        let (empty_args, managed_ch, _) = make_channel_types(&mut tys);
        let prov_adt = tys.add_adt(AdtDef {
            name: "Provider".into(),
            kind: AdtKind::Package,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Provider".into(),
                fields: vec![FieldDef {
                    name: "channel".into(),
                    ty: managed_ch,
                }],
            }],
            user_drop: false,
        });
        let prov_ty = tys.intern(TyKind::Adt {
            adt: prov_adt,
            args: empty_args,
        });
        let ref_prov = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: prov_ty,
        });
        let dyn_id = tys.add_dyn(DynDef {
            name: "Inspect".into(),
            slots: vec![DynSlotSig {
                name: "inspect".into(),
                receiver: Some(Mutability::Shared),
                params: vec![],
                result: unit_ty,
            }],
        });
        let dyn_ty = tys.intern(TyKind::Dyn(dyn_id));
        let ref_dyn = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: dyn_ty,
        });
        let caller = Function {
            name: "managed_dyn_caller".into(),
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
                        name: Some("r".into()),
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
        let impl_fn = Function {
            name: "inspect_provider".into(),
            sig: FnSig {
                params: vec![ref_prov],
                result: unit_ty,
                type_params: 0,
                region_params: 0,
                role: FnRole::DynSlotImpl {
                    dyn_id,
                    concrete_ty: prov_ty,
                    slot: 0,
                },
                resume_mode: ResumeMode::Local,
                state_contracts: vec![],
            },
            body: Body {
                param_count: 1,
                locals: vec![LocalDecl {
                    ty: ref_prov,
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
        let m = Module {
            tys,
            functions: vec![caller, impl_fn],
            foreigns: vec![],
            statics: vec![],
            dyn_impls: vec![DynImpl {
                dyn_id,
                concrete_ty: prov_ty,
                methods: vec![FnId(1)],
            }],
        };
        let decoded = decode_module(&encode_module(&m)).expect("decode managed dyn module");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed dyn module");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject &dyn (&self) when provider holds Managed<Channel>",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }

    // --- 3. Raw/Wire: `&Context { handle: Managed<Channel>, tag: i32 }` precise field projection ---
    {
        let build_ctx_module = |declared_field: Option<u32>| -> Module {
            let mut tys = TyTable::new();
            let unit_ty = tys.intern(TyKind::Unit);
            let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
            let (empty_args, managed_ch, _) = make_channel_types(&mut tys);
            let ctx_adt = tys.add_adt(AdtDef {
                name: "Context".into(),
                kind: AdtKind::Package,
                type_params: 0,
                region_params: 0,
                variants: vec![VariantDef {
                    name: "Context".into(),
                    fields: vec![
                        FieldDef {
                            name: "handle".into(),
                            ty: managed_ch,
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
            let ref_ctx = tys.intern(TyKind::Ref {
                region: Region::Static,
                mutability: Mutability::Shared,
                pointee: ctx_ty,
            });
            let mut contracts = vec![StateContractClause::InvalidationUpperBound];
            if let Some(fi) = declared_field {
                contracts.push(StateContractClause::Invalidates {
                    param: 0,
                    path: ContractFieldPath::from_slice(&[FieldIdx(fi)]).unwrap(),
                });
            }
            let caller = Function {
                name: "managed_ctx_caller".into(),
                sig: FnSig {
                    params: vec![ref_ctx],
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
                            ty: ref_ctx,
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
                name: "opaque_ctx_callee".into(),
                sig: FnSig {
                    params: vec![ref_ctx],
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
                        ty: ref_ctx,
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

        // 3a. Negative: empty upper bound rejected.
        let m_empty = build_ctx_module(None);
        let decoded = decode_module(&encode_module(&m_empty)).expect("decode managed ctx empty");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed ctx empty");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject &Context<Managed> under empty bound");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 3b. Negative: disjoint `c.tag` (field 1) rejected.
        let m_tag = build_ctx_module(Some(1));
        let decoded = decode_module(&encode_module(&m_tag)).expect("decode managed ctx tag");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed ctx tag");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject disjoint c.tag for &Context<Managed>");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 3c. Positive: exact `c.handle` (field 0) accepted without widening to `c.tag`.
        let m_handle = build_ctx_module(Some(0));
        let decoded = decode_module(&encode_module(&m_handle)).expect("decode managed ctx handle");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed ctx handle");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept exact c.handle upper bound");
    }
}

#[test]
fn nested_resource_alias_and_apart_controls_execute_natively_on_dual_backends() {
    let dir = temp_dir("nested-resource-alias-native");
    let src = r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
resource OtherRes { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
package OtherContext { manages other: OtherRes; pub tag: i32; }
struct ContextBorrow<life a> { pub handle: &'a Managed<Channel>; pub tag: i32; }

fn mutate_context(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}

fn mutate_context_contracted(c: &Context)
    contract { effects(invalidates(c.handle)); }
{
    let d: Channel = c.handle
    d.state = ChanState.Closed
}

fn read_tag_only(c: &Context) -> i32
    contract { effects(invalidates(c.tag)); }
{
    return c.tag
}

fn mutate_other(o: &OtherContext) {
    let d: OtherRes = o.other
    d.state = ChanState.Closed
}

// 1. Explicit `requires(apart(ctx, ch))`: unannotated callee + contracted callee + inline Describe
//    all preserve `ch == Open` while closing `ctx.handle`.
fn apart_mutate_unannotated(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(ctx, ch));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_context(ctx)
}

fn apart_mutate_contracted(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(ctx, ch));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_context_contracted(ctx)
}

fn apart_mutate_inline(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(ctx, ch));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
}

// 2. Disjoint field upper bound `effects(invalidates(c.tag))` does NOT require `apart(ctx, ch)`.
fn disjoint_tag_preserves_ch(ctx: &Context, ch: &Managed<Channel>) -> i32
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    return read_tag_only(ctx)
}

// 3. Unrelated resource type (`OtherRes` vs `Channel`) does NOT require `apart`.
fn distinct_resource_type_preserves_ch(o: &OtherContext, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_other(o)
}

// 4. By-value uniquely owned `Context` cannot alias `ch: &Managed<Channel>`.
fn owned_context_preserves_ch(ctx: Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_context(&ctx)
}

// 5. Borrowed aggregate `ContextBorrow<'a>` with explicit `requires(apart(cb, ch))`.
fn apart_borrow_mutate<life a>(cb: ContextBorrow<'a>, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(cb, ch));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = cb.handle
    d.state = ChanState.Closed
}

fn main() -> i32 {
    let ch_main = Channel { state: ChanState.Open }
    let ctx1 = Context { handle: Channel { state: ChanState.Open }, tag: 11 }
    let ctx2 = Context { handle: Channel { state: ChanState.Open }, tag: 22 }
    let ctx3 = Context { handle: Channel { state: ChanState.Open }, tag: 33 }
    let other_ctx = OtherContext { other: OtherRes { state: ChanState.Open }, tag: 44 }

    let tag_val = disjoint_tag_preserves_ch(&ctx1, &ch_main)
    apart_mutate_unannotated(&ctx1, &ch_main)
    apart_mutate_contracted(&ctx2, &ch_main)
    apart_mutate_inline(&ctx3, &ch_main)
    distinct_resource_type_preserves_ch(&other_ctx, &ch_main)
    owned_context_preserves_ch(Context { handle: Channel { state: ChanState.Open }, tag: 55 }, &ch_main)

    let ch_other = Channel { state: ChanState.Open }
    let cb = ContextBorrow { handle: &ch_other, tag: 66 }
    apart_borrow_mutate(cb, &ch_main)

    match ch_main.state {
        ChanState.Open() => {
            match ctx1.handle.state {
                ChanState.Closed() => {
                    match ch_other.state {
                        ChanState.Closed() => {
                            if tag_val == 11 {
                                return 0
                            }
                        }
                        ChanState.Open() => {}
                    }
                }
                ChanState.Open() => {}
            }
        }
        ChanState.Closed() => {}
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
fn nested_resource_alias_and_apart_violations_are_rejected() {
    let dir = temp_dir("nested-resource-alias-reject");

    // 1. Exact P0-9 PoC: `&Context` holding `manages handle: Channel` + `ch: &Managed<Channel>`
    //    without `apart(ctx, ch)`: calling unannotated `mutate_context(ctx)` violates `provides(state(ch, Open))`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_unannotated_callee_violates_state_preservation",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn mutate_context(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
}
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_context(ctx)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 2. Hidden `H` rollback (`Open -> Closed -> Open`) through unannotated `mutate_context(ctx)`
    //    must be rejected even when `provides(state(ch, Open, Closed))` allows both states.
    check_rejected_with_reason(
        &dir,
        "nested_managed_unannotated_callee_triggers_hidden_rollback_h",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn bounce_context(c: &Context) {
    let d: Channel = c.handle
    d.state = ChanState.Closed
    d.state = ChanState.Open
}
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    bounce_context(ctx)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 3. Inline `Rvalue::Describe` (`let d: Channel = ctx.handle; d.state = ChanState.Closed`)
    //    must propagate `ch`'s protocol identity into `d` and reject `provides(state(ch, Open))`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_inline_describe_violates_state_preservation",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 4. Inline `Rvalue::Describe` hidden rollback (`Open -> Closed -> Open`) must be rejected by `H`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_inline_describe_triggers_hidden_rollback_h",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
    d.state = ChanState.Open
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 5. Contracted callee `effects(invalidates(c.handle))` called on `ctx` without `apart(ctx, ch)`
    //    must invalidate `ch` (both state preservation and hidden rollback).
    check_rejected_with_reason(
        &dir,
        "nested_managed_contracted_invalidates_handle_violates_ch_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn mutate_ctx(c: &Context)
    contract { effects(invalidates(c.handle)); }
{
    let d: Channel = c.handle
    d.state = ChanState.Closed
}
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    mutate_ctx(ctx)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    check_rejected_with_reason(
        &dir,
        "nested_managed_contracted_invalidates_handle_triggers_rollback_h",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn bounce_ctx(c: &Context)
    contract { effects(invalidates(c.handle)); }
{
    let d: Channel = c.handle
    d.state = ChanState.Closed
    d.state = ChanState.Open
}
fn bad(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open, ChanState.Closed));
    }
{
    bounce_ctx(ctx)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    bad(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "rollback",
    );

    // 6. By-value borrowed aggregate `ContextBorrow<'a> { pub handle: &'a Managed<Channel> }`
    //    without `apart(cb, ch)` must reject both inline Describe and callee mutation.
    check_rejected_with_reason(
        &dir,
        "nested_managed_borrowed_aggregate_inline_violates_ch_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
struct ContextBorrow<life a> { pub handle: &'a Managed<Channel>; pub tag: i32; }
fn bad<life a>(cb: ContextBorrow<'a>, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = cb.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    let cb = ContextBorrow { handle: &ch, tag: 1 }
    bad(cb, &ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 7. 2-level nested package `Outer { pub inner: Context }` inline Describe `o.inner.handle`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_two_level_projection_violates_ch_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
package Outer { pub inner: Context; pub id: i32; }
fn bad(o: &Outer, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = o.inner.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let o = Outer { inner: Context { handle: Channel { state: ChanState.Open }, tag: 1 }, id: 2 }
    bad(&o, &o.inner.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 8. `&OwnedDyn<dyn Inspector>` concrete provider holding `Managed<Channel>` + `ch: &Managed<Channel>`
    //    without `apart`: `&self` DynSlot call must invalidate `ch`.
    check_rejected_with_reason(
        &dir,
        "nested_managed_dyn_provider_alongside_ch_violates_state",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Provider { manages channel: Channel; }
trait Inspector {
    fn inspect(self: &Self) -> i32;
}
impl Inspector for Provider {
    fn inspect(self: &Self) -> i32 {
        let d: Channel = self.channel
        d.state = ChanState.Closed
        return 1
    }
}
fn bad(r: &OwnedDyn<dyn Inspector>, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    let _ = r.inspect()
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    match owned_dyn::<dyn Inspector>(Provider { channel: Channel { state: ChanState.Open } }) {
        .err(_) => { return 1 }
        .ok(owner) => {
            bad(&owner, &ch)
        }
    }
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "provides(state(...))",
    );

    // 9. Call-site `requires(apart(ctx, ch))` soundness:
    //    9a. Passing `(&ctx, &ctx.handle)` must be rejected directly by `apart`.
    check_rejected_with_reason(
        &dir,
        "apart_callsite_rejects_ctx_and_ctx_handle",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn good_apart(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(apart(ctx, ch));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    good_apart(&ctx, &ctx.handle)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "apart",
    );

    //    9b. Passing `(&cb, &ch)` where `cb = ContextBorrow { handle: &ch }` must be rejected.
    check_rejected_with_reason(
        &dir,
        "apart_callsite_rejects_borrowed_aggregate_containing_ch",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
struct ContextBorrow<life a> { pub handle: &'a Managed<Channel>; pub tag: i32; }
fn good_apart<life a>(cb: ContextBorrow<'a>, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(cb, ch));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = cb.handle
    d.state = ChanState.Closed
}
fn main() -> i32 {
    let ch = Channel { state: ChanState.Open }
    let cb = ContextBorrow { handle: &ch, tag: 1 }
    good_apart(cb, &ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "apart",
    );

    //    9c. Forwarding `(ctx, ch)` without caller declaring `requires(apart(ctx, ch))` must be rejected.
    check_rejected_with_reason(
        &dir,
        "apart_callsite_rejects_unproven_forwarded_ctx_and_ch",
        r#"
enum ChanState { Open, Closed }
resource Channel { state: ChanState }
package Context { manages handle: Channel; pub tag: i32; }
fn good_apart(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open), apart(ctx, ch));
        provides(state(ch, ChanState.Open));
    }
{
    let d: Channel = ctx.handle
    d.state = ChanState.Closed
}
fn bad_forward(ctx: &Context, ch: &Managed<Channel>)
    contract {
        requires(state(ch, ChanState.Open));
        provides(state(ch, ChanState.Open));
    }
{
    good_apart(ctx, ch)
}
fn main() -> i32 {
    let ctx = Context { handle: Channel { state: ChanState.Open }, tag: 1 }
    let ch = Channel { state: ChanState.Open }
    bad_forward(&ctx, &ch)
    return 0
}
"#,
        "E-SAFETY-CHECK",
        "apart",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn raw_wire_nested_resource_alias_and_managed_create_checked() {
    use zil_ir::body::{
        AdtDef, AdtKind, AggregateKind, Block, BlockId, Body, Callee, ConstValue, FieldDef,
        FieldIdx, FieldRef, FnId, FnRole, FnSig, Function, GenericArgs, Instruction,
        InstructionKind, IntTy, IntrinsicOp, Local, LocalDecl, Module, Mutability, Operand,
        ParamEnv, Place, PlaceRoot, Projection, ProtocolStateSet, Region, ResumeMode, Rvalue,
        StateContractClause, Terminator, TyKind, TyTable, VariantDef, VariantIdx, decode_module,
        encode_module,
    };

    let make_types = |tys: &mut TyTable| {
        let empty_args = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        let unit_ty = tys.intern(TyKind::Unit);
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let st_adt = tys.add_adt(AdtDef {
            name: "ChanState".into(),
            kind: AdtKind::Enum,
            type_params: 0,
            region_params: 0,
            variants: vec![
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
            kind: AdtKind::Resource,
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
        let managed_ch = tys.intern(TyKind::Managed(ch_ty));
        let desc_ch = tys.intern(TyKind::Desc(ch_ty));
        let ref_managed_ch = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: managed_ch,
        });
        let ctx_adt = tys.add_adt(AdtDef {
            name: "Context".into(),
            kind: AdtKind::Package,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Context".into(),
                fields: vec![
                    FieldDef {
                        name: "handle".into(),
                        ty: managed_ch,
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
        let ref_ctx = tys.intern(TyKind::Ref {
            region: Region::Static,
            mutability: Mutability::Shared,
            pointee: ctx_ty,
        });
        (
            empty_args,
            unit_ty,
            i32_ty,
            st_adt,
            st_ty,
            ch_adt,
            managed_ch,
            desc_ch,
            ref_managed_ch,
            ctx_adt,
            ctx_ty,
            ref_ctx,
        )
    };

    // --- Part 1: Raw/Wire nested `Context { handle: Managed<Channel> }` vs `ch: &Managed<Channel>` ---
    {
        #[derive(Clone, Copy)]
        enum NestedMode {
            PreserveNoApart,
            WideNoRollback,
            PreserveWithApart,
        }
        let build_call_mod = |mode: NestedMode| -> Module {
            let mut tys = TyTable::new();
            let (_, unit_ty, _, _, _, _, _, _, ref_managed_ch, _, _, ref_ctx) =
                make_types(&mut tys);
            let open = ProtocolStateSet::single(0);
            let both = open.union(ProtocolStateSet::single(1));
            let contracts = match mode {
                NestedMode::PreserveNoApart => vec![
                    StateContractClause::RequiresState {
                        param: 1,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 1,
                        states: open,
                    },
                ],
                NestedMode::WideNoRollback => vec![
                    StateContractClause::RequiresState {
                        param: 1,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 1,
                        states: both,
                    },
                ],
                NestedMode::PreserveWithApart => vec![
                    StateContractClause::RequiresState {
                        param: 1,
                        states: open,
                    },
                    StateContractClause::RequiresApart {
                        param_a: 0,
                        param_b: 1,
                    },
                    StateContractClause::ProvidesState {
                        param: 1,
                        states: open,
                    },
                ],
            };
            let caller = Function {
                name: "nested_ctx_caller".into(),
                sig: FnSig {
                    params: vec![ref_ctx, ref_managed_ch],
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
                            ty: ref_ctx,
                            name: Some("ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("ch".into()),
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
            let callee = Function {
                name: "mutate_context".into(),
                sig: FnSig {
                    params: vec![ref_ctx],
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
                        ty: ref_ctx,
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

        // 1a. Negative: unannotated call on `&Context` without `RequiresApart` violates `ch == Open`.
        let m = build_call_mod(NestedMode::PreserveNoApart);
        let decoded = decode_module(&encode_module(&m)).expect("decode nested call preserve");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve nested call preserve");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject preserving ch=Open across &Context call");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1b. Negative: widening to Open|Closed without AllowsRollback rejected by rollback H.
        let m = build_call_mod(NestedMode::WideNoRollback);
        let decoded = decode_module(&encode_module(&m)).expect("decode nested call rollback");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve nested call rollback");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject hidden rollback H on ch across &Context call");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1c. Positive: WITH explicit `RequiresApart(0, 1)`, `ch == Open` is preserved.
        let m = build_call_mod(NestedMode::PreserveWithApart);
        let decoded = decode_module(&encode_module(&m)).expect("decode nested call apart");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve nested call apart");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept preserving ch=Open when RequiresApart(ctx, ch) holds");

        // 1d & 1e. Inline `Rvalue::Describe((*ctx).handle)` + state write (preserve & rollback H).
        let build_inline_mod = |bounce_back: bool| -> Module {
            let mut tys = TyTable::new();
            let (
                empty_args,
                unit_ty,
                _,
                st_adt,
                st_ty,
                ch_adt,
                _,
                desc_ch,
                ref_managed_ch,
                ctx_adt,
                _,
                ref_ctx,
            ) = make_types(&mut tys);
            let open = ProtocolStateSet::single(0);
            let mut insts = vec![
                Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(2)),
                        value: Rvalue::Describe {
                            place: Place {
                                root: PlaceRoot::Local(Local(0)),
                                projections: vec![
                                    Projection::Deref,
                                    Projection::Field(FieldRef {
                                        adt: ctx_adt,
                                        args: empty_args,
                                        variant: VariantIdx(0),
                                        field: FieldIdx(0),
                                    }),
                                ],
                            },
                        },
                    },
                    span: zil_base::Span::default(),
                },
                Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(3)),
                        value: Rvalue::Aggregate {
                            kind: AggregateKind::Adt {
                                adt: st_adt,
                                variant: VariantIdx(1),
                                args: empty_args,
                            },
                            operands: vec![],
                        },
                    },
                    span: zil_base::Span::default(),
                },
                Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place {
                            root: PlaceRoot::Local(Local(2)),
                            projections: vec![
                                Projection::Deref,
                                Projection::Field(FieldRef {
                                    adt: ch_adt,
                                    args: empty_args,
                                    variant: VariantIdx(0),
                                    field: FieldIdx(0),
                                }),
                            ],
                        },
                        value: Rvalue::Use(Operand::Copy(Place::local(Local(3)))),
                    },
                    span: zil_base::Span::default(),
                },
            ];
            let contracts = if bounce_back {
                insts.push(Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(4)),
                        value: Rvalue::Aggregate {
                            kind: AggregateKind::Adt {
                                adt: st_adt,
                                variant: VariantIdx(0),
                                args: empty_args,
                            },
                            operands: vec![],
                        },
                    },
                    span: zil_base::Span::default(),
                });
                insts.push(Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place {
                            root: PlaceRoot::Local(Local(2)),
                            projections: vec![
                                Projection::Deref,
                                Projection::Field(FieldRef {
                                    adt: ch_adt,
                                    args: empty_args,
                                    variant: VariantIdx(0),
                                    field: FieldIdx(0),
                                }),
                            ],
                        },
                        value: Rvalue::Use(Operand::Copy(Place::local(Local(4)))),
                    },
                    span: zil_base::Span::default(),
                });
                vec![StateContractClause::RequiresState {
                    param: 1,
                    states: open,
                }]
            } else {
                vec![
                    StateContractClause::RequiresState {
                        param: 1,
                        states: open,
                    },
                    StateContractClause::ProvidesState {
                        param: 1,
                        states: open,
                    },
                ]
            };
            let func = Function {
                name: "inline_describe_nested".into(),
                sig: FnSig {
                    params: vec![ref_ctx, ref_managed_ch],
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
                            ty: ref_ctx,
                            name: Some("ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("ch".into()),
                        },
                        LocalDecl {
                            ty: desc_ch,
                            name: Some("d".into()),
                        },
                        LocalDecl {
                            ty: st_ty,
                            name: Some("closed".into()),
                        },
                        LocalDecl {
                            ty: st_ty,
                            name: Some("open".into()),
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

        // 1d. Negative: inline Describe(ctx.handle) + d.state = Closed violates ch=Open.
        let m = build_inline_mod(false);
        let decoded = decode_module(&encode_module(&m)).expect("decode inline describe preserve");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve inline describe preserve");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject inline Describe(ctx.handle) state write");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1e. Negative: inline Describe(ctx.handle) + Closed -> Open violates rollback H on ch.
        let m = build_inline_mod(true);
        let decoded = decode_module(&encode_module(&m)).expect("decode inline describe rollback");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve inline describe rollback");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err("checker must reject inline Describe(ctx.handle) hidden rollback H");
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));

        // 1f. Negative: Raw/Wire call site passing `(&ctx, &ctx.handle)` to `RequiresApart(0, 1)`.
        {
            let mut tys = TyTable::new();
            let (
                empty_args,
                unit_ty,
                i32_ty,
                st_adt,
                st_ty,
                _,
                managed_ch,
                _,
                ref_managed_ch,
                ctx_adt,
                ctx_ty,
                ref_ctx,
            ) = make_types(&mut tys);
            let caller = Function {
                name: "bad_apart_callsite".into(),
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
                            ty: st_ty,
                            name: Some("open_st".into()),
                        },
                        LocalDecl {
                            ty: managed_ch,
                            name: Some("ch".into()),
                        },
                        LocalDecl {
                            ty: ctx_ty,
                            name: Some("ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_ctx,
                            name: Some("r_ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("r_handle".into()),
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
                                    dest: Place::local(Local(0)),
                                    value: Rvalue::Aggregate {
                                        kind: AggregateKind::Adt {
                                            adt: st_adt,
                                            variant: VariantIdx(0),
                                            args: empty_args,
                                        },
                                        operands: vec![],
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::ManagedCreate),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
                                dest: Place::local(Local(1)),
                                target: BlockId(1),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![
                                Instruction {
                                    kind: InstructionKind::Assign {
                                        dest: Place::local(Local(2)),
                                        value: Rvalue::Aggregate {
                                            kind: AggregateKind::Adt {
                                                adt: ctx_adt,
                                                variant: VariantIdx(0),
                                                args: empty_args,
                                            },
                                            operands: vec![
                                                Operand::Move(Place::local(Local(1))),
                                                Operand::Const(ConstValue::Int {
                                                    ty: IntTy::I32,
                                                    bits: 1,
                                                }),
                                            ],
                                        },
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
                                Instruction {
                                    kind: InstructionKind::Assign {
                                        dest: Place::local(Local(4)),
                                        value: Rvalue::Ref {
                                            region: Region::Static,
                                            mutability: Mutability::Shared,
                                            place: Place {
                                                root: PlaceRoot::Local(Local(2)),
                                                projections: vec![Projection::Field(FieldRef {
                                                    adt: ctx_adt,
                                                    args: empty_args,
                                                    variant: VariantIdx(0),
                                                    field: FieldIdx(0),
                                                })],
                                            },
                                        },
                                    },
                                    span: zil_base::Span::default(),
                                },
                            ],
                            terminator: Terminator::Call {
                                callee: Callee::Direct(FnId(1)),
                                args: vec![
                                    Operand::Copy(Place::local(Local(3))),
                                    Operand::Copy(Place::local(Local(4))),
                                ],
                                dest: Place::local(Local(5)),
                                target: BlockId(2),
                                cancel: None,
                            },
                            terminator_span: zil_base::Span::default(),
                        },
                        Block {
                            instructions: vec![Instruction {
                                kind: InstructionKind::Drop {
                                    place: Place::local(Local(2)),
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
            let _ = i32_ty;
            let callee = Function {
                name: "needs_apart".into(),
                sig: FnSig {
                    params: vec![ref_ctx, ref_managed_ch],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![StateContractClause::RequiresApart {
                        param_a: 0,
                        param_b: 1,
                    }],
                },
                body: Body {
                    param_count: 2,
                    locals: vec![
                        LocalDecl {
                            ty: ref_ctx,
                            name: Some("ctx".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("ch".into()),
                        },
                    ],
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
            let m = Module {
                tys,
                functions: vec![caller, callee],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            };
            let decoded = decode_module(&encode_module(&m)).expect("decode bad apart callsite");
            let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
                .expect("solve bad apart callsite");
            let err = zil_check::check_module(
                decoded,
                &cert,
                zil_check::CheckBudget { steps: 1_000_000 },
            )
            .expect_err("checker must reject (&ctx, &ctx.handle) at RequiresApart callsite");
            assert_eq!(err.function, FnId(0));
            assert!(matches!(
                err.kind,
                zil_check::CheckErrorKind::CertificateShape(_)
            ));
        }
    }

    // --- Part 2: Direct `Callee::Intrinsic(IntrinsicOp::ManagedCreate)` Raw/Wire certificates ---
    {
        let build_managed_create_mod = |init_variant: u32| -> Module {
            let mut tys = TyTable::new();
            let (empty_args, unit_ty, _, st_adt, st_ty, _, managed_ch, _, ref_managed_ch, _, _, _) =
                make_types(&mut tys);
            let open = ProtocolStateSet::single(0);
            let caller = Function {
                name: "managed_create_caller".into(),
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
                            ty: st_ty,
                            name: Some("st".into()),
                        },
                        LocalDecl {
                            ty: managed_ch,
                            name: Some("ch".into()),
                        },
                        LocalDecl {
                            ty: ref_managed_ch,
                            name: Some("r_ch".into()),
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
                                    dest: Place::local(Local(0)),
                                    value: Rvalue::Aggregate {
                                        kind: AggregateKind::Adt {
                                            adt: st_adt,
                                            variant: VariantIdx(init_variant),
                                            args: empty_args,
                                        },
                                        operands: vec![],
                                    },
                                },
                                span: zil_base::Span::default(),
                            }],
                            terminator: Terminator::Call {
                                callee: Callee::Intrinsic(IntrinsicOp::ManagedCreate),
                                args: vec![Operand::Copy(Place::local(Local(0)))],
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
                                        region: Region::Static,
                                        mutability: Mutability::Shared,
                                        place: Place::local(Local(1)),
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
            };
            let require_open = Function {
                name: "require_open".into(),
                sig: FnSig {
                    params: vec![ref_managed_ch],
                    result: unit_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                    resume_mode: ResumeMode::Local,
                    state_contracts: vec![StateContractClause::RequiresState {
                        param: 0,
                        states: open,
                    }],
                },
                body: Body {
                    param_count: 1,
                    locals: vec![LocalDecl {
                        ty: ref_managed_ch,
                        name: Some("ch".into()),
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
                functions: vec![caller, require_open],
                foreigns: vec![],
                statics: vec![],
                dyn_impls: vec![],
            }
        };

        // 2a. Positive: `ManagedCreate(ChanState.Open)` satisfies `requires(state(ch, Open))`.
        let m_ok = build_managed_create_mod(0);
        let decoded = decode_module(&encode_module(&m_ok)).expect("decode managed create ok");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed create ok");
        zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
            .expect("checker must accept ManagedCreate(Open) passed to requires(state(ch, Open))");

        // 2b. Negative: `ManagedCreate(ChanState.Closed)` rejected by `requires(state(ch, Open))`.
        let m_bad = build_managed_create_mod(1);
        let decoded = decode_module(&encode_module(&m_bad)).expect("decode managed create bad");
        let cert = zil_solve::solve_module(&decoded, zil_solve::Budget { steps: 1_000_000 })
            .expect("solve managed create bad");
        let err =
            zil_check::check_module(decoded, &cert, zil_check::CheckBudget { steps: 1_000_000 })
                .expect_err(
                    "checker must reject ManagedCreate(Closed) passed to requires(state(ch, Open))",
                );
        assert_eq!(err.function, FnId(0));
        assert!(matches!(
            err.kind,
            zil_check::CheckErrorKind::CertificateShape(_)
        ));
    }
}

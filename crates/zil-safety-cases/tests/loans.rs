//! Loan family: hand-written cases.
//!
//! Expectations come from the language contract (`_zil_design.md` §2.2
//! address windows, §4 access effects, §7 lifetime end) and are written by
//! hand. No case calls the semantic kernel; verdicts come from the full
//! solve → check pipeline, and certificate cases mutate the solver output.

use zil_ir::body::*;
use zil_safety_cases::*;

fn reason(v: &Verdict) -> String {
    let text = format!("{v:?}");
    for k in [
        "LoanConflict",
        "ReturnsLocalBorrow",
        "EscapingLocalBorrow",
        "UnknownVariant",
        "UseOfUninit",
        "LeakedObligation",
        "Accepted",
    ] {
        if text.contains(k) {
            return k.to_string();
        }
    }
    text
}

fn ref_ty(m: &mut ModuleBuilder, mutability: Mutability, pointee: TyId) -> TyId {
    m.ty(TyKind::Ref {
        region: Region::Static,
        mutability,
        pointee,
    })
}

fn borrow(place: Place) -> Rvalue {
    Rvalue::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        place,
    }
}

fn borrow_mut(place: Place) -> Rvalue {
    Rvalue::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        place,
    }
}

fn set(f: &mut FnBuilder, b: BlockId, dest: Place, v: i32) {
    f.assign(b, dest, Rvalue::Use(int(v)));
}

fn call(callee: FnId, args: Vec<Operand>, dest: Place, target: BlockId) -> Terminator {
    Terminator::Call {
        callee: Callee::Direct(callee),
        args,
        dest,
        target,
        cancel: None,
    }
}

/// A callee with the given parameters that does nothing.
fn noop(m: &mut ModuleBuilder, name: &str, params: &[TyId]) -> FnId {
    let mut f = FnBuilder::new(name, params, m.t.unit);
    let b = f.block();
    f.terminate(b, ret_unit());
    m.function(f)
}

/// `x = 1; r = &x; <between>; y = *r; return y`.
fn shared_then(between: impl FnOnce(&mut FnBuilder, BlockId, Local), read_before: bool) -> Module {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let r_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let mut f = FnBuilder::new("f", &[], i32);
    let b = f.block();
    let (x, r, y) = (f.local(i32), f.local(r_ty), f.local(i32));
    set(&mut f, b, l(x), 1);
    f.assign(b, l(r), borrow(l(x)));
    if read_before {
        f.assign(b, l(y), Rvalue::Use(copy(l(r).deref())));
        between(&mut f, b, x);
    } else {
        between(&mut f, b, x);
        f.assign(b, l(y), Rvalue::Use(copy(l(r).deref())));
    }
    f.terminate(b, Terminator::Return { value: copy(l(y)) });
    m.function(f);
    m.finish()
}

#[test]
fn writing_a_place_while_a_shared_address_of_it_is_used_later_is_rejected() {
    let module = shared_then(|f, b, x| set(f, b, l(x), 2), false);
    assert_eq!(reason(&verdict(module)), "LoanConflict");
}

#[test]
fn writing_after_the_last_use_of_the_address_is_accepted() {
    let module = shared_then(|f, b, x| set(f, b, l(x), 2), true);
    assert_eq!(reason(&verdict(module)), "Accepted");
}

#[test]
fn reading_through_a_shared_address_alongside_the_owner_is_accepted() {
    let module = shared_then(
        |f, b, x| {
            let z = f.local(TyId(2));
            f.assign(b, l(z), Rvalue::Use(copy(l(x))));
        },
        false,
    );
    assert_eq!(reason(&verdict(module)), "Accepted");
}

#[test]
fn ending_storage_while_an_address_is_used_later_is_rejected() {
    let module = shared_then(|f, b, x| f.push(b, InstructionKind::StorageDead(x)), false);
    assert_eq!(reason(&verdict(module)), "LoanConflict");
}

#[test]
fn reading_the_owner_while_an_exclusive_address_is_used_later_is_rejected() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let r_ty = ref_ty(&mut m, Mutability::Exclusive, i32);
    let mut f = FnBuilder::new("f", &[], i32);
    let b = f.block();
    let (x, r, y) = (f.local(i32), f.local(r_ty), f.local(i32));
    set(&mut f, b, l(x), 1);
    f.assign(b, l(r), borrow_mut(l(x)));
    f.assign(b, l(y), Rvalue::Use(copy(l(x))));
    set(&mut f, b, l(r).deref(), 3);
    f.terminate(b, Terminator::Return { value: copy(l(y)) });
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "LoanConflict");
}

/// Two exclusive addresses of fields `a` and `b` of one record, both
/// written afterwards.
fn two_exclusive(a: u32, b_field: u32) -> Module {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let (rec, rec_ty) = m.adt("m.Pair", AdtKind::Record, false, &[("Pair", &[i32, i32])]);
    let r_ty = ref_ty(&mut m, Mutability::Exclusive, i32);
    let (fa, fb) = (m.field(rec, 0, a), m.field(rec, 0, b_field));
    let mut f = FnBuilder::new("f", &[], m.t.unit);
    let b = f.block();
    let (x, p, q) = (f.local(rec_ty), f.local(r_ty), f.local(r_ty));
    f.assign(
        b,
        l(x),
        Rvalue::Aggregate {
            kind: AggregateKind::Adt {
                adt: rec,
                variant: VariantIdx(0),
                args: m.t.empty,
            },
            operands: vec![int(1), int(2)],
        },
    );
    f.assign(b, l(p), borrow_mut(l(x).field(fa)));
    f.assign(b, l(q), borrow_mut(l(x).field(fb)));
    set(&mut f, b, l(p).deref(), 5);
    set(&mut f, b, l(q).deref(), 6);
    f.terminate(b, ret_unit());
    m.function(f);
    m.finish()
}

#[test]
fn exclusive_addresses_of_disjoint_fields_coexist() {
    assert_eq!(reason(&verdict(two_exclusive(0, 1))), "Accepted");
}

#[test]
fn two_live_exclusive_addresses_of_one_field_are_rejected() {
    assert_eq!(reason(&verdict(two_exclusive(1, 1))), "LoanConflict");
}

#[test]
fn returning_an_address_of_a_local_is_rejected() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let r_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let mut f = FnBuilder::new("f", &[], r_ty);
    let b = f.block();
    let (x, r) = (f.local(i32), f.local(r_ty));
    set(&mut f, b, l(x), 1);
    f.assign(b, l(r), borrow(l(x)));
    f.terminate(b, Terminator::Return { value: copy(l(r)) });
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "ReturnsLocalBorrow");
}

#[test]
fn returning_a_reborrow_of_a_parameter_is_accepted() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let r_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let mut f = FnBuilder::new("f", &[r_ty], r_ty);
    let b = f.block();
    let r = f.local(r_ty);
    f.assign(b, l(r), borrow(l(f.param(0)).deref()));
    f.terminate(b, Terminator::Return { value: copy(l(r)) });
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "Accepted");
}

#[test]
fn returning_a_reborrow_through_a_local_address_of_a_local_is_rejected() {
    // q = &mut x; r = &*q; return r — the reborrow still reaches x.
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let s_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let e_ty = ref_ty(&mut m, Mutability::Exclusive, i32);
    let mut f = FnBuilder::new("f", &[], s_ty);
    let b = f.block();
    let (x, q, r) = (f.local(i32), f.local(e_ty), f.local(s_ty));
    set(&mut f, b, l(x), 1);
    f.assign(b, l(q), borrow_mut(l(x)));
    f.assign(b, l(r), borrow(l(q).deref()));
    f.terminate(b, Terminator::Return { value: copy(l(r)) });
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "ReturnsLocalBorrow");
}

#[test]
fn storing_an_address_of_a_local_behind_a_parameter_is_rejected() {
    // fn f(p: &mut &i32) { let x = 1; *p = &x; }
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let s_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let p_ty = ref_ty(&mut m, Mutability::Exclusive, s_ty);
    let mut f = FnBuilder::new("f", &[p_ty], m.t.unit);
    let b = f.block();
    let (x, r) = (f.local(i32), f.local(s_ty));
    set(&mut f, b, l(x), 1);
    f.assign(b, l(r), borrow(l(x)));
    f.assign(b, l(f.param(0)).deref(), Rvalue::Use(copy(l(r))));
    f.terminate(b, ret_unit());
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "EscapingLocalBorrow");
}

#[test]
fn passing_an_address_of_a_local_with_writable_external_storage_is_rejected() {
    // fn f(p: &mut &i32) { let x = 1; put(p, &x) } — put may store it.
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let s_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let p_ty = ref_ty(&mut m, Mutability::Exclusive, s_ty);
    let put = noop(&mut m, "put", &[p_ty, s_ty]);
    let mut f = FnBuilder::new("f", &[p_ty], m.t.unit);
    let (b, done) = (f.block(), f.block());
    let (x, r, u) = (f.local(i32), f.local(s_ty), f.local(m.t.unit));
    set(&mut f, b, l(x), 1);
    f.assign(b, l(r), borrow(l(x)));
    f.terminate(
        b,
        call(put, vec![mv(l(f.param(0))), copy(l(r))], l(u), done),
    );
    f.terminate(done, ret_unit());
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "EscapingLocalBorrow");
}

/// `h = &z; put(&mut h, &x); x = 2; y = read_h ? *h : 0; return y`.
fn propagation(read_h: bool) -> Module {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let s_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let hr_ty = ref_ty(&mut m, Mutability::Exclusive, s_ty);
    let put = noop(&mut m, "put", &[hr_ty, s_ty]);
    let mut f = FnBuilder::new("f", &[], i32);
    let (b, after) = (f.block(), f.block());
    let (x, z, h, hr, v, u, y) = (
        f.local(i32),
        f.local(i32),
        f.local(s_ty),
        f.local(hr_ty),
        f.local(s_ty),
        f.local(m.t.unit),
        f.local(i32),
    );
    set(&mut f, b, l(x), 1);
    set(&mut f, b, l(z), 0);
    f.assign(b, l(h), borrow(l(z)));
    f.assign(b, l(hr), borrow_mut(l(h)));
    f.assign(b, l(v), borrow(l(x)));
    f.terminate(b, call(put, vec![mv(l(hr)), copy(l(v))], l(u), after));
    set(&mut f, after, l(x), 2);
    if read_h {
        f.assign(after, l(y), Rvalue::Use(copy(l(h).deref())));
    } else {
        set(&mut f, after, l(y), 0);
    }
    f.terminate(after, Terminator::Return { value: copy(l(y)) });
    m.function(f);
    m.finish()
}

#[test]
fn a_callee_may_store_an_argument_address_into_another_argument() {
    assert_eq!(reason(&verdict(propagation(true))), "LoanConflict");
    assert_eq!(reason(&verdict(propagation(false))), "Accepted");
}

#[test]
fn a_shared_address_of_an_address_keeps_the_inner_loan_alive() {
    // y = &x; r = &y; x = 2; z = **r
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let s_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let ss_ty = ref_ty(&mut m, Mutability::Shared, s_ty);
    let mut f = FnBuilder::new("f", &[], i32);
    let b = f.block();
    let (x, y, r, z) = (f.local(i32), f.local(s_ty), f.local(ss_ty), f.local(i32));
    set(&mut f, b, l(x), 1);
    f.assign(b, l(y), borrow(l(x)));
    f.assign(b, l(r), borrow(l(y)));
    set(&mut f, b, l(x), 2);
    f.assign(b, l(z), Rvalue::Use(copy(l(r).deref().deref())));
    f.terminate(b, Terminator::Return { value: copy(l(z)) });
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "LoanConflict");
}

struct Opt {
    m: ModuleBuilder,
    opt: AdtId,
    opt_ty: TyId,
}

fn opt() -> Opt {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let (opt, opt_ty) = m.adt(
        "m.Opt",
        AdtKind::Enum,
        false,
        &[("None", &[]), ("Some", &[i32])],
    );
    Opt { m, opt, opt_ty }
}

fn some(o: &Opt, v: i32) -> Rvalue {
    Rvalue::Aggregate {
        kind: AggregateKind::Adt {
            adt: o.opt,
            variant: VariantIdx(1),
            args: o.m.t.empty,
        },
        operands: vec![int(v)],
    }
}

#[test]
fn an_exclusive_address_passed_to_a_call_invalidates_variant_facts() {
    // x = Some(1); g(&mut x); return x.Some.0 — g may have stored None.
    let mut o = opt();
    let e_ty = ref_ty(&mut o.m, Mutability::Exclusive, o.opt_ty);
    let g = noop(&mut o.m, "g", &[e_ty]);
    let field = o.m.field(o.opt, 1, 0);
    let i32 = o.m.t.i32;
    let mut f = FnBuilder::new("f", &[], i32);
    let (b, after) = (f.block(), f.block());
    let (x, r, u) = (f.local(o.opt_ty), f.local(e_ty), f.local(o.m.t.unit));
    f.assign(b, l(x), some(&o, 1));
    f.assign(b, l(r), borrow_mut(l(x)));
    f.terminate(b, call(g, vec![mv(l(r))], l(u), after));
    f.terminate(
        after,
        Terminator::Return {
            value: copy(l(x).field(field)),
        },
    );
    o.m.function(f);
    assert_eq!(reason(&verdict(o.m.finish())), "UnknownVariant");
}

#[test]
fn reading_a_tag_while_an_exclusive_address_is_used_later_is_rejected() {
    let mut o = opt();
    let e_ty = ref_ty(&mut o.m, Mutability::Exclusive, o.opt_ty);
    let g = noop(&mut o.m, "g", &[e_ty]);
    let i32 = o.m.t.i32;
    let mut f = FnBuilder::new("f", &[], i32);
    let (b, after) = (f.block(), f.block());
    let (x, r, t, u) = (
        f.local(o.opt_ty),
        f.local(e_ty),
        f.local(i32),
        f.local(o.m.t.unit),
    );
    f.assign(b, l(x), some(&o, 1));
    f.assign(b, l(r), borrow_mut(l(x)));
    f.assign(b, l(t), Rvalue::Discriminant(l(x)));
    f.terminate(b, call(g, vec![mv(l(r))], l(u), after));
    f.terminate(after, Terminator::Return { value: copy(l(t)) });
    o.m.function(f);
    assert_eq!(reason(&verdict(o.m.finish())), "LoanConflict");
}

#[test]
fn moving_an_owner_while_its_address_is_used_later_is_rejected() {
    let mut m = ModuleBuilder::new();
    let (_, _, managed) = m.resource("m.File");
    let r_ty = ref_ty(&mut m, Mutability::Shared, managed);
    let mut sink = FnBuilder::new("sink", &[managed], m.t.unit);
    let sb = sink.block();
    sink.push(
        sb,
        InstructionKind::Drop {
            place: l(sink.param(0)),
        },
    );
    sink.terminate(sb, ret_unit());
    let sink = m.function(sink);
    let mut f = FnBuilder::new("f", &[managed], m.t.unit);
    let (b, after) = (f.block(), f.block());
    let (r, r2, u) = (f.local(r_ty), f.local(r_ty), f.local(m.t.unit));
    let x = f.param(0);
    f.assign(b, l(r), borrow(l(x)));
    f.terminate(b, call(sink, vec![mv(l(x))], l(u), after));
    f.assign(after, l(r2), Rvalue::Use(copy(l(r))));
    f.terminate(after, ret_unit());
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "LoanConflict");
}

/// `x = 1; r = &x; if flag {} else {}; <join>; y = *r; return y` — the
/// loan crosses the join, so the join invariant must claim it.
fn loan_across_join() -> Module {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let bool_ty = m.t.bool;
    let r_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let mut f = FnBuilder::new("f", &[bool_ty], i32);
    let (b0, a, c, join) = (f.block(), f.block(), f.block(), f.block());
    let (x, r, y) = (f.local(i32), f.local(r_ty), f.local(i32));
    set(&mut f, b0, l(x), 1);
    f.assign(b0, l(r), borrow(l(x)));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(f.param(0))),
            arms: vec![(1, a)],
            otherwise: c,
        },
    );
    f.terminate(a, goto(join));
    f.terminate(c, goto(join));
    f.assign(join, l(y), Rvalue::Use(copy(l(r).deref())));
    f.terminate(join, Terminator::Return { value: copy(l(y)) });
    m.function(f);
    m.finish()
}

#[test]
fn the_honest_certificate_claims_loans_across_a_join() {
    let module = loan_across_join();
    let cert = solve(&module);
    let join = &cert.functions[0].invariants[0];
    assert_eq!(
        join.loans,
        vec![LoanClaim {
            holder: Local(2),
            origin: LoanOrigin::At(OpLocation {
                block: BlockId(0),
                instruction: Some(1),
            }),
        }]
    );
    check_with(module, &cert).unwrap();
}

#[test]
fn dropping_a_carried_loan_from_an_invariant_is_rejected() {
    let module = loan_across_join();
    let mut cert = solve(&module);
    cert.functions[0].invariants[0].loans.clear();
    assert!(matches!(
        check_with(module, &cert).unwrap_err().kind,
        CheckErrorKind::EdgeNotCovered { .. }
    ));
}

#[test]
fn an_exclusive_address_renewed_every_iteration_is_accepted() {
    // loop { r = &mut x; *r = 1; if !flag break } return x
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let bool_ty = m.t.bool;
    let r_ty = ref_ty(&mut m, Mutability::Exclusive, i32);
    let mut f = FnBuilder::new("f", &[bool_ty], i32);
    let (b0, head, exit) = (f.block(), f.block(), f.block());
    let (x, r) = (f.local(i32), f.local(r_ty));
    set(&mut f, b0, l(x), 0);
    f.terminate(b0, goto(head));
    f.assign(head, l(r), borrow_mut(l(x)));
    set(&mut f, head, l(r).deref(), 1);
    f.terminate(
        head,
        Terminator::Switch {
            discr: copy(l(f.param(0))),
            arms: vec![(1, head)],
            otherwise: exit,
        },
    );
    f.terminate(exit, Terminator::Return { value: copy(l(x)) });
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "Accepted");
}

#[test]
fn an_address_kept_across_a_back_edge_blocks_the_write() {
    // r = &x; loop { x = 1; y = *r; if !flag break } return y
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let bool_ty = m.t.bool;
    let r_ty = ref_ty(&mut m, Mutability::Shared, i32);
    let mut f = FnBuilder::new("f", &[bool_ty], i32);
    let (b0, head, exit) = (f.block(), f.block(), f.block());
    let (x, r, y) = (f.local(i32), f.local(r_ty), f.local(i32));
    set(&mut f, b0, l(x), 0);
    f.assign(b0, l(r), borrow(l(x)));
    f.terminate(b0, goto(head));
    set(&mut f, head, l(x), 1);
    f.assign(head, l(y), Rvalue::Use(copy(l(r).deref())));
    f.terminate(
        head,
        Terminator::Switch {
            discr: copy(l(f.param(0))),
            arms: vec![(1, head)],
            otherwise: exit,
        },
    );
    f.terminate(exit, Terminator::Return { value: copy(l(y)) });
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "LoanConflict");
}

//! Resource creation, `impl Drop` bindings, foreign calls and the
//! Store/ByteBuffer intrinsics with ordinary call effects. Expectations come
//! from `_zil_design.md` §2 (Managed/Desc), §7 (Drop binding), §12.4 (C
//! value ABI) and the store contract; verdicts from solve → check.

use zil_ir::body::*;
use zil_safety_cases::*;

fn reason(v: &Verdict) -> String {
    let text = format!("{v:?}");
    for k in [
        "LeakedObligation",
        "UseOfUninit",
        "DropBinding",
        "cleanup entry is neither called nor named",
        "foreign signature outside the C value ABI",
        "resource payload field is not Copy",
        "store.new without a store destination",
        "TypeMismatch",
        "Unspecified",
        "Accepted",
    ] {
        if text.contains(k) {
            return k.to_string();
        }
    }
    text
}

fn call(callee: Callee, args: Vec<Operand>, dest: Place, target: BlockId) -> Terminator {
    Terminator::Call {
        callee,
        args,
        dest,
        target,
        cancel: None,
    }
}

struct Res {
    m: ModuleBuilder,
    adt: AdtId,
    managed: TyId,
    desc: TyId,
}

/// `resource Mark { code: i32 }`, optionally with a user drop.
fn mark(user_drop: bool) -> Res {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let (adt, payload) = m.adt("m.Mark", AdtKind::Resource, user_drop, &[("Mark", &[i32])]);
    let managed = m.ty(TyKind::Managed(payload));
    let desc = m.ty(TyKind::Desc(payload));
    Res {
        m,
        adt,
        managed,
        desc,
    }
}

/// `impl Drop for Mark { fn drop(self: Self) {} }`
fn drop_entry(r: &mut Res, params: &[TyId]) -> FnId {
    let unit = r.m.t.unit;
    let mut f = FnBuilder::new("Mark.drop", params, unit).drop_entry(r.adt);
    let b = f.block();
    f.terminate(b, ret_unit());
    r.m.function(f)
}

/// `fn f() { let m = Mark{code: 7}; <close m if close> }`
fn create(r: &mut Res, close: bool) {
    let unit = r.m.t.unit;
    let mut f = FnBuilder::new("f", &[], unit);
    let (b0, b1) = (f.block(), f.block());
    let m = f.local(r.managed);
    f.terminate(
        b0,
        call(
            Callee::Intrinsic(IntrinsicOp::ManagedCreate),
            vec![int(7)],
            l(m),
            b1,
        ),
    );
    if close {
        f.push(b1, InstructionKind::Drop { place: l(m) });
    }
    f.terminate(b1, ret_unit());
    r.m.function(f);
}

#[test]
fn a_created_resource_closed_at_scope_end_is_accepted() {
    let mut r = mark(true);
    let desc = r.desc;
    drop_entry(&mut r, &[desc]);
    create(&mut r, true);
    assert_eq!(reason(&verdict(r.m.finish())), "Accepted");
}

#[test]
fn a_created_resource_never_closed_is_leaked() {
    let mut r = mark(false);
    create(&mut r, false);
    assert_eq!(reason(&verdict(r.m.finish())), "LeakedObligation");
}

#[test]
fn create_arguments_follow_the_payload_fields() {
    let mut r = mark(false);
    let unit = r.m.t.unit;
    let mut f = FnBuilder::new("f", &[], unit);
    let (b0, b1) = (f.block(), f.block());
    let m = f.local(r.managed);
    f.terminate(
        b0,
        call(
            Callee::Intrinsic(IntrinsicOp::ManagedCreate),
            vec![Operand::Const(ConstValue::Bool(true))],
            l(m),
            b1,
        ),
    );
    f.push(b1, InstructionKind::Drop { place: l(m) });
    f.terminate(b1, ret_unit());
    r.m.function(f);
    assert_eq!(reason(&verdict(r.m.finish())), "TypeMismatch");
}

#[test]
fn a_resource_payload_must_be_copy() {
    let mut m = ModuleBuilder::new();
    let (_, _, inner) = m.resource("m.File");
    let (_, payload) = m.adt("m.Bad", AdtKind::Resource, false, &[("Bad", &[inner])]);
    let managed = m.ty(TyKind::Managed(payload));
    let unit = m.t.unit;
    let mut f = FnBuilder::new("f", &[inner], unit);
    let (b0, b1) = (f.block(), f.block());
    let out = f.local(managed);
    f.terminate(
        b0,
        call(
            Callee::Intrinsic(IntrinsicOp::ManagedCreate),
            vec![mv(l(f.param(0)))],
            l(out),
            b1,
        ),
    );
    f.push(b1, InstructionKind::Drop { place: l(out) });
    f.terminate(b1, ret_unit());
    m.function(f);
    assert_eq!(
        reason(&verdict(m.finish())),
        "resource payload field is not Copy"
    );
}

#[test]
fn a_user_drop_resource_needs_its_cleanup_entry() {
    let mut r = mark(true);
    create(&mut r, true);
    assert_eq!(reason(&verdict(r.m.finish())), "DropBinding");
}

#[test]
fn a_cleanup_entry_takes_exactly_the_descriptor() {
    let mut r = mark(true);
    let i32 = r.m.t.i32;
    drop_entry(&mut r, &[i32]);
    assert_eq!(reason(&verdict(r.m.finish())), "DropBinding");
}

#[test]
fn a_cleanup_entry_of_a_resource_without_user_drop_is_rejected() {
    let mut r = mark(false);
    let desc = r.desc;
    drop_entry(&mut r, &[desc]);
    assert_eq!(reason(&verdict(r.m.finish())), "DropBinding");
}

#[test]
fn two_cleanup_entries_for_one_resource_are_rejected() {
    let mut r = mark(true);
    let desc = r.desc;
    drop_entry(&mut r, &[desc]);
    drop_entry(&mut r, &[desc]);
    assert_eq!(reason(&verdict(r.m.finish())), "DropBinding");
}

#[test]
fn calling_a_cleanup_entry_directly_is_rejected() {
    let mut r = mark(true);
    let desc = r.desc;
    let entry = drop_entry(&mut r, &[desc]);
    let unit = r.m.t.unit;
    let mut f = FnBuilder::new("f", &[desc], unit);
    let (b0, b1) = (f.block(), f.block());
    let u = f.local(unit);
    f.terminate(
        b0,
        call(Callee::Direct(entry), vec![copy(l(f.param(0)))], l(u), b1),
    );
    f.terminate(b1, ret_unit());
    r.m.function(f);
    assert_eq!(
        reason(&verdict(r.m.finish())),
        "cleanup entry is neither called nor named"
    );
}

#[test]
fn naming_a_cleanup_entry_as_a_value_is_rejected() {
    let mut r = mark(true);
    let desc = r.desc;
    let entry = drop_entry(&mut r, &[desc]);
    let unit = r.m.t.unit;
    let ptr = r.m.ty(TyKind::FnPtr {
        params: vec![desc],
        result: unit,
    });
    let mut f = FnBuilder::new("f", &[], unit);
    let b0 = f.block();
    let p = f.local(ptr);
    f.assign(b0, l(p), Rvalue::FnRef(entry));
    f.terminate(b0, ret_unit());
    r.m.function(f);
    assert_eq!(
        reason(&verdict(r.m.finish())),
        "cleanup entry is neither called nor named"
    );
}

#[test]
fn a_foreign_call_with_c_scalars_is_accepted() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    m.foreigns.push(ForeignDecl {
        symbol: "putchar".into(),
        abi: ForeignAbi::C,
        params: vec![i32],
        result: i32,
        noescape_params: Vec::new(),
    });
    let mut f = FnBuilder::new("f", &[], i32);
    let (b0, b1) = (f.block(), f.block());
    let r = f.local(i32);
    f.terminate(
        b0,
        call(Callee::Foreign(ForeignId(0)), vec![int(65)], l(r), b1),
    );
    f.terminate(b1, Terminator::Return { value: copy(l(r)) });
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "Accepted");
}

#[test]
fn a_foreign_signature_with_an_address_is_rejected() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let unit = m.t.unit;
    let r_ty = m.ty(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: i32,
    });
    m.foreigns.push(ForeignDecl {
        symbol: "poke".into(),
        abi: ForeignAbi::C,
        params: vec![r_ty],
        result: unit,
        noescape_params: Vec::new(),
    });
    let mut f = FnBuilder::new("f", &[], unit);
    let (b0, b1) = (f.block(), f.block());
    let x = f.local(i32);
    let r = f.local(r_ty);
    let u = f.local(unit);
    f.assign(b0, l(x), Rvalue::Use(int(1)));
    f.assign(
        b0,
        l(r),
        Rvalue::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            place: l(x),
        },
    );
    f.terminate(
        b0,
        call(Callee::Foreign(ForeignId(0)), vec![mv(l(r))], l(u), b1),
    );
    f.terminate(b1, ret_unit());
    m.function(f);
    assert_eq!(
        reason(&verdict(m.finish())),
        "foreign signature outside the C value ABI"
    );
}

struct StoreCase {
    m: ModuleBuilder,
    pkg_ty: TyId,
    store_desc: TyId,
    store_managed: TyId,
    push_result: TyId,
}

/// `package Item { manages mark: Mark }` and `Store<Item>`.
fn store_case() -> StoreCase {
    let mut m = ModuleBuilder::new();
    let (_, _, managed) = m.resource("m.Mark");
    let (_, pkg_ty) = m.adt("m.Item", AdtKind::Package, false, &[("Item", &[managed])]);
    let store = m.ty(TyKind::Opaque(OpaqueTy::OwnedStore { element: pkg_ty }));
    let store_desc = m.ty(TyKind::Desc(store));
    let store_managed = m.ty(TyKind::Managed(store));
    let unit = m.t.unit;
    let push_result = m.ty(TyKind::ErrorUnion {
        ok: unit,
        err: pkg_ty,
    });
    m.ty(TyKind::Int(IntTy::I64));
    StoreCase {
        m,
        pkg_ty,
        store_desc,
        store_managed,
        push_result,
    }
}

/// `fn f(give item: Item) { let s = store_new(); let r = store_push(s, item);
///  <use item again if reuse>; drop r; drop s }`
fn push_item(reuse: bool) -> Module {
    let mut c = store_case();
    let unit = c.m.t.unit;
    let mut f = FnBuilder::new("f", &[c.pkg_ty], unit);
    let (b0, b1, b2, b3) = (f.block(), f.block(), f.block(), f.block());
    let s = f.local(c.store_managed);
    let d = f.local(c.store_desc);
    let r = f.local(c.push_result);
    let r2 = f.local(c.push_result);
    f.terminate(
        b0,
        call(Callee::Intrinsic(IntrinsicOp::StoreNew), vec![], l(s), b1),
    );
    f.assign(b1, l(d), Rvalue::Describe { place: l(s) });
    f.terminate(
        b1,
        call(
            Callee::Intrinsic(IntrinsicOp::StorePush),
            vec![copy(l(d)), mv(l(f.param(0)))],
            l(r),
            b2,
        ),
    );
    if reuse {
        f.terminate(
            b2,
            call(
                Callee::Intrinsic(IntrinsicOp::StorePush),
                vec![copy(l(d)), mv(l(f.param(0)))],
                l(r2),
                b3,
            ),
        );
    } else {
        f.terminate(b2, goto(b3));
    }
    f.push(b3, InstructionKind::Drop { place: l(r) });
    if reuse {
        f.push(b3, InstructionKind::Drop { place: l(r2) });
    }
    f.push(b3, InstructionKind::Drop { place: l(s) });
    f.terminate(b3, ret_unit());
    c.m.function(f);
    c.m.finish()
}

#[test]
fn pushing_a_package_into_a_store_and_closing_both_is_accepted() {
    assert_eq!(reason(&verdict(push_item(false))), "Accepted");
}

#[test]
fn a_pushed_package_is_moved() {
    assert_eq!(reason(&verdict(push_item(true))), "UseOfUninit");
}

#[test]
fn the_rejected_value_of_a_push_must_be_settled() {
    // The push result may hold the package back; forgetting it leaks.
    let mut c = store_case();
    let unit = c.m.t.unit;
    let mut f = FnBuilder::new("f", &[c.pkg_ty], unit);
    let (b0, b1, b2) = (f.block(), f.block(), f.block());
    let s = f.local(c.store_managed);
    let d = f.local(c.store_desc);
    let r = f.local(c.push_result);
    f.terminate(
        b0,
        call(Callee::Intrinsic(IntrinsicOp::StoreNew), vec![], l(s), b1),
    );
    f.assign(b1, l(d), Rvalue::Describe { place: l(s) });
    f.terminate(
        b1,
        call(
            Callee::Intrinsic(IntrinsicOp::StorePush),
            vec![copy(l(d)), mv(l(f.param(0)))],
            l(r),
            b2,
        ),
    );
    f.push(b2, InstructionKind::Drop { place: l(s) });
    f.terminate(b2, ret_unit());
    c.m.function(f);
    assert_eq!(reason(&verdict(c.m.finish())), "LeakedObligation");
}

#[test]
fn a_store_of_addresses_is_rejected() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let unit = m.t.unit;
    let r_ty = m.ty(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: i32,
    });
    let store = m.ty(TyKind::Opaque(OpaqueTy::OwnedStore { element: r_ty }));
    let managed = m.ty(TyKind::Managed(store));
    let mut f = FnBuilder::new("f", &[], unit);
    let (b0, b1) = (f.block(), f.block());
    let s = f.local(managed);
    f.terminate(
        b0,
        call(Callee::Intrinsic(IntrinsicOp::StoreNew), vec![], l(s), b1),
    );
    f.push(b1, InstructionKind::Drop { place: l(s) });
    f.terminate(b1, ret_unit());
    m.function(f);
    assert_eq!(
        reason(&verdict(m.finish())),
        "store.new without a store destination"
    );
}

#[test]
fn store_element_borrow_is_accepted_when_ended_before_drop() {
    let mut c = store_case();
    let unit = c.m.t.unit;
    let elem_ref = c.m.ty(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: c.pkg_ty,
    });
    let mut f = FnBuilder::new("f", &[], unit);
    let (b0, b1, b2) = (f.block(), f.block(), f.block());
    let s = f.local(c.store_managed);
    let d = f.local(c.store_desc);
    let r = f.local(elem_ref);
    f.terminate(
        b0,
        call(Callee::Intrinsic(IntrinsicOp::StoreNew), vec![], l(s), b1),
    );
    f.assign(b1, l(d), Rvalue::Describe { place: l(s) });
    f.terminate(
        b1,
        call(
            Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                mutability: Mutability::Shared,
            }),
            vec![
                copy(l(d)),
                Operand::Const(ConstValue::Int {
                    ty: IntTy::I64,
                    bits: 0,
                }),
            ],
            l(r),
            b2,
        ),
    );
    // `r` is dead here, so closing `s` is safe.
    f.push(b2, InstructionKind::Drop { place: l(s) });
    f.terminate(b2, ret_unit());
    c.m.function(f);
    assert_eq!(reason(&verdict(c.m.finish())), "Accepted");
}

#[test]
fn dropping_a_store_while_an_element_borrow_is_live_is_a_loan_conflict() {
    let mut c = store_case();
    let unit = c.m.t.unit;
    let elem_ref = c.m.ty(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Shared,
        pointee: c.pkg_ty,
    });
    let mut f = FnBuilder::new("f", &[], unit);
    let (b0, b1, b2) = (f.block(), f.block(), f.block());
    let s = f.local(c.store_managed);
    let d = f.local(c.store_desc);
    let r = f.local(elem_ref);
    let r2 = f.local(elem_ref);
    f.terminate(
        b0,
        call(Callee::Intrinsic(IntrinsicOp::StoreNew), vec![], l(s), b1),
    );
    f.assign(b1, l(d), Rvalue::Describe { place: l(s) });
    f.terminate(
        b1,
        call(
            Callee::Intrinsic(IntrinsicOp::StoreBorrow {
                mutability: Mutability::Shared,
            }),
            vec![
                copy(l(d)),
                Operand::Const(ConstValue::Int {
                    ty: IntTy::I64,
                    bits: 0,
                }),
            ],
            l(r),
            b2,
        ),
    );
    f.push(b2, InstructionKind::Drop { place: l(s) });
    // `r` is still live across the drop of `s`.
    f.assign(b2, l(r2), Rvalue::Use(copy(l(r))));
    f.terminate(b2, ret_unit());
    c.m.function(f);
    assert!(
        format!("{:?}", verdict(c.m.finish())).contains("LoanConflict"),
        "expected LoanConflict"
    );
}

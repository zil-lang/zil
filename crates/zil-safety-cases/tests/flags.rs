//! Conditional cleanup through declared drop flags, and the shared rule
//! that facts keyed by a local die with any write into that local.
//! Expectations come from `_zil_design.md` §7 (conditional closing by
//! control flow, tag or flag) and `spec/semantic-kernel.md` §5.

use zil_ir::body::*;
use zil_safety_cases::*;

struct Pkgs {
    m: ModuleBuilder,
    pkg_ty: TyId,
    consume: FnId,
}

/// `package Owner { manages file: File; count: i32 }` and
/// `fn consume(give o: Owner) { drop o }`.
fn pkgs() -> Pkgs {
    let mut m = ModuleBuilder::new();
    let (_, _, managed) = m.resource("m.File");
    let i32 = m.t.i32;
    let (_, pkg_ty) = m.adt(
        "m.Owner",
        AdtKind::Package,
        false,
        &[("Owner", &[managed, i32])],
    );
    let mut f = FnBuilder::new("consume", &[pkg_ty], m.t.unit);
    let b = f.block();
    f.push(
        b,
        InstructionKind::Drop {
            place: l(f.param(0)),
        },
    );
    f.terminate(b, ret_unit());
    let consume = m.function(f);
    Pkgs { m, pkg_ty, consume }
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

fn boolean(b: bool) -> Rvalue {
    Rvalue::Use(Operand::Const(ConstValue::Bool(b)))
}

fn reason(v: &Verdict) -> String {
    let text = format!("{v:?}");
    for k in [
        "UseOfMaybeUninit",
        "UseOfUninit",
        "LeakedObligation",
        "ReachableUnreachable",
        "DropFlagDecl",
        "EdgeNotCovered",
        "UnknownPlace",
        "Accepted",
    ] {
        if text.contains(k) {
            return k.to_string();
        }
    }
    text
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// The flag is cleared after the conditional give.
    Correct,
    /// The give path forgets to clear the flag.
    ForgetClear,
    /// The cleanup runs on the flag's false arm.
    InvertedTest,
}

/// `fn f(give o: Owner, c: bool) { if c { consume(o) } }` with the scope
/// exit closing `o` under its drop flag:
/// ```text
/// b0:   flag = true; switch c [1: give] otherwise join
/// give: consume(move o) -> cleared
/// cleared: flag = false (unless ForgetClear); goto join
/// join: switch flag [0: exit] otherwise close   (arms swapped if inverted)
/// close: drop o; goto exit
/// exit: return
/// ```
fn conditional_give(shape: Shape) -> Module {
    let mut p = pkgs();
    let bool_ty = p.m.t.bool;
    let mut f = FnBuilder::new("f", &[p.pkg_ty, bool_ty], p.m.t.unit);
    let (b0, give, cleared, join, close, exit) = (
        f.block(),
        f.block(),
        f.block(),
        f.block(),
        f.block(),
        f.block(),
    );
    let flag = f.local(bool_ty);
    let u = f.local(p.m.t.unit);
    let o = f.param(0);
    f.drop_flag(flag, l(o));
    f.assign(b0, l(flag), boolean(true));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(f.param(1))),
            arms: vec![(1, give)],
            otherwise: join,
        },
    );
    f.terminate(give, call(p.consume, vec![mv(l(o))], l(u), cleared));
    if shape != Shape::ForgetClear {
        f.assign(cleared, l(flag), boolean(false));
    }
    f.terminate(cleared, goto(join));
    let (on_false, on_true) = match shape {
        Shape::InvertedTest => (close, exit),
        Shape::Correct | Shape::ForgetClear => (exit, close),
    };
    f.terminate(
        join,
        Terminator::Switch {
            discr: copy(l(flag)),
            arms: vec![(0, on_false)],
            otherwise: on_true,
        },
    );
    f.push(close, InstructionKind::Drop { place: l(o) });
    f.terminate(close, goto(exit));
    f.terminate(exit, ret_unit());
    p.m.function(f);
    p.m.finish()
}

#[test]
fn a_flag_guarded_close_after_a_conditional_give_is_accepted() {
    assert_eq!(
        reason(&verdict(conditional_give(Shape::Correct))),
        "Accepted"
    );
}

#[test]
fn a_flag_not_cleared_on_the_give_path_proves_nothing() {
    // `o` stays maybe-initialized on both arms: the close uses it, the
    // exit leaks it. Which is reported first depends on visit order.
    let r = reason(&verdict(conditional_give(Shape::ForgetClear)));
    assert!(r == "UseOfMaybeUninit" || r == "LeakedObligation", "{r}");
}

#[test]
fn closing_on_the_false_arm_of_the_flag_is_a_use_of_a_moved_value() {
    assert_eq!(
        reason(&verdict(conditional_give(Shape::InvertedTest))),
        "UseOfUninit"
    );
}

#[test]
fn a_flag_claim_not_established_on_every_edge_is_rejected_by_the_checker() {
    // The certificate of the correct program claims the flag at the join;
    // replayed against the program that forgets to clear it, the give edge
    // does not cover the claim.
    let cert = solve(&conditional_give(Shape::Correct));
    let err = check_with(conditional_give(Shape::ForgetClear), &cert).unwrap_err();
    assert!(
        matches!(err.kind, CheckErrorKind::EdgeNotCovered { .. }),
        "{err:?}"
    );
}

#[test]
fn a_flag_claim_naming_an_undeclared_pair_is_rejected() {
    let module = conditional_give(Shape::Correct);
    let mut cert = solve(&module);
    let join = cert.functions[1]
        .invariants
        .iter_mut()
        .find(|i| i.block == BlockId(3))
        .expect("join is certified");
    assert_eq!(join.flags.len(), 1, "the solver states the flag fact");
    join.flags[0].place = l(Local(1));
    let err = check_with(module, &cert).unwrap_err();
    assert_eq!(err.kind, CheckErrorKind::UnknownPlace);
}

#[test]
fn a_flag_that_is_not_bool_is_a_malformed_declaration() {
    let mut p = pkgs();
    let i32 = p.m.t.i32;
    let mut f = FnBuilder::new("f", &[p.pkg_ty], p.m.t.unit);
    let b0 = f.block();
    let flag = f.local(i32);
    f.drop_flag(flag, l(f.param(0)));
    f.push(
        b0,
        InstructionKind::Drop {
            place: l(f.param(0)),
        },
    );
    f.terminate(b0, ret_unit());
    p.m.function(f);
    assert_eq!(reason(&verdict(p.m.finish())), "DropFlagDecl");
}

#[test]
fn writing_the_flag_through_an_address_forgets_the_correlation() {
    // flag = true; r = &mut flag; *r = false; switch flag [0: exit] otherwise close
    let mut p = pkgs();
    let bool_ty = p.m.t.bool;
    let r_ty = p.m.ty(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: bool_ty,
    });
    let mut f = FnBuilder::new("f", &[p.pkg_ty], p.m.t.unit);
    let (b0, close, exit) = (f.block(), f.block(), f.block());
    let flag = f.local(bool_ty);
    let r = f.local(r_ty);
    let o = f.param(0);
    f.drop_flag(flag, l(o));
    f.assign(b0, l(flag), boolean(true));
    f.assign(
        b0,
        l(r),
        Rvalue::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            place: l(flag),
        },
    );
    f.assign(b0, l(r).deref(), boolean(false));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(flag)),
            arms: vec![(0, exit)],
            otherwise: close,
        },
    );
    f.push(close, InstructionKind::Drop { place: l(o) });
    f.terminate(close, goto(exit));
    f.terminate(exit, ret_unit());
    p.m.function(f);
    // Without the correlation `o` is still live on the false arm.
    assert_eq!(reason(&verdict(p.m.finish())), "LeakedObligation");
}

#[test]
fn writing_a_tag_local_through_an_address_forgets_the_tag() {
    // d = tag(e); r = &mut d; *r = 7; switch d [0, 1] otherwise unreachable
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let (_, e_ty) = m.adt("m.E", AdtKind::Enum, false, &[("A", &[i32]), ("B", &[i32])]);
    let r_ty = m.ty(TyKind::Ref {
        region: Region::Static,
        mutability: Mutability::Exclusive,
        pointee: i32,
    });
    let mut f = FnBuilder::new("f", &[e_ty], i32);
    let (b0, a, b, other) = (f.block(), f.block(), f.block(), f.block());
    let d = f.local(i32);
    let r = f.local(r_ty);
    f.assign(b0, l(d), Rvalue::Discriminant(l(f.param(0))));
    f.assign(
        b0,
        l(r),
        Rvalue::Ref {
            region: Region::Static,
            mutability: Mutability::Exclusive,
            place: l(d),
        },
    );
    f.assign(b0, l(r).deref(), Rvalue::Use(int(7)));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(d)),
            arms: vec![(0, a), (1, b)],
            otherwise: other,
        },
    );
    f.terminate(a, Terminator::Return { value: int(0) });
    f.terminate(b, Terminator::Return { value: int(1) });
    f.terminate(other, Terminator::Unreachable);
    m.function(f);
    assert_eq!(reason(&verdict(m.finish())), "ReachableUnreachable");
}

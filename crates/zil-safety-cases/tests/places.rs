//! Place / initialization / move / variant family: hand-written cases.
//!
//! Each expectation comes from the language contract (`_zil_design.md` §2.3,
//! §2.4, §3.3, §4, §7) and is written by hand. No case calls the semantic
//! kernel; verdicts come from the full solve → check pipeline.

use zil_ir::body::*;
use zil_safety_cases::*;

struct Pkgs {
    m: ModuleBuilder,
    pkg: AdtId,
    pkg_ty: TyId,
    managed: TyId,
    consume: FnId,
}

/// `package Owner { manages file: File; count: i32 }` plus
/// `fn consume(give o: Owner) { drop o }`.
fn pkgs(user_drop: bool) -> Pkgs {
    let mut m = ModuleBuilder::new();
    let (_, _, managed) = m.resource("m.File");
    let i32 = m.t.i32;
    let (pkg, pkg_ty) = m.adt(
        "m.Owner",
        AdtKind::Package,
        user_drop,
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
    Pkgs {
        m,
        pkg,
        pkg_ty,
        managed,
        consume,
    }
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

/// The rejection reason by name. The case crate does not depend on the
/// kernel, so reasons are matched on their rendered form.
fn violation(v: &Verdict) -> String {
    let text = format!("{v:?}");
    for k in [
        "UseOfMaybeUninit",
        "UseOfUninit",
        "LeakedObligation",
        "OverwriteLive",
        "UnknownVariant",
        "ReachableUnreachable",
        "WriteIntoUninitParent",
        "WriteThroughShared",
        "MoveFromBorrowed",
        "PartialMoveOfDropValue",
        "OverwriteThroughIndirection",
        "OperandClass",
        "Accepted",
    ] {
        if text.contains(k) {
            return k.to_string();
        }
    }
    text
}

#[test]
fn move_then_use_is_rejected() {
    let mut p = pkgs(false);
    let mut f = FnBuilder::new("f", &[p.pkg_ty], p.m.t.unit);
    let (b0, b1, b2) = (f.block(), f.block(), f.block());
    let u0 = f.local(p.m.t.unit);
    let u1 = f.local(p.m.t.unit);
    f.terminate(b0, call(p.consume, vec![mv(l(f.param(0)))], l(u0), b1));
    f.terminate(b1, call(p.consume, vec![mv(l(f.param(0)))], l(u1), b2));
    f.terminate(b2, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "UseOfUninit");
}

#[test]
fn giving_the_parameter_once_is_accepted() {
    let mut p = pkgs(false);
    let mut f = FnBuilder::new("f", &[p.pkg_ty], p.m.t.unit);
    let (b0, b1) = (f.block(), f.block());
    let u0 = f.local(p.m.t.unit);
    f.terminate(b0, call(p.consume, vec![mv(l(f.param(0)))], l(u0), b1));
    f.terminate(b1, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "Accepted");
}

#[test]
fn move_on_one_branch_then_use_after_join_is_rejected() {
    let mut p = pkgs(false);
    let bool_ty = p.m.t.bool;
    let mut f = FnBuilder::new("f", &[p.pkg_ty, bool_ty], p.m.t.unit);
    let (b0, b1, b2, b3, b4) = (f.block(), f.block(), f.block(), f.block(), f.block());
    let u0 = f.local(p.m.t.unit);
    let u1 = f.local(p.m.t.unit);
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(f.param(1))),
            arms: vec![(1, b1)],
            otherwise: b2,
        },
    );
    f.terminate(b1, call(p.consume, vec![mv(l(f.param(0)))], l(u0), b3));
    f.terminate(b2, goto(b3));
    f.terminate(b3, call(p.consume, vec![mv(l(f.param(0)))], l(u1), b4));
    f.terminate(b4, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "UseOfMaybeUninit");
}

#[test]
fn branch_local_obligations_must_settle_on_every_path() {
    // One branch gives the value away, the other forgets it.
    let mut p = pkgs(false);
    let bool_ty = p.m.t.bool;
    let mut f = FnBuilder::new("f", &[p.pkg_ty, bool_ty], p.m.t.unit);
    let (b0, b1, b2, b3) = (f.block(), f.block(), f.block(), f.block());
    let u0 = f.local(p.m.t.unit);
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(f.param(1))),
            arms: vec![(1, b1)],
            otherwise: b2,
        },
    );
    f.terminate(b1, call(p.consume, vec![mv(l(f.param(0)))], l(u0), b3));
    f.terminate(b2, goto(b3));
    f.terminate(b3, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "LeakedObligation");
}

#[test]
fn returning_without_settling_a_parameter_is_rejected() {
    let mut p = pkgs(false);
    let mut f = FnBuilder::new("f", &[p.pkg_ty], p.m.t.unit);
    let b0 = f.block();
    f.terminate(b0, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "LeakedObligation");
}

#[test]
fn partial_move_leaves_only_the_remaining_parts_owed() {
    // give o.file away; o.count is Copy, so nothing remains owed.
    let mut p = pkgs(false);
    let mut take = FnBuilder::new("take_file", &[p.managed], p.m.t.unit);
    let tb = take.block();
    take.push(
        tb,
        InstructionKind::Drop {
            place: l(take.param(0)),
        },
    );
    take.terminate(tb, ret_unit());
    let take = p.m.function(take);
    let file = p.m.field(p.pkg, 0, 0);
    let mut f = FnBuilder::new("f", &[p.pkg_ty], p.m.t.unit);
    let (b0, b1) = (f.block(), f.block());
    let u0 = f.local(p.m.t.unit);
    f.terminate(
        b0,
        call(take, vec![mv(l(f.param(0)).field(file))], l(u0), b1),
    );
    f.terminate(b1, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "Accepted");
}

#[test]
fn partial_move_does_not_forgive_other_managed_fields() {
    // Two managed fields; giving one away leaves the other owed.
    let mut m = ModuleBuilder::new();
    let (_, _, managed) = m.resource("m.File");
    let (pkg, pkg_ty) = m.adt(
        "m.Two",
        AdtKind::Package,
        false,
        &[("Two", &[managed, managed])],
    );
    let mut take = FnBuilder::new("take_file", &[managed], m.t.unit);
    let tb = take.block();
    take.push(
        tb,
        InstructionKind::Drop {
            place: l(take.param(0)),
        },
    );
    take.terminate(tb, ret_unit());
    let take = m.function(take);
    let first = m.field(pkg, 0, 0);
    let mut f = FnBuilder::new("f", &[pkg_ty], m.t.unit);
    let (b0, b1) = (f.block(), f.block());
    let u0 = f.local(m.t.unit);
    f.terminate(
        b0,
        call(take, vec![mv(l(f.param(0)).field(first))], l(u0), b1),
    );
    f.terminate(b1, ret_unit());
    m.function(f);
    let v = verdict(m.finish());
    assert_eq!(violation(&v), "LeakedObligation");
    assert!(
        format!("{v:?}").contains("FieldIdx(1)"),
        "the second field is the leak: {v:?}"
    );
}

#[test]
fn refilling_a_moved_field_restores_the_whole_value() {
    let mut p = pkgs(false);
    let file = p.m.field(p.pkg, 0, 0);
    let managed = p.managed;
    let mut take = FnBuilder::new("take_file", &[managed], p.m.t.unit);
    let tb = take.block();
    take.push(
        tb,
        InstructionKind::Drop {
            place: l(take.param(0)),
        },
    );
    take.terminate(tb, ret_unit());
    let take = p.m.function(take);
    let mut f = FnBuilder::new("f", &[p.pkg_ty, managed], p.m.t.unit);
    let (b0, b1, b2) = (f.block(), f.block(), f.block());
    let u0 = f.local(p.m.t.unit);
    let u1 = f.local(p.m.t.unit);
    let o = f.param(0);
    f.terminate(b0, call(take, vec![mv(l(o).field(file))], l(u0), b1));
    f.assign(b1, l(o).field(file), Rvalue::Use(mv(l(f.param(1)))));
    f.terminate(b1, call(p.consume, vec![mv(l(o))], l(u1), b2));
    f.terminate(b2, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "Accepted");
}

#[test]
fn overwriting_a_live_owning_value_is_rejected() {
    let mut p = pkgs(false);
    let mut f = FnBuilder::new("f", &[p.pkg_ty, p.pkg_ty], p.m.t.unit);
    let b0 = f.block();
    f.assign(b0, l(f.param(0)), Rvalue::Use(mv(l(f.param(1)))));
    f.push(
        b0,
        InstructionKind::Drop {
            place: l(f.param(0)),
        },
    );
    f.terminate(b0, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "OverwriteLive");
}

#[test]
fn replace_swaps_a_live_value_and_hands_back_the_old_one() {
    let mut p = pkgs(false);
    let mut f = FnBuilder::new("f", &[p.pkg_ty, p.pkg_ty], p.m.t.unit);
    let b0 = f.block();
    let old = f.local(p.pkg_ty);
    f.push(
        b0,
        InstructionKind::Replace {
            place: l(f.param(0)),
            value: mv(l(f.param(1))),
            old,
        },
    );
    f.push(b0, InstructionKind::Drop { place: l(old) });
    f.push(
        b0,
        InstructionKind::Drop {
            place: l(f.param(0)),
        },
    );
    f.terminate(b0, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "Accepted");
}

#[test]
fn partial_move_out_of_a_value_with_user_drop_is_rejected() {
    let mut p = pkgs(true);
    let file = p.m.field(p.pkg, 0, 0);
    let mut f = FnBuilder::new("f", &[p.pkg_ty], p.managed);
    let b0 = f.block();
    f.terminate(
        b0,
        Terminator::Return {
            value: mv(l(f.param(0)).field(file)),
        },
    );
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "PartialMoveOfDropValue");
}

#[test]
fn storage_end_of_a_live_owning_local_is_rejected() {
    let mut p = pkgs(false);
    let pkg_ty = p.pkg_ty;
    let mut f = FnBuilder::new("f", &[pkg_ty], p.m.t.unit);
    let b0 = f.block();
    let x = f.local(pkg_ty);
    f.push(b0, InstructionKind::StorageLive(x));
    f.assign(b0, l(x), Rvalue::Use(mv(l(f.param(0)))));
    f.push(b0, InstructionKind::StorageDead(x));
    f.terminate(b0, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "LeakedObligation");
}

#[test]
fn moving_inside_a_loop_body_is_a_use_after_move_on_the_back_edge() {
    let mut p = pkgs(false);
    let bool_ty = p.m.t.bool;
    let mut f = FnBuilder::new("f", &[p.pkg_ty, bool_ty], p.m.t.unit);
    let (b0, head, body, exit) = (f.block(), f.block(), f.block(), f.block());
    let u0 = f.local(p.m.t.unit);
    f.terminate(b0, goto(head));
    f.terminate(
        head,
        Terminator::Switch {
            discr: copy(l(f.param(1))),
            arms: vec![(1, body)],
            otherwise: exit,
        },
    );
    f.terminate(body, call(p.consume, vec![mv(l(f.param(0)))], l(u0), head));
    // The zero-iteration path settles the value at the exit.
    f.push(
        exit,
        InstructionKind::Drop {
            place: l(f.param(0)),
        },
    );
    f.terminate(exit, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "UseOfMaybeUninit");
}

#[test]
fn copy_values_can_be_read_any_number_of_times() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let (pair, pair_ty) = m.adt("m.Pair", AdtKind::Record, false, &[("Pair", &[i32, i32])]);
    let a = m.field(pair, 0, 0);
    let mut f = FnBuilder::new("f", &[pair_ty], i32);
    let b0 = f.block();
    let s = f.local(i32);
    f.assign(
        b0,
        l(s),
        Rvalue::Binary {
            op: BinOp::Add,
            lhs: copy(l(f.param(0)).field(a)),
            rhs: copy(l(f.param(0)).field(a)),
        },
    );
    f.terminate(b0, Terminator::Return { value: copy(l(s)) });
    m.function(f);
    assert_eq!(violation(&verdict(m.finish())), "Accepted");
}

#[test]
fn copying_an_owning_value_is_rejected_by_its_class() {
    let mut p = pkgs(false);
    let mut f = FnBuilder::new("f", &[p.pkg_ty], p.pkg_ty);
    let b0 = f.block();
    f.terminate(
        b0,
        Terminator::Return {
            value: copy(l(f.param(0))),
        },
    );
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "OperandClass");
}

// ---- variants -------------------------------------------------------------

struct Opt {
    m: ModuleBuilder,
    opt: AdtId,
    opt_ty: TyId,
}

/// `enum Opt { None, Some(i32) }`
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

#[test]
fn payload_read_without_a_tag_test_is_rejected() {
    let mut o = opt();
    let some = o.m.field(o.opt, 1, 0);
    let i32 = o.m.t.i32;
    let mut f = FnBuilder::new("f", &[o.opt_ty], i32);
    let b0 = f.block();
    f.terminate(
        b0,
        Terminator::Return {
            value: copy(l(f.param(0)).field(some)),
        },
    );
    o.m.function(f);
    assert_eq!(violation(&verdict(o.m.finish())), "UnknownVariant");
}

fn match_opt(o: &mut Opt, read_in_some: u32, otherwise_unreachable: bool) -> Module {
    let i32 = o.m.t.i32;
    let field = o.m.field(o.opt, read_in_some, 0);
    let mut f = FnBuilder::new("f", &[o.opt_ty], i32);
    let (b0, none, some, other) = (f.block(), f.block(), f.block(), f.block());
    let d = f.local(i32);
    f.assign(b0, l(d), Rvalue::Discriminant(l(f.param(0))));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(d)),
            arms: vec![(0, none), (1, some)],
            otherwise: other,
        },
    );
    f.terminate(none, Terminator::Return { value: int(0) });
    f.terminate(
        some,
        Terminator::Return {
            value: copy(l(f.param(0)).field(field)),
        },
    );
    f.terminate(
        other,
        if otherwise_unreachable {
            Terminator::Unreachable
        } else {
            Terminator::Return { value: int(9) }
        },
    );
    o.m.function(f);
    std::mem::take(&mut o.m).finish()
}

#[test]
fn payload_read_in_the_matching_arm_is_accepted() {
    let mut o = opt();
    assert_eq!(violation(&verdict(match_opt(&mut o, 1, false))), "Accepted");
}

#[test]
fn payload_read_of_another_variant_in_an_arm_is_rejected() {
    // In the `Some` arm, a `None`-variant field does not exist; use a
    // two-payload enum to make the wrong-variant field well-typed.
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let (e, e_ty) = m.adt("m.E", AdtKind::Enum, false, &[("A", &[i32]), ("B", &[i32])]);
    let a = m.field(e, 0, 0);
    let mut f = FnBuilder::new("f", &[e_ty], i32);
    let (b0, arm_a, arm_b) = (f.block(), f.block(), f.block());
    let d = f.local(i32);
    f.assign(b0, l(d), Rvalue::Discriminant(l(f.param(0))));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(d)),
            arms: vec![(0, arm_a)],
            otherwise: arm_b,
        },
    );
    f.terminate(
        arm_a,
        Terminator::Return {
            value: copy(l(f.param(0)).field(a)),
        },
    );
    f.terminate(
        arm_b,
        Terminator::Return {
            value: copy(l(f.param(0)).field(a)),
        },
    );
    m.function(f);
    assert_eq!(violation(&verdict(m.finish())), "UnknownVariant");
}

/// `match v { 0 => .., 1 => .., otherwise => read field of `read` }` on a
/// three-variant enum, or with only arm 0 when `two_arms` is false.
fn three_way(two_arms: bool, read: u32) -> Module {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let (e, e_ty) = m.adt(
        "m.C",
        AdtKind::Enum,
        false,
        &[("A", &[i32]), ("B", &[i32]), ("C", &[i32])],
    );
    let field = m.field(e, read, 0);
    let mut f = FnBuilder::new("f", &[e_ty], i32);
    let (b0, arm, other) = (f.block(), f.block(), f.block());
    let d = f.local(i32);
    f.assign(b0, l(d), Rvalue::Discriminant(l(f.param(0))));
    let arms = if two_arms {
        vec![(0, arm), (1, arm)]
    } else {
        vec![(0, arm)]
    };
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(d)),
            arms,
            otherwise: other,
        },
    );
    f.terminate(arm, Terminator::Return { value: int(0) });
    f.terminate(
        other,
        Terminator::Return {
            value: copy(l(f.param(0)).field(field)),
        },
    );
    m.function(f);
    m.finish()
}

#[test]
fn otherwise_with_one_remaining_variant_establishes_it() {
    assert_eq!(violation(&verdict(three_way(true, 2))), "Accepted");
}

#[test]
fn otherwise_never_establishes_a_covered_variant() {
    assert_eq!(violation(&verdict(three_way(true, 1))), "UnknownVariant");
}

#[test]
fn otherwise_with_two_remaining_variants_establishes_neither() {
    assert_eq!(violation(&verdict(three_way(false, 2))), "UnknownVariant");
}

#[test]
fn unreachable_after_an_exhaustive_tag_test_is_proved() {
    // `otherwise` is infeasible only once the tag is known; build a value
    // with a known variant so the semantics proves the arm dead.
    let mut o = opt();
    let i32 = o.m.t.i32;
    let some = o.m.field(o.opt, 1, 0);
    let mut f = FnBuilder::new("f", &[i32], i32);
    let (b0, arm_some, other) = (f.block(), f.block(), f.block());
    let x = f.local(o.opt_ty);
    let d = f.local(i32);
    f.assign(
        b0,
        l(x),
        Rvalue::Aggregate {
            kind: AggregateKind::Adt {
                adt: o.opt,
                variant: VariantIdx(1),
                args: o.m.t.empty,
            },
            operands: vec![copy(l(f.param(0)))],
        },
    );
    f.assign(b0, l(d), Rvalue::Discriminant(l(x)));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(d)),
            arms: vec![(1, arm_some)],
            otherwise: other,
        },
    );
    f.terminate(
        arm_some,
        Terminator::Return {
            value: copy(l(x).field(some)),
        },
    );
    f.terminate(other, Terminator::Unreachable);
    o.m.function(f);
    assert_eq!(violation(&verdict(o.m.finish())), "Accepted");
}

#[test]
fn otherwise_of_an_exhaustive_tag_switch_is_proved_dead() {
    // Every variant has an arm, so no tag remains for `otherwise`.
    let mut o = opt();
    assert_eq!(violation(&verdict(match_opt(&mut o, 1, true))), "Accepted");
}

#[test]
fn reachable_unreachable_is_rejected() {
    // An integer switch leaves every other value to `otherwise`.
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let mut f = FnBuilder::new("f", &[i32], i32);
    let (b0, arm, other) = (f.block(), f.block(), f.block());
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(f.param(0))),
            arms: vec![(0, arm)],
            otherwise: other,
        },
    );
    f.terminate(arm, Terminator::Return { value: int(0) });
    f.terminate(other, Terminator::Unreachable);
    m.function(f);
    assert_eq!(violation(&verdict(m.finish())), "ReachableUnreachable");
}

#[test]
fn rewriting_the_scrutinee_invalidates_the_tag_fact() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let (e, e_ty) = m.adt("m.E", AdtKind::Enum, false, &[("A", &[i32]), ("B", &[i32])]);
    let a = m.field(e, 0, 0);
    let mut f = FnBuilder::new("f", &[e_ty, e_ty], i32);
    let (b0, arm_a, arm_b) = (f.block(), f.block(), f.block());
    let d = f.local(i32);
    f.assign(b0, l(d), Rvalue::Discriminant(l(f.param(0))));
    // The scrutinee changes after its tag was read.
    f.assign(b0, l(f.param(0)), Rvalue::Use(copy(l(f.param(1)))));
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(d)),
            arms: vec![(0, arm_a)],
            otherwise: arm_b,
        },
    );
    f.terminate(
        arm_a,
        Terminator::Return {
            value: copy(l(f.param(0)).field(a)),
        },
    );
    f.terminate(arm_b, Terminator::Return { value: int(0) });
    m.function(f);
    assert_eq!(violation(&verdict(m.finish())), "UnknownVariant");
}

// ---- indirections and captures -------------------------------------------

#[test]
fn writing_through_a_shared_address_is_rejected() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let r = m.ty(TyKind::Ref {
        region: Region::Param(0),
        mutability: Mutability::Shared,
        pointee: i32,
    });
    let mut f = FnBuilder::new("f", &[r], m.t.unit);
    let b0 = f.block();
    f.assign(b0, l(f.param(0)).deref(), Rvalue::Use(int(1)));
    f.terminate(b0, ret_unit());
    m.function(f);
    assert_eq!(violation(&verdict(m.finish())), "WriteThroughShared");
}

#[test]
fn writing_a_copy_value_through_an_exclusive_address_is_accepted() {
    let mut m = ModuleBuilder::new();
    let i32 = m.t.i32;
    let r = m.ty(TyKind::Ref {
        region: Region::Param(0),
        mutability: Mutability::Exclusive,
        pointee: i32,
    });
    let mut f = FnBuilder::new("f", &[r], m.t.unit);
    let b0 = f.block();
    f.assign(b0, l(f.param(0)).deref(), Rvalue::Use(int(1)));
    f.terminate(b0, ret_unit());
    m.function(f);
    assert_eq!(violation(&verdict(m.finish())), "Accepted");
}

#[test]
fn moving_out_through_an_exclusive_address_is_rejected() {
    let mut p = pkgs(false);
    let r = p.m.ty(TyKind::Ref {
        region: Region::Param(0),
        mutability: Mutability::Exclusive,
        pointee: p.pkg_ty,
    });
    let mut f = FnBuilder::new("f", &[r], p.m.t.unit);
    let (b0, b1) = (f.block(), f.block());
    let u0 = f.local(p.m.t.unit);
    f.terminate(
        b0,
        call(p.consume, vec![mv(l(f.param(0)).deref())], l(u0), b1),
    );
    f.terminate(b1, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "MoveFromBorrowed");
}

#[test]
fn initializing_an_owning_value_behind_an_address_is_rejected() {
    let mut p = pkgs(false);
    let r = p.m.ty(TyKind::Ref {
        region: Region::Param(0),
        mutability: Mutability::Exclusive,
        pointee: p.pkg_ty,
    });
    let mut f = FnBuilder::new("f", &[r, p.pkg_ty], p.m.t.unit);
    let b0 = f.block();
    f.assign(b0, l(f.param(0)).deref(), Rvalue::Use(mv(l(f.param(1)))));
    f.terminate(b0, ret_unit());
    p.m.function(f);
    assert_eq!(
        violation(&verdict(p.m.finish())),
        "OverwriteThroughIndirection"
    );
}

#[test]
fn a_cancelled_call_does_not_initialize_its_destination() {
    let mut p = pkgs(false);
    let pkg_ty = p.pkg_ty;
    // fn id(give o: Owner) -> Owner { return o }
    let mut id = FnBuilder::new("id", &[pkg_ty], pkg_ty);
    let ib = id.block();
    id.terminate(
        ib,
        Terminator::Return {
            value: mv(l(id.param(0))),
        },
    );
    let id = p.m.function(id);
    let mut f = FnBuilder::new("f", &[pkg_ty], p.m.t.unit);
    let (b0, ok, cancelled, done) = (f.block(), f.block(), f.block(), f.block());
    let x = f.local(pkg_ty);
    let u0 = f.local(p.m.t.unit);
    let u1 = f.local(p.m.t.unit);
    f.terminate(
        b0,
        Terminator::Call {
            callee: Callee::Direct(id),
            args: vec![mv(l(f.param(0)))],
            dest: l(x),
            target: ok,
            cancel: Some(cancelled),
        },
    );
    f.terminate(ok, call(p.consume, vec![mv(l(x))], l(u0), done));
    // On cancellation the destination was never written.
    f.terminate(cancelled, call(p.consume, vec![mv(l(x))], l(u1), done));
    f.terminate(done, ret_unit());
    p.m.function(f);
    assert_eq!(violation(&verdict(p.m.finish())), "UseOfUninit");
}

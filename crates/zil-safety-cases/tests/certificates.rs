//! Untrusted-certificate mutations. Each forged certificate either claims
//! more than the program guarantees or breaks the certificate shape; the
//! checker must reject every one. The honest certificate is accepted.

use zil_ir::body::*;
use zil_safety_cases::*;

/// A diamond that moves an owning parameter on one side only, joins, then
/// settles it under a test of the same flag. The join block needs an
/// invariant in which the parameter is `Maybe`.
fn diamond() -> (Module, Local) {
    let mut m = ModuleBuilder::new();
    let (_, _, managed) = m.resource("m.File");
    let bool_ty = m.t.bool;
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
    let mut f = FnBuilder::new("f", &[managed, bool_ty], m.t.unit);
    let (b0, give, keep, join, settle, done) = (
        f.block(),
        f.block(),
        f.block(),
        f.block(),
        f.block(),
        f.block(),
    );
    let u = f.local(m.t.unit);
    let flag = f.param(1);
    let x = f.param(0);
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(flag)),
            arms: vec![(1, give)],
            otherwise: keep,
        },
    );
    f.terminate(
        give,
        Terminator::Call {
            callee: Callee::Direct(sink),
            args: vec![mv(l(x))],
            dest: l(u),
            target: join,
            cancel: None,
        },
    );
    f.push(keep, InstructionKind::Drop { place: l(x) });
    f.terminate(keep, goto(join));
    f.terminate(join, goto(settle));
    f.terminate(settle, goto(done));
    f.terminate(done, ret_unit());
    m.function(f);
    (m.finish(), x)
}

fn join_invariant(cert: &mut Certificate) -> &mut BlockInvariant {
    cert.functions[1]
        .invariants
        .iter_mut()
        .find(|i| i.block == BlockId(3))
        .expect("join is certified")
}

#[test]
fn the_honest_certificate_is_accepted() {
    let (module, x) = diamond();
    let cert = solve(&module);
    let inv = cert.functions[1]
        .invariants
        .iter()
        .find(|i| i.block == BlockId(3))
        .unwrap();
    // The parameter is uninitialized on both sides of the join.
    assert!(
        inv.init
            .iter()
            .any(|c| c.place == l(x) && c.state == InitClaim::Uninit)
    );
    check_with(module, &cert).unwrap();
}

#[test]
fn a_missing_join_invariant_is_rejected() {
    let (module, _) = diamond();
    let mut cert = solve(&module);
    cert.functions[1]
        .invariants
        .retain(|i| i.block != BlockId(3));
    let e = check_with(module, &cert).unwrap_err();
    assert_eq!(e.kind, CheckErrorKind::MissingInvariant(BlockId(3)));
}

#[test]
fn claiming_a_moved_value_initialized_is_rejected() {
    let (module, x) = diamond();
    let mut cert = solve(&module);
    for c in &mut join_invariant(&mut cert).init {
        if c.place == l(x) {
            c.state = InitClaim::Init;
        }
    }
    // Rejected either at the edge into the join or by the obligation the
    // forged claim creates downstream; never accepted.
    let e = check_with(module, &cert).unwrap_err();
    assert!(
        matches!(
            e.kind,
            CheckErrorKind::EdgeNotCovered { to: BlockId(3), .. }
        ) || format!("{e:?}").contains("LeakedObligation"),
        "{e:?}"
    );
}

#[test]
fn a_forged_variant_fact_is_rejected() {
    let (module, _) = diamond();
    let mut cert = solve(&module);
    join_invariant(&mut cert).variants.push(VariantClaim {
        place: l(Local(1)),
        variant: 1,
    });
    let e = check_with(module, &cert).unwrap_err();
    assert!(
        matches!(e.kind, CheckErrorKind::EdgeNotCovered { .. }),
        "{e:?}"
    );
}

#[test]
fn a_claim_about_an_untracked_place_is_rejected() {
    let (module, _) = diamond();
    let mut cert = solve(&module);
    join_invariant(&mut cert).init.push(PlaceInit {
        place: l(Local(99)),
        state: InitClaim::Init,
    });
    let e = check_with(module, &cert).unwrap_err();
    assert_eq!(e.kind, CheckErrorKind::UnknownPlace);
}

#[test]
fn duplicate_invariants_are_rejected() {
    let (module, _) = diamond();
    let mut cert = solve(&module);
    let dup = join_invariant(&mut cert).clone();
    cert.functions[1].invariants.push(dup);
    let e = check_with(module, &cert).unwrap_err();
    assert_eq!(e.kind, CheckErrorKind::DuplicateInvariant(BlockId(3)));
}

#[test]
fn a_truncated_certificate_is_rejected() {
    let (module, _) = diamond();
    let mut cert = solve(&module);
    cert.functions.pop();
    assert!(matches!(
        check_with(module, &cert).unwrap_err().kind,
        CheckErrorKind::CertificateShape(_)
    ));
}

#[test]
fn loan_claims_are_not_accepted_before_their_family_exists() {
    let (module, x) = diamond();
    let mut cert = solve(&module);
    join_invariant(&mut cert).loans.push(LoanClaim {
        place: l(x),
        mutability: Mutability::Shared,
        region: Region::Static,
    });
    assert!(matches!(
        check_with(module, &cert).unwrap_err().kind,
        CheckErrorKind::CertificateShape(_)
    ));
}

#[test]
fn weakening_a_claim_is_sound_and_cannot_hide_an_obligation() {
    // Weakening the flag (a Copy value never read after the join) to Maybe
    // still checks: a weaker invariant is a valid inductive bound.
    let (module, _) = diamond();
    let flag = l(Local(1));
    let mut cert = solve(&module);
    join_invariant(&mut cert).init.retain(|c| c.place != flag);
    check_with(module, &cert).unwrap();
    // Weakening the owning parameter makes it possibly live at the exit,
    // which the checker reports instead of trusting the earlier move.
    let (module, x) = diamond();
    let mut cert = solve(&module);
    join_invariant(&mut cert).init.retain(|c| c.place != l(x));
    let e = check_with(module, &cert).unwrap_err();
    assert!(format!("{e:?}").contains("LeakedObligation"), "{e:?}");
}

#[test]
fn a_forged_entry_invariant_must_cover_the_initial_state() {
    // A loop back to the entry forces an entry invariant; claiming the
    // parameter uninitialized there contradicts the initial state.
    let mut m = ModuleBuilder::new();
    let bool_ty = m.t.bool;
    let mut f = FnBuilder::new("f", &[bool_ty], m.t.unit);
    let (b0, done) = (f.block(), f.block());
    f.terminate(
        b0,
        Terminator::Switch {
            discr: copy(l(f.param(0))),
            arms: vec![(1, b0)],
            otherwise: done,
        },
    );
    f.terminate(done, ret_unit());
    m.function(f);
    let module = m.finish();
    let mut cert = solve(&module);
    check_with(module.clone(), &cert).unwrap();
    let entry = cert.functions[0]
        .invariants
        .iter_mut()
        .find(|i| i.block == BlockId(0))
        .unwrap();
    entry.init.retain(|c| c.place != l(Local(0)));
    entry.init.push(PlaceInit {
        place: l(Local(0)),
        state: InitClaim::Uninit,
    });
    let e = check_with(module, &cert).unwrap_err();
    assert_eq!(e.kind, CheckErrorKind::EntryNotCovered);
}

#[test]
fn checking_cost_is_bounded_by_the_budget() {
    let (module, _) = diamond();
    let cert = solve(&module);
    let e = zil_check::check_module(module, &cert, CheckBudget { steps: 3 }).unwrap_err();
    assert_eq!(e.kind, CheckErrorKind::ResourceLimit);
}

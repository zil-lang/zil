//! Body IR and Certificate Wire roundtrip, adversarial SafeReject, and
//! SourceMap fidelity checks.

use zil_base::NodeId;
use zil_check::{CheckBudget, check_fidelity, check_module};
use zil_ir::body::{
    BlockId, FieldIdx, FnId, Local, OpLocation, SourceRelation, WireError, decode_certificate,
    decode_module, encode_certificate, encode_module,
};
use zil_solve::{Budget, solve_module};

#[test]
fn wire_module_and_certificate_roundtrip_and_reverify() {
    let src = r#"
struct Pair {
    left: i32;
    right: i32;
}

fn pick<life a>(cond: bool, x: &'a i32, y: &'a i32) -> &'a i32 {
    if cond {
        return &*x
    }
    return &*y
}

fn main() -> i32 {
    let p = Pair { left: 10, right: 32 }
    let r = pick(true, &p.left, &p.right)
    return *r + p.right
}
"#;
    let lowered = zil_typeck::lower_source("wire_roundtrip.zil", src).expect("lowers");
    let cert = solve_module(&lowered.module, Budget { steps: 100_000 }).expect("solves");

    let mod_bytes = encode_module(&lowered.module);
    let cert_bytes = encode_certificate(&cert);

    let decoded_mod = decode_module(&mod_bytes).expect("module decodes");
    let decoded_cert = decode_certificate(&cert_bytes).expect("certificate decodes");

    // Canonical deterministic re-encoding.
    assert_eq!(encode_module(&decoded_mod), mod_bytes);
    assert_eq!(encode_certificate(&decoded_cert), cert_bytes);

    // Checker admits the decoded module + certificate identically.
    check_module(decoded_mod, &decoded_cert, CheckBudget { steps: 100_000 })
        .expect("decoded module and certificate pass checker");
}

#[test]
fn wire_adversarial_inputs_safe_reject() {
    let src = "fn main() -> i32 { return 7 }";
    let lowered = zil_typeck::lower_source("wire_adv.zil", src).expect("lowers");
    let cert = solve_module(&lowered.module, Budget { steps: 100_000 }).expect("solves");
    let mod_bytes = encode_module(&lowered.module);
    let cert_bytes = encode_certificate(&cert);

    // Empty and bad magic / bad version.
    assert_eq!(decode_module(&[]).unwrap_err(), WireError::UnexpectedEof);
    assert_eq!(
        decode_module(b"NOPE\x01\x00\x00\x00").unwrap_err(),
        WireError::BadMagic
    );
    assert_eq!(
        decode_module(b"ZIRB\x99\x00\x00\x00").unwrap_err(),
        WireError::UnsupportedVersion(0x99)
    );
    assert_eq!(
        decode_certificate(b"ZIRB\x01\x00\x00\x00").unwrap_err(),
        WireError::BadMagic
    );

    // Every strict prefix truncation must fail cleanly without panicking.
    for cut in 0..mod_bytes.len() {
        assert!(
            decode_module(&mod_bytes[..cut]).is_err(),
            "truncated module at {cut} must fail"
        );
    }
    for cut in 0..cert_bytes.len() {
        assert!(
            decode_certificate(&cert_bytes[..cut]).is_err(),
            "truncated certificate at {cut} must fail"
        );
    }

    // Trailing garbage bytes must be rejected.
    let mut with_tail = mod_bytes.clone();
    with_tail.push(0xAA);
    assert_eq!(
        decode_module(&with_tail).unwrap_err(),
        WireError::TrailingBytes(1)
    );
    let mut cert_tail = cert_bytes.clone();
    cert_tail.extend_from_slice(&[1, 2, 3]);
    assert_eq!(
        decode_certificate(&cert_tail).unwrap_err(),
        WireError::TrailingBytes(3)
    );

    // Adversarial huge sequence length must fail with SequenceTooLong, never OOM.
    let mut huge_len = b"ZIRB".to_vec();
    huge_len.extend_from_slice(&1u32.to_le_bytes());
    huge_len.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode_module(&huge_len),
        Err(WireError::SequenceTooLong(_))
    ));
}

#[test]
fn fidelity_accepts_honest_source_map_and_rejects_tampering() {
    let src = r#"
struct Point {
    x: i32;
    y: i32;
}

fn sum(p: Point) -> i32 {
    return p.x + p.y
}
"#;
    let lowered = zil_typeck::lower_source("fidelity.zil", src).expect("lowers");
    assert!(
        !lowered.source_map.functions.is_empty(),
        "typeck must populate SourceMap"
    );
    check_fidelity(&lowered.module, &lowered.source_map, &[]).expect("honest source map passes");

    // Tamper 1: out-of-range binding local.
    let mut bad_binding = lowered.source_map.clone();
    bad_binding.functions[0]
        .relations
        .push(SourceRelation::Binding {
            node: NodeId(999),
            local: Local(9999),
        });
    assert!(check_fidelity(&lowered.module, &bad_binding, &[]).is_err());

    // Tamper 2: bogus FieldSelect ordinal.
    let mut bad_field = lowered.source_map.clone();
    let mut mutated = false;
    for rel in &mut bad_field.functions[0].relations {
        if let SourceRelation::FieldSelect { field, .. } = rel {
            field.field = FieldIdx(99);
            mutated = true;
            break;
        }
    }
    assert!(mutated, "expected at least one FieldSelect relation");
    assert!(check_fidelity(&lowered.module, &bad_field, &[]).is_err());

    // Tamper 3: bogus ResourceOp pointing to a non-resource instruction.
    let mut bad_res = lowered.source_map.clone();
    bad_res.functions[0]
        .relations
        .push(SourceRelation::ResourceOp {
            node: NodeId(1),
            at: OpLocation {
                block: BlockId(0),
                instruction: Some(0),
            },
        });
    assert!(check_fidelity(&lowered.module, &bad_res, &[]).is_err());

    // Tamper 4: unknown function id.
    let mut bad_fn = lowered.source_map.clone();
    bad_fn.functions[0].function = FnId(777);
    assert!(check_fidelity(&lowered.module, &bad_fn, &[]).is_err());
}

#[test]
fn forged_dyn_impl_in_wire_is_rejected_by_checker() {
    let src = include_str!(
        "../../../tests/language-readiness/dynamic-dispatch/dyn-owned-user-drop-store.zil"
    );
    let lowered =
        zil_typeck::lower_source("dyn-owned-user-drop-store.zil", src).expect("dyn fixture lowers");
    assert!(
        lowered.module.dyn_impls.len() >= 2,
        "expected at least 2 concrete DynImpls"
    );
    let cert =
        solve_module(&lowered.module, zil_solve::Budget { steps: 1_000_000 }).expect("solves");

    // Honest wire roundtrip must be admitted.
    let wire_bytes = encode_module(&lowered.module);
    let decoded = decode_module(&wire_bytes).expect("decode honest module");
    check_module(decoded.clone(), &cert, CheckBudget { steps: 1_000_000 })
        .expect("honest decoded module must be admitted");

    // Tamper 1: truncated method list.
    let mut t1 = decoded.clone();
    t1.dyn_impls[0].methods.clear();
    let d1 = decode_module(&encode_module(&t1)).expect("decode t1");
    assert!(
        check_module(d1, &cert, CheckBudget { steps: 1_000_000 }).is_err(),
        "truncated dyn_impl methods must be rejected"
    );

    // Tamper 2: wrong method FnId (mismatched signature / Drop entry).
    let mut t2 = decoded.clone();
    let other_fn = (0..t2.functions.len())
        .map(FnId::from_index)
        .find(|&fid| fid != t2.dyn_impls[0].methods[0])
        .expect("another function");
    t2.dyn_impls[0].methods[0] = other_fn;
    let d2 = decode_module(&encode_module(&t2)).expect("decode t2");
    assert!(
        check_module(d2, &cert, CheckBudget { steps: 1_000_000 }).is_err(),
        "mismatched dyn_impl method signature must be rejected"
    );

    // Tamper 3: out-of-bounds DynId.
    let mut t3 = decoded.clone();
    t3.dyn_impls[0].dyn_id = zil_ir::body::DynId(999);
    let d3 = decode_module(&encode_module(&t3)).expect("decode t3");
    assert!(
        check_module(d3, &cert, CheckBudget { steps: 1_000_000 }).is_err(),
        "unknown DynId must be rejected"
    );

    // Tamper 4: duplicate (DynId, concrete_ty).
    let mut t4 = decoded.clone();
    let dup = t4.dyn_impls[0].clone();
    t4.dyn_impls.push(dup);
    let d4 = decode_module(&encode_module(&t4)).expect("decode t4");
    assert!(
        check_module(d4, &cert, CheckBudget { steps: 1_000_000 }).is_err(),
        "duplicate dyn_impl must be rejected"
    );
}

#[test]
fn same_signature_dyn_impl_substitution_attack_is_rejected() {
    use zil_ir::body::{Callee, FnRole, IntrinsicOp, Terminator};

    // `spoof` has the exact same `(self: &A) -> i32` signature as `impl Read for A::read`,
    // and is referenced from `main` so it is lowered into the Body IR module.
    let src = r#"
trait Read {
    fn read(self: &Self) -> i32;
}

struct A { v: i32 }
struct B { v: i32 }

impl Read for A {
    fn read(self: &Self) -> i32 { return (*self).v }
}

impl Read for B {
    fn read(self: &Self) -> i32 { return (*self).v + 10 }
}

fn spoof(a: &A) -> i32 {
    return (*a).v + 999
}

fn call_read(x: OwnedDyn<dyn Read>) -> i32 {
    return x.read()
}

fn run() -> DynCreateError!i32 {
    let a0 = A { v: 1 }
    let _ = spoof(&a0)
    let ra = owned_dyn::<dyn Read>(A { v: 1 })?
    let rb = owned_dyn::<dyn Read>(B { v: 2 })?
    return call_read(ra) + call_read(rb) - 13
}

fn main() -> i32 {
    match run() {
        .ok(v) => { return v },
        .err(_) => { return 99 }
    }
}
"#;
    let lowered = zil_typeck::lower_source("dyn_spoof_attack.zil", src).expect("lowers");
    let cert =
        solve_module(&lowered.module, zil_solve::Budget { steps: 1_000_000 }).expect("solves");
    let decoded = decode_module(&encode_module(&lowered.module)).expect("decode honest");
    check_module(decoded.clone(), &cert, CheckBudget { steps: 1_000_000 })
        .expect("honest module passes");

    let spoof_id = decoded
        .functions
        .iter()
        .enumerate()
        .find_map(|(i, f)| {
            (f.name == "spoof" && f.sig.role == FnRole::Normal).then_some(FnId::from_index(i))
        })
        .expect("spoof Normal function exists");
    let a_impl_idx = decoded
        .dyn_impls
        .iter()
        .position(|di| {
            let fid = di.methods[0];
            let f = &decoded.functions[fid.index()];
            f.sig.params == decoded.functions[spoof_id.index()].sig.params
                && f.sig.result == decoded.functions[spoof_id.index()].sig.result
        })
        .expect("find A DynImpl matching spoof signature");
    let b_impl_idx = decoded
        .dyn_impls
        .iter()
        .enumerate()
        .find_map(|(i, _)| (i != a_impl_idx).then_some(i))
        .expect("find B DynImpl");

    // Attack 1: replace A's DynImpl method with same-signature Normal `spoof`.
    // Must be rejected because `spoof` lacks `FnRole::DynSlotImpl` authorization.
    let mut attack1 = decoded.clone();
    attack1.dyn_impls[a_impl_idx].methods[0] = spoof_id;
    let d1 = decode_module(&encode_module(&attack1)).expect("decode attack1");
    assert!(
        check_module(d1, &cert, CheckBudget { steps: 1_000_000 }).is_err(),
        "same-signature Normal function substitution into DynImpl must be rejected"
    );

    // Attack 2: substitute B's authorized DynSlotImpl into A's DynImpl.
    let mut attack2 = decoded.clone();
    attack2.dyn_impls[a_impl_idx].methods[0] = attack2.dyn_impls[b_impl_idx].methods[0];
    let d2 = decode_module(&encode_module(&attack2)).expect("decode attack2");
    assert!(
        check_module(d2, &cert, CheckBudget { steps: 1_000_000 }).is_err(),
        "cross-concrete-type DynSlotImpl substitution must be rejected"
    );

    // Attack 3: inject a TaskYield suspension effect into an authorized DynSlotImpl body.
    let mut attack3 = decoded.clone();
    let a_method_id = attack3.dyn_impls[a_impl_idx].methods[0];
    let unit_ty = attack3.tys.intern(zil_ir::body::TyKind::Unit);
    let a_body = &mut attack3.functions[a_method_id.index()].body;
    let tmp_local = Local::from_index(a_body.locals.len());
    a_body.locals.push(zil_ir::body::LocalDecl {
        ty: unit_ty,
        name: None,
    });
    let next_block = BlockId::from_index(a_body.blocks.len());
    let old_span = a_body.blocks[0].terminator_span;
    let old_entry_term = std::mem::replace(
        &mut a_body.blocks[0].terminator,
        Terminator::Call {
            callee: Callee::Intrinsic(IntrinsicOp::TaskYield),
            args: Vec::new(),
            dest: zil_ir::body::Place::local(tmp_local),
            target: next_block,
            cancel: None,
        },
    );
    a_body.blocks.push(zil_ir::body::Block {
        instructions: Vec::new(),
        terminator: old_entry_term,
        terminator_span: old_span,
    });
    let d3 = decode_module(&encode_module(&attack3)).expect("decode attack3");
    assert!(
        check_module(d3, &cert, CheckBudget { steps: 1_000_000 }).is_err(),
        "TaskYield effect inside synchronous DynSlotImpl must be rejected"
    );

    // Attack 4: transitive Suspend — inject TaskYield into `spoof`, and make A::read
    // call `spoof` via `Callee::Direct` (A::read itself has no direct TaskYield).
    let mut attack4 = decoded.clone();
    let unit_ty4 = attack4.tys.intern(zil_ir::body::TyKind::Unit);
    let spoof_body = &mut attack4.functions[spoof_id.index()].body;
    let s_tmp = Local::from_index(spoof_body.locals.len());
    spoof_body.locals.push(zil_ir::body::LocalDecl {
        ty: unit_ty4,
        name: None,
    });
    let s_next = BlockId::from_index(spoof_body.blocks.len());
    let s_span = spoof_body.blocks[0].terminator_span;
    let s_old = std::mem::replace(
        &mut spoof_body.blocks[0].terminator,
        Terminator::Call {
            callee: Callee::Intrinsic(IntrinsicOp::TaskYield),
            args: Vec::new(),
            dest: zil_ir::body::Place::local(s_tmp),
            target: s_next,
            cancel: None,
        },
    );
    spoof_body.blocks.push(zil_ir::body::Block {
        instructions: Vec::new(),
        terminator: s_old,
        terminator_span: s_span,
    });
    let a_method_id4 = attack4.dyn_impls[a_impl_idx].methods[0];
    let i32_ty = attack4.functions[a_method_id4.index()].sig.result;
    let a_body4 = &mut attack4.functions[a_method_id4.index()].body;
    let call_dest = Local::from_index(a_body4.locals.len());
    a_body4.locals.push(zil_ir::body::LocalDecl {
        ty: i32_ty,
        name: None,
    });
    let a_next4 = BlockId::from_index(a_body4.blocks.len());
    let a_span4 = a_body4.blocks[0].terminator_span;
    let a_old4 = std::mem::replace(
        &mut a_body4.blocks[0].terminator,
        Terminator::Call {
            callee: Callee::Direct(spoof_id),
            args: vec![zil_ir::body::Operand::Copy(zil_ir::body::Place::local(
                Local(0),
            ))],
            dest: zil_ir::body::Place::local(call_dest),
            target: a_next4,
            cancel: None,
        },
    );
    a_body4.blocks.push(zil_ir::body::Block {
        instructions: Vec::new(),
        terminator: a_old4,
        terminator_span: a_span4,
    });
    let d4 = decode_module(&encode_module(&attack4)).expect("decode attack4");
    assert!(
        check_module(d4, &cert, CheckBudget { steps: 1_000_000 }).is_err(),
        "transitive DynSlotImpl -> helper -> TaskYield must be rejected"
    );
}

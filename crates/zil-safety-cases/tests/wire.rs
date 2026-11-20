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

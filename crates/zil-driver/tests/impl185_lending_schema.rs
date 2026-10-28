//! Schema23 carries a raw lending claim, without granting execution authority.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, TypedModule};
fn module() -> (TypedModule, CandidatePlan) {
    let s = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "lending-schema185.zil",
        "fn make(n:i32)->impl Fn()->i32{return move()=>n}fn main()->i32{let f=make(21);return f()-21}",
    );
    let ast = zil_syntax::parse(&s).into_result().unwrap();
    let core = zil_expand::expand(&s, &ast).unwrap();
    let m = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    (m, p)
}
#[test]
fn both_lending_fields_survive_wire_and_change_fingerprints() {
    let (mut m, mut p) = module();
    let before = m.fingerprint();
    let factory = m
        .functions
        .iter()
        .position(|f| f.callable_return_contract.is_some())
        .unwrap();
    let old = zil_ir::callable_return_contract_fingerprint(
        m.functions[factory]
            .callable_return_contract
            .as_ref()
            .unwrap(),
    );
    m.closures[0].lending_captures = vec![0];
    assert_ne!(before, m.fingerprint());
    let after_definition = m.fingerprint();
    m.functions[factory]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .lending_captures = vec![0];
    assert_ne!(after_definition, m.fingerprint());
    assert_ne!(
        old,
        zil_ir::callable_return_contract_fingerprint(
            m.functions[factory]
                .callable_return_contract
                .as_ref()
                .unwrap()
        )
    );
    p.module_fingerprint = m.fingerprint();
    let b = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&b, &Limits::default()).unwrap();
    assert_eq!(decoded, m);
    assert_eq!(plan, p);
}
#[test]
fn transport_claim_does_not_grant_a_copy_result_lending_capability() {
    for contract_only in [false, true] {
        let (mut m, mut p) = module();
        if contract_only {
            m.functions
                .iter_mut()
                .find_map(|f| f.callable_return_contract.as_mut())
                .unwrap()
                .lending_captures = vec![0];
        } else {
            m.closures[0].lending_captures = vec![0];
        }
        p.module_fingerprint = m.fingerprint();
        let b = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
        let target = m.target.clone();
        for r in [
            zil_verify::verify_module(m, p, &target, &Limits::default()),
            zil_verify::verify_module_bytes(&b, &target, &Limits::default()),
        ] {
            let d = r.unwrap_err();
            assert!(
                d.iter().any(|d| d.phase == Phase::Verify
                    && matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )
                    && d.message.contains("lending")),
                "{d:?}"
            );
        }
    }
}
#[test]
fn malformed_capture_indices_are_independently_rejected() {
    for indices in [vec![0, 0], vec![1], vec![u32::MAX]] {
        let (mut m, mut p) = module();
        m.closures[0].lending_captures = indices;
        p.module_fingerprint = m.fingerprint();
        let b = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
        let target = m.target.clone();
        for r in [
            zil_verify::verify_module(m, p, &target, &Limits::default()),
            zil_verify::verify_module_bytes(&b, &target, &Limits::default()),
        ] {
            let d = r.unwrap_err();
            assert!(
                d.iter().any(|d| d.phase == Phase::Verify
                    && d.category == ResultCategory::SafeReject
                    && d.message.contains("lending")),
                "{d:?}"
            );
        }
    }
}
#[test]
fn oversized_lending_claims_are_bounded_before_verifier_fingerprinting() {
    for contract_only in [false, true] {
        let (mut m, p) = module();
        let limits = Limits {
            max_locals: 16,
            ..Limits::default()
        };
        if contract_only {
            m.functions
                .iter_mut()
                .find_map(|f| f.callable_return_contract.as_mut())
                .unwrap()
                .lending_captures = vec![0; 17];
        } else {
            m.closures[0].lending_captures = vec![0; 17];
        }
        let target = m.target.clone();
        let d = zil_verify::verify_module(m.clone(), p.clone(), &target, &limits).unwrap_err();
        assert!(
            d.iter()
                .any(|d| d.category == ResultCategory::ResourceLimit),
            "{d:?}"
        );
        assert!(zil_ir::wire::encode_module_candidate(&m, &p, &limits).is_err());
    }
}

#[test]
fn opaque_public_json_retains_the_lending_relation() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "export185.zil",
        "pub fn make(n:i32)->impl Fn()->i32{return move()=>n}fn main(){}",
    );
    let report = zil_driver::compile_source(&source, &zil_driver::CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(
        report.to_json().contains("\"lending_captures\":[]"),
        "{}",
        report.to_json()
    );
}

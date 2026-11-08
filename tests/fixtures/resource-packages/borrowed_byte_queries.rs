//! Query reads use the actual weak source and its own delegation alternative.
use super::*;

fn report_for(label: &str, program: &str) -> zil_driver::CompileReport {
    compile_source(&source(label, program), &CompileOptions::default())
}
fn assert_success(label: &str, program: &str) {
    let report = report_for(label, program);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn assert_loan(label: &str, program: &str) {
    let report = report_for(label, program);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{label}: {}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("actual stored target/parent loan")
                || d.message.contains("live incompatible loan")),
        "{label}: {}",
        report.to_json()
    );
}

#[test]
fn weak_metadata_query_reaches_actual_source_and_parent_checks() {
    for (label, program) in [
        ("direct", include_str!("byte-query-controls/direct.zil")),
        ("identity", include_str!("byte-query-controls/identity.zil")),
        (
            "current-exclusive-parent",
            include_str!("byte-query-controls/exclusive-derived-query.zil"),
        ),
        (
            "both-derived-false",
            include_str!("byte-query-controls/both-derived-false-query.zil"),
        ),
        (
            "both-derived-true",
            include_str!("byte-query-controls/both-derived-true-query.zil"),
        ),
    ] {
        assert_success(label, program);
    }
    for (label, program) in [
        (
            "older-description-exclusive-parent",
            include_str!("byte-query-controls/exclusive-query.zil"),
        ),
        (
            "helper-read",
            include_str!("byte-query-controls/read-helper.zil"),
        ),
        (
            "helper-relay",
            include_str!("byte-query-controls/read-relay.zil"),
        ),
        (
            "mixed-false",
            include_str!("byte-query-controls/mixed-false-query.zil"),
        ),
        (
            "mixed-true",
            include_str!("byte-query-controls/mixed-true-query.zil"),
        ),
    ] {
        assert_loan(label, program);
    }
    let ignore = format!(
        "fn enable_query(b:ByteBuffer)->i64{{return bytes_len(b)}}{}",
        include_str!("byte-query-controls/ignore-helper-control.zil")
    );
    assert_success("ignore-is-not-a-read", &ignore);
}

fn activated_ignore(program: &str) -> String {
    format!(
        "fn enable_query(b:ByteBuffer)->i64{{return bytes_capacity(b)}}{}",
        program.replace(
            "fn inspect(buffer:ByteBuffer)->i64{return bytes_len(buffer)}",
            "fn inspect(buffer:ByteBuffer)->i64{let copy=buffer;return 0}"
        )
    )
}

#[test]
fn raw_query_rebuilds_helper_reads_and_delegation_without_candidate_authority() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (label, program, reject) in [
        (
            "old-parent",
            include_str!("byte-query-controls/read-helper.zil"),
            true,
        ),
        (
            "mixed-false",
            include_str!("byte-query-controls/mixed-false-query.zil"),
            true,
        ),
        (
            "mixed-true",
            include_str!("byte-query-controls/mixed-true-query.zil"),
            true,
        ),
        (
            "derived-false",
            include_str!("byte-query-controls/both-derived-false-query.zil"),
            false,
        ),
        (
            "derived-true",
            include_str!("byte-query-controls/both-derived-true-query.zil"),
            false,
        ),
    ] {
        let control = activated_ignore(program);
        assert_success(label, &control);
        let mut raw = typed_source(label, &control, &target, &limits);
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let function = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "inspect")
            .unwrap();
        let descriptor = function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::Load(local) if local == function.parameters[0] => i.result,
                _ => None,
            })
            .unwrap();
        assert!(
            matches!(&function.values[descriptor.0 as usize],Type::Desc(p) if p.as_ref()==&Type::ByteBuffer)
        );
        let returned = function
            .blocks
            .iter()
            .find_map(|b| match b.terminator {
                zil_ir::Terminator::Return(Some(v)) => Some(v),
                _ => None,
            })
            .unwrap();
        assert_eq!(function.values[returned.0 as usize], Type::I64);
        let instruction = function
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| i.result == Some(returned))
            .unwrap();
        instruction.kind = InstructionKind::ByteBufferLen { buffer: descriptor };
        // Retain every old claim and metadata row. Only the actual read and
        // its module fingerprint change; this is not a producer certificate.
        plan.module_fingerprint = zil_ir::module_fingerprint(&raw, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        if reject {
            let a = zil_analyze::analyze(&raw, &limits).unwrap_err();
            assert!(
                a.iter()
                    .any(|d| d.message.contains("actual stored target/parent loan")),
                "{label}: {a:?}"
            );
            let v =
                zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
            assert!(
                v.iter()
                    .any(|d| d.message.contains("live incompatible loan")),
                "{label}: {v:?}"
            );
            let w = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
            assert!(
                w.iter()
                    .any(|d| d.message.contains("live incompatible loan")),
                "{label}: {w:?}"
            );
        } else {
            zil_analyze::analyze(&raw, &limits).unwrap();
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        }
    }
}

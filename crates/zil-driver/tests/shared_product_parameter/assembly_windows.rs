//! Recent assembly/extraction/replacement boundaries; never use Analyze to
//! obtain the candidates used for independent Verify checks.
use super::*;

fn selected(case: &Case) -> bool {
    case.name.starts_with("assembled-")
        || case.name == "field-borrow-after-assemble"
        || case.name.starts_with("refill-")
        || case.name.starts_with("swap-")
}

#[test]
fn assembly_source_windows_have_exact_outcomes() {
    let mut failures = Vec::new();
    for case in cases().into_iter().filter(selected) {
        let source = SourceUnit::new(FileId(0), ModuleId(0), case.name, case.source);
        let report = compile_source(&source, &CompileOptions::default());
        let status = if case.success {
            ResultCategory::Success
        } else {
            ResultCategory::SafeReject
        };
        record(
            &case,
            "assembly-source",
            &format!(
                "{:?}/{:?} verification_runs={}",
                report.status, report.phase, report.verification_runs
            ),
        );
        if report.status != status
            || report.phase != case.phase
            || report.verification_runs != usize::from(case.phase == Phase::Verify)
            || (!case.success
                && !report
                    .diagnostics
                    .iter()
                    .any(|d| d.phase == case.phase && d.code == "E-BORROW-CONFLICT"))
        {
            failures.push(format!("{}: {}", case.name, report.to_json()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn assembly_raw_wire_windows_are_independent_of_analyze() {
    let mut failures = Vec::new();
    // Establish the lawful third-Store / assembled-before-borrow prefixes first.
    let mut cases: Vec<_> = cases().into_iter().filter(selected).collect();
    cases.sort_by_key(|case| !case.success);
    for case in cases {
        let source = SourceUnit::new(FileId(0), ModuleId(0), case.name, case.source);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &syntax).unwrap();
        let target = zil_base::TargetProfile::host_llvm();
        let limits = Limits::default();
        let module = zil_types::check(&core, &target, &limits).unwrap();
        let plan = raw_prefix::straight_line_claims(&module);
        let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        for (route, result) in [
            (
                "assembly-raw",
                zil_verify::verify_module(module, plan, &target, &limits),
            ),
            (
                "assembly-wire",
                zil_verify::verify_module_bytes(&wire, &target, &limits),
            ),
        ] {
            match result {
                Ok(_) => {
                    record(&case, route, "Success/Verify");
                    if !case.success {
                        failures.push(format!("{} {route}: invalid window accepted", case.name));
                    }
                }
                Err(errors) => {
                    record(
                        &case,
                        route,
                        &format!(
                            "{:?}",
                            errors
                                .iter()
                                .map(|d| (&d.phase, &d.category, &d.code))
                                .collect::<Vec<_>>()
                        ),
                    );
                    if case.success
                        || !errors.iter().any(|d| {
                            d.phase == Phase::Verify
                                && d.category == ResultCategory::SafeReject
                                && d.code == "E-BORROW-CONFLICT"
                        })
                    {
                        failures.push(format!("{} {route}: {errors:?}", case.name));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

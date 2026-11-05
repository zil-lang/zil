//! Source-driven owning replacement through independent nonlocal positions.
use super::*;
const CAPTURE: &str = include_str!("captured-whole.zil");
const MULTIPLE: &str = include_str!("multiple-inputs.zil");
const STORE: &str = include_str!("store-whole.zil");
fn typed(text: &str) -> zil_ir::TypedModule {
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
fn check(text: &str) {
    let module = typed(text);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap();
}
#[test]
fn g139_nonlocal_complete_package_source_raw_wire() {
    for text in [
        CAPTURE,
        MULTIPLE,
        STORE,
        include_str!("two-leaf-helper.zil"),
        include_str!("two-leaf-local.zil"),
        include_str!("store-abort.zil"),
        include_str!("pin-apart.zil"),
        include_str!("enum-helper.zil"),
        include_str!("store-helper.zil"),
        include_str!("enum-multiple-leaves.zil"),
        include_str!("nested-package-helper.zil"),
        include_str!("nested-package-local.zil"),
        include_str!("multiple-leaves-abort.zil"),
        include_str!("public-role-capture-probe.zil"),
        include_str!("public-role-repeat.zil"),
        include_str!("public-role-no-call.zil"),
        include_str!("public-role-loop.zil"),
    ] {
        check(text);
    }
}

#[test]
fn g139_nonlocal_replacement_source_safety_neighbors() {
    for text in [
        include_str!("captured-old-desc.zil"),
        include_str!("store-old-desc.zil"),
        include_str!("same-inputs.zil"),
        include_str!("pin-overlap.zil"),
        include_str!("nested-old-desc.zil"),
        include_str!("nested-partial.zil"),
        include_str!("public-role-old-desc-probe.zil"),
    ] {
        let module = typed(text);
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
    }
    for text in [
        include_str!("captured-escape.zil"),
        include_str!("captured-return.zil"),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
        assert_ne!(report.phase, Phase::Parse, "{}", report.to_json());
    }
}

fn verify_reject(module: &zil_ir::TypedModule, plan: &zil_ir::CandidatePlan, code: &str) {
    let limits = Limits::default();
    let mut claim = plan.clone();
    claim.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(module, &claim, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module.clone(), claim, &module.target, &limits),
        zil_verify::verify_module_bytes(&bytes, &module.target, &limits),
    ] {
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|d| d.code == code), "{errors:?}");
    }
}
#[test]
fn g139_independent_verify_abort_and_unpaid_old_owner_neighbors() {
    let module = typed(include_str!("store-abort.zil"));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut missing_abort = module.clone();
    let mut removed = 0;
    for f in &mut missing_abort.functions {
        for block in &mut f.blocks {
            block.instructions.retain(|i| {
                if matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }) {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
        }
    }
    assert!(removed > 0);
    verify_reject(&missing_abort, &plan, "E-VERIFY-PLACE-REPLACE");
    let module = typed(include_str!("two-leaf-helper.zil"));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut missing_old_cleanup = module;
    let f = missing_old_cleanup
        .functions
        .iter_mut()
        .find(|f| f.name == "work")
        .unwrap();
    let mut removed = 0;
    for block in &mut f.blocks {
        block.instructions.retain(|i| {
            if matches!(i.kind, InstructionKind::DropPackage { .. }) {
                removed += 1;
                false
            } else {
                true
            }
        });
    }
    assert_eq!(removed, 1);
    verify_reject(&missing_old_cleanup, &plan, "E-VERIFY-RESOURCE");
}
#[test]
fn g139_independent_verify_captured_and_store_retirement() {
    for (text, code) in [
        (include_str!("captured-old-desc.zil"), "E-VERIFY-RESOURCE"),
        (include_str!("store-old-desc.zil"), "E-BORROW-CONFLICT"),
        (
            include_str!("public-role-old-desc-probe.zil"),
            "E-VERIFY-RESOURCE",
        ),
    ] {
        let module = typed(text);
        let mut safe = module.clone();
        let f = safe
            .functions
            .iter_mut()
            .find(|f| f.name == "work")
            .unwrap();
        let block = f
            .blocks
            .iter_mut()
            .find(|block| {
                block
                    .instructions
                    .iter()
                    .any(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
            })
            .unwrap();
        let read = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
            .unwrap();
        let InstructionKind::ReadDesc { desc, .. } = block.instructions[read].kind else {
            unreachable!()
        };
        let start = block
            .instructions
            .iter()
            .position(|i| i.result == Some(desc))
            .unwrap();
        let moved: Vec<_> = block.instructions.drain(start..=read).collect();
        let borrow = block
            .instructions
            .iter()
            .position(|i| {
                matches!(
                    i.kind,
                    InstructionKind::BorrowLocalMut { .. }
                        | InstructionKind::OwnedStoreBorrowMut { .. }
                )
            })
            .unwrap();
        block.instructions.splice(borrow..borrow, moved);
        let plan = zil_analyze::analyze(&safe, &Limits::default()).unwrap();
        zil_verify::verify_module(safe.clone(), plan.clone(), &safe.target, &Limits::default())
            .unwrap();
        verify_reject(&module, &plan, code);
    }
}
#[test]
fn g139_native_old_drop_full_store_and_preparation_abort() {
    use zil_driver::{NativeBackend, Optimization};
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text, expected) in [
                ("capture", CAPTURE, b"AB42\n".as_slice()),
                (
                    "public-role-capture",
                    include_str!("public-role-capture-probe.zil"),
                    b"AB42\n".as_slice(),
                ),
                (
                    "public-role-repeat",
                    include_str!("public-role-repeat.zil"),
                    b"ABB42\n".as_slice(),
                ),
                (
                    "public-role-no-call",
                    include_str!("public-role-no-call.zil"),
                    b"A42\n".as_slice(),
                ),
                (
                    "public-role-loop",
                    include_str!("public-role-loop.zil"),
                    b"ABB42\n".as_slice(),
                ),
                ("store", STORE, b"AB42\n".as_slice()),
                (
                    "tuple-store-local",
                    include_str!("tuple-store-local.zil"),
                    b"AB42\n".as_slice(),
                ),
                (
                    "enum-package-helper",
                    include_str!("enum-package-helper.zil"),
                    b"ABCDABEF42,43\n".as_slice(),
                ),
                (
                    "enum-package-local",
                    include_str!("enum-package-local.zil"),
                    b"ABCDABEF42,43\n".as_slice(),
                ),
                (
                    "whole-store-local",
                    include_str!("whole-store-local.zil"),
                    b"AB42\n".as_slice(),
                ),
                (
                    "whole-store-apart",
                    include_str!("whole-store-apart.zil"),
                    b"ACB42\n".as_slice(),
                ),
                (
                    "whole-store-abort",
                    include_str!("whole-store-abort.zil"),
                    b"BA42\n".as_slice(),
                ),
                (
                    "tuple-helper",
                    include_str!("tuple-whole-helper.zil"),
                    b"AB42\n".as_slice(),
                ),
                (
                    "tuple-local",
                    include_str!("tuple-whole-local.zil"),
                    b"AB42\n".as_slice(),
                ),
                (
                    "tuple-abort",
                    include_str!("tuple-whole-abort.zil"),
                    b"BA42\n".as_slice(),
                ),
                (
                    "error-helper",
                    include_str!("error-whole-helper.zil"),
                    b"ABA42,42\n".as_slice(),
                ),
                (
                    "error-abort",
                    include_str!("error-whole-abort.zil"),
                    b"BA42\n".as_slice(),
                ),
                (
                    "owning-result",
                    include_str!("owning-result-helper.zil"),
                    b"ACB42\n".as_slice(),
                ),
                (
                    "owning-give-result",
                    include_str!("owning-result-give.zil"),
                    b"ACB42\n".as_slice(),
                ),
                (
                    "owning-result-abort",
                    include_str!("owning-result-abort.zil"),
                    b"BA42\n".as_slice(),
                ),
                (
                    "owning-error-abort",
                    include_str!("owning-error-abort.zil"),
                    b"BCA42\n".as_slice(),
                ),
                (
                    "store-owning-result",
                    include_str!("store-owning-result.zil"),
                    b"ACB42\n".as_slice(),
                ),
                (
                    "two-leaf",
                    include_str!("two-leaf-helper.zil"),
                    b"ABCD42\n".as_slice(),
                ),
                (
                    "local-two-leaf",
                    include_str!("two-leaf-local.zil"),
                    b"ABCD42\n".as_slice(),
                ),
                (
                    "tagged",
                    include_str!("enum-helper.zil"),
                    b"ABAC42,43\n".as_slice(),
                ),
                (
                    "multiple-leaves-abort",
                    include_str!("multiple-leaves-abort.zil"),
                    b"CAB42\n".as_slice(),
                ),
                (
                    "store-helper",
                    include_str!("store-helper.zil"),
                    b"AB42\n".as_slice(),
                ),
                (
                    "enum-multileaf",
                    include_str!("enum-multiple-leaves.zil"),
                    b"AFBCAFDE42,43\n".as_slice(),
                ),
                (
                    "nested-helper",
                    include_str!("nested-package-helper.zil"),
                    b"ABCD42\n".as_slice(),
                ),
                (
                    "nested-local",
                    include_str!("nested-package-local.zil"),
                    b"ABCD42\n".as_slice(),
                ),
                (
                    "store-abort",
                    include_str!("store-abort.zil"),
                    b"BA42\n".as_slice(),
                ),
            ] {
                let directory = Temp::new();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(directory.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let output = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{name}: {output:?}");
                assert_eq!(
                    output.stdout, expected,
                    "{name}/{backend:?}/{optimization:?}"
                );
                assert!(output.stderr.is_empty());
            }
        }
    }
}

#[test]
fn g139_public_role_capture_current_contract_probe() {
    let module = typed(include_str!("public-role-capture-probe.zil"));
    for function in &module.functions {
        if let Some(contract) = &function.callable_contract {
            println!("G139 public-role helper {} {:?}", function.name, contract);
            for block in &function.blocks {
                println!("G139 actual helper instructions {:?}", block.instructions);
            }
        }
    }
    let result = zil_analyze::analyze(&module, &Limits::default());
    println!("G139 public-role actual Analyze success={}", result.is_ok());
    if result.is_err() {
        for function in &module.functions {
            println!(
                "G139 rejected actual function {} {:?}",
                function.name, function.blocks
            );
        }
    }
    assert!(result.is_ok(), "{result:?}");
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
    );
}

#[test]
fn g139_public_role_trace_cannot_hide_actual_invocation() {
    let module = typed(include_str!("public-role-capture-probe.zil"));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut forged = module.clone();
    let mut changed = 0;
    for f in &mut forged.functions {
        if let Some(contract) = &mut f.callable_contract
            && let zil_ir::CallableTraces::Finite(paths) = &mut contract.traces
        {
            for path in paths {
                changed += path.calls.len();
                path.calls.clear();
            }
        }
    }
    assert!(changed > 0);
    let mut claim = plan;
    claim.module_fingerprint = forged.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
    for result in [
        zil_verify::verify_module(forged.clone(), claim, &forged.target, &limits),
        zil_verify::verify_module_bytes(&bytes, &forged.target, &limits),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
    }
}

#[test]
fn g139_owned_result_helper_responsibility_and_failure() {
    for text in [
        include_str!("owning-result-helper.zil"),
        include_str!("owning-result-give.zil"),
        include_str!("owning-result-abort.zil"),
        include_str!("owning-error-abort.zil"),
        include_str!("store-owning-result.zil"),
    ] {
        check(text);
    }
    for text in [
        include_str!("owning-result-old-desc.zil"),
        include_str!("owning-result-no-must.zil"),
        include_str!("owning-result-false-create.zil"),
    ] {
        let module = typed(text);
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
    }
    let safe = typed(include_str!("owning-result-give.zil"));
    let plan = zil_analyze::analyze(&safe, &Limits::default()).unwrap();
    let mut false_create = safe.clone();
    false_create
        .functions
        .iter_mut()
        .find(|f| f.name == "reset")
        .unwrap()
        .result_mode = zil_ir::PassingMode::Create;
    verify_reject(&false_create, &plan, "E-VERIFY-RESOURCE");

    let module = typed(include_str!("owning-result-helper.zil"));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut unpaid_result = module;
    let work = unpaid_result
        .functions
        .iter_mut()
        .find(|f| f.name == "work")
        .unwrap();
    let mut removed = 0;
    let extra = work
        .locals
        .iter()
        .find(|local| local.name == "extra")
        .unwrap()
        .id;
    for block in &mut work.blocks {
        block.instructions.retain(|i| {
            if matches!(i.kind, InstructionKind::DropManaged { local } if local == extra) {
                removed += 1;
                false
            } else {
                true
            }
        });
    }
    assert!(removed > 0);
    verify_reject(&unpaid_result, &plan, "E-VERIFY-RESOURCE");
}

#[test]
fn g139_whole_sum_responsibility_and_failure() {
    for text in [
        include_str!("tuple-store-local.zil"),
        include_str!("enum-package-helper.zil"),
        include_str!("enum-package-local.zil"),
        include_str!("tuple-whole-helper.zil"),
        include_str!("tuple-whole-local.zil"),
        include_str!("tuple-whole-abort.zil"),
        include_str!("error-whole-helper.zil"),
        include_str!("error-whole-abort.zil"),
    ] {
        check(text);
    }
    for text in [
        include_str!("tuple-whole-old-desc.zil"),
        include_str!("tuple-whole-partial.zil"),
        include_str!("enum-package-old-desc.zil"),
        include_str!("enum-package-no-must.zil"),
    ] {
        let module = typed(text);
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
    }
    for text in [
        include_str!("tuple-whole-abort.zil"),
        include_str!("error-whole-abort.zil"),
    ] {
        let module = typed(text);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let mut missing_abort = module;
        let mut removed = 0;
        for f in &mut missing_abort.functions {
            for block in &mut f.blocks {
                block.instructions.retain(|i| {
                    if matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }) {
                        removed += 1;
                        false
                    } else {
                        true
                    }
                });
            }
        }
        assert!(removed > 0);
        verify_reject(&missing_abort, &plan, "E-VERIFY-PLACE-REPLACE");
    }
}

#[test]
fn g139_whole_store_responsibility_and_failure() {
    for text in [
        include_str!("whole-store-local.zil"),
        include_str!("whole-store-apart.zil"),
        include_str!("whole-store-abort.zil"),
    ] {
        check(text);
    }
    let module = typed(include_str!("whole-store-old-desc.zil"));
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
    let report = compile_source(
        &source(include_str!("whole-store-helper.zil")),
        &CompileOptions {
            stop_after: Stage::Types,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
}

#[test]
fn g139_enum_store_declaration_keeps_existing_qualification_boundary() {
    // The declaration itself is outside the current ordinary owned shape:
    // Store<P> is a bare header type here, unlike an inferred Managed<Store<P>>
    // result inside the admitted Tuple source tested above.
    for text in [
        include_str!("enum-store-helper.zil"),
        "resource R{n:i32} package P{manages owner:R;n:i32} enum E{Left(Store<P>),Right(Store<P>)} fn main(){}",
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Types,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d
                .message
                .contains("enum payload requires finite Copy or an ordinary complete owned value")),
            "{}",
            report.to_json()
        );
    }
}

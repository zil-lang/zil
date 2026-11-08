//! Ordinary owner observations retain their independent lifetime and loans.
use super::*;
use zil_base::{Diagnostic, Phase};
use zil_ir::{LocalId, TypedModule};

const MODEL: &str = include_str!("stored_composition/model.zil");
const LIBRARY: &str = include_str!("stored_composition/library.zil");
const PROTOCOL: &str = include_str!("stored_composition/protocol.zil");
const MAIN: &str = include_str!("stored_composition/main.zil");

fn files() -> [(&'static str, &'static str); 4] {
    [
        ("main", MAIN),
        ("model", MODEL),
        ("protocol", PROTOCOL),
        ("library", LIBRARY),
    ]
}

// Raw attacks additionally use the same provider bodies in one actual source
// module. Only imports and their module qualifiers are removed; the delivered
// positive program retains its original four-module bytes above.
fn flat(model: &str) -> String {
    format!("{PROTOCOL}\n{LIBRARY}\n{model}\n{MAIN}")
        .replace("import protocol", "")
        .replace("import library", "")
        .replace("import model", "")
        .replace("protocol.", "")
        .replace("library.", "")
        .replace("model.", "")
}

fn typed(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "ordinary-package-fields.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

fn control(module: &TypedModule) -> zil_ir::CandidatePlan {
    let limits = Limits::default();
    let plan = zil_analyze::analyze(module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    plan
}

fn rejected(result: Result<impl Sized, Vec<Diagnostic>>, label: &str) -> Vec<Diagnostic> {
    let errors = result.err().unwrap_or_else(|| panic!("{label}: accepted"));
    eprintln!("{label}: {errors:?}");
    assert!(
        errors.iter().all(|d| matches!(
            d.category,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        )),
        "{label}: {errors:?}"
    );
    assert!(
        !errors
            .iter()
            .any(|d| d.code.contains("FINGERPRINT") || d.code.ends_with("CLAIM")),
        "{label}: metadata is not a semantic rejection: {errors:?}"
    );
    errors
}

#[test]
fn ordinary_package_fields_preserve_complete_lending_and_owned_results() {
    let report = compile_project(&project(&files()), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn ordinary_package_fields_execute_the_complete_project() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-owner-observation-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let report = compile_project(
                &project(&files()),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(dir.join("execution.txt"), format!("{output:?}")).unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
            eprintln!(
                "ordinary Package observations {backend:?}/{optimization:?}: {}",
                dir.display()
            );
        }
    }
}

#[test]
fn ordinary_package_fields_raw_and_wire_keep_actual_shell_child_and_loans() {
    let guarded = MODEL.replace(
        "let copy:Item=original.duplicate()",
        "let copy:Item=original.duplicate();var spare:Item=original.duplicate();let exclusive=&mut spare",
    ).replace("if consume(copy)!=21", "if exclusive.tag!=21{return 40}if consume(copy)!=21")
     .replace("if consume(copy)!=23", "if exclusive.tag!=23{return 41}if consume(copy)!=23");
    let module = typed(&flat(&guarded));
    let plan = control(&module);
    for attack in [
        "exclusive-read",
        "owner-field",
        "field-index",
        "result-type",
        "moved-shell",
    ] {
        let mut raw = module.clone();
        let function = raw.functions.iter_mut().find(|f| f.name == "run").unwrap();
        let copies = function
            .locals
            .iter()
            .filter(|l| l.name == "copy")
            .map(|l| l.id)
            .collect::<Vec<_>>();
        let spare = function
            .locals
            .iter()
            .find(|l| l.name == "spare")
            .unwrap()
            .id;
        let first_copy = copies[0];
        let target = function.blocks.iter_mut().flat_map(|b| &mut b.instructions).find(|i| {
            matches!(i.kind, InstructionKind::ReadPackageField { local, .. }
                if function.locals[local.0 as usize].name == if attack == "moved-shell" { "fresh" } else { "copy" })
        }).unwrap();
        let InstructionKind::ReadPackageField { local, index } = &mut target.kind else {
            unreachable!()
        };
        match attack {
            "exclusive-read" => *local = spare,
            "owner-field" => *index = 0,
            "field-index" => *index = u32::MAX,
            "result-type" => {
                function.values[target.result.unwrap().0 as usize] = zil_ir::Type::Bool
            }
            "moved-shell" => *local = first_copy,
            _ => unreachable!(),
        }
        let mut claim = plan.clone();
        assert_ne!(claim.module_fingerprint, raw.fingerprint());
        claim.module_fingerprint = raw.fingerprint();
        let a = rejected(
            zil_analyze::analyze(&raw, &Limits::default()),
            &format!("{attack}/Analyze"),
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &Limits::default()).unwrap();
        for errors in [
            rejected(
                zil_verify::verify_module(raw.clone(), claim, &raw.target, &Limits::default()),
                &format!("{attack}/Verify"),
            ),
            rejected(
                zil_verify::verify_module_bytes(&wire, &raw.target, &Limits::default()),
                &format!("{attack}/Wire"),
            ),
        ] {
            if attack == "exclusive-read" {
                assert!(
                    a.iter().any(|d| d.code == "E-ANALYZE-IR"
                        && d.message.contains("actual stored target/parent loan")),
                    "{a:?}"
                );
                assert!(
                    errors
                        .iter()
                        .any(|d| d.code == "E-BORROW-CONFLICT" || d.code == "E-VERIFY-STORED"),
                    "{errors:?}"
                );
            }
        }
    }
}

#[test]
fn ordinary_package_fields_do_not_replace_qualified_leaf_transport() {
    let source = include_str!("../../../fixtures/resource-packages/qualified-record-getter.zil");
    let module = typed(source);
    let plan = control(&module);
    let mut raw = module.clone();
    let mut changed = false;
    for function in &mut raw.functions {
        for i in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::BorrowPackageField { local, index, .. } = i.kind
                && matches!(
                    function.locals[local.0 as usize].ty,
                    zil_ir::Type::BorrowedStorage { .. }
                )
            {
                i.kind = InstructionKind::ReadPackageField { local, index };
                changed = true;
                break;
            }
        }
        if changed {
            break;
        }
    }
    assert!(changed, "qualified field producer is required");
    let mut claim = plan;
    claim.module_fingerprint = raw.fingerprint();
    rejected(
        zil_analyze::analyze(&raw, &Limits::default()),
        "qualified field/Analyze",
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &Limits::default()).unwrap();
    rejected(
        zil_verify::verify_module(raw.clone(), claim, &raw.target, &Limits::default()),
        "qualified field/Verify",
    );
    rejected(
        zil_verify::verify_module_bytes(&wire, &raw.target, &Limits::default()),
        "qualified field/Wire",
    );
}

#[test]
fn ordinary_package_fields_have_exact_fresh_analysis_and_verification_work() {
    let module = typed(&flat(MODEL));
    let plan = control(&module);
    for phase in [Phase::Analyze, Phase::Verify] {
        let run = |n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            if phase == Phase::Analyze {
                zil_analyze::analyze(&module, &limits).map(|actual| assert_eq!(actual, plan))
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                    .map(|_| ())
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        run(high).unwrap();
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(()) => high = mid,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = mid;
                }
            }
        }
        run(high).unwrap();
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
        run(high).unwrap();
        eprintln!("ordinary Package Copy observation {phase:?} N={high}; N-1/fresh agree");
    }
}

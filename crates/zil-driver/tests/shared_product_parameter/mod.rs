//! Frozen source consumers and an independent arithmetic/Store-identity oracle.
//! Core raw fixtures belong to the independent Analyze/Verify slices.
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CallableKind, HoldRoot, InstructionKind, Type};

#[path = "assembly_windows.rs"]
mod assembly_windows;
#[path = "raw_prefix.rs"]
mod raw_prefix;

const ORACLE: &str =
    include_str!("../../../../tests/shared-product-parameter-capture/expectations.tsv");
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case {
    name: &'static str,
    source: &'static str,
    success: bool,
    phase: Phase,
    exit: Option<i32>,
}

macro_rules! source {
    ($name:literal) => {
        include_str!(concat!(
            "../../../../tests/shared-product-parameter-capture/sources/",
            $name,
            ".zil"
        ))
    };
}

fn cases() -> Vec<Case> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 6);
            let source = match fields[0] {
                "original-local" => source!("original-local"),
                "local-last-use" => source!("local-last-use"),
                "local-last-use-direct" => source!("local-last-use-direct"),
                "store-last-use" => source!("store-last-use"),
                "store-last-use-direct" => source!("store-last-use-direct"),
                "different-store" => source!("different-store"),
                "different-store-pop" => source!("different-store-pop"),
                "different-store-after-read" => source!("different-store-after-read"),
                "different-store-fields" => source!("different-store-fields"),
                "same-store-field" => source!("same-store-field"),
                "give-different-store" => source!("give-different-store"),
                "give-different-store-direct" => source!("give-different-store-direct"),
                "give-same-store" => source!("give-same-store"),
                "give-same-store-direct" => source!("give-same-store-direct"),
                "different-owner-db" => source!("different-owner-db"),
                "different-owner-db-direct" => source!("different-owner-db-direct"),
                "move-owner-db" => source!("move-owner-db"),
                "move-owner-db-direct" => source!("move-owner-db-direct"),
                "inline-db-give" => source!("inline-db-give"),
                "inline-db-give-direct" => source!("inline-db-give-direct"),
                "inline-db-disjoint" => source!("inline-db-disjoint"),
                "inline-db-disjoint-direct" => source!("inline-db-disjoint-direct"),
                "assembled-then-take-field" => source!("assembled-then-take-field"),
                "assembled-then-take-field-direct" => source!("assembled-then-take-field-direct"),
                "assembled-other-field-after-borrow" => {
                    source!("assembled-other-field-after-borrow")
                }
                "assembled-extract-then-move" => source!("assembled-extract-then-move"),
                "assembled-take-field-disjoint" => source!("assembled-take-field-disjoint"),
                "field-borrow-after-assemble" => source!("field-borrow-after-assemble"),
                "refill-same-store" => source!("refill-same-store"),
                "refill-disjoint-store" => source!("refill-disjoint-store"),
                "swap-fields-left-view" => source!("swap-fields-left-view"),
                "swap-fields-right-view" => source!("swap-fields-right-view"),
                "swap-fields-same-store" => source!("swap-fields-same-store"),
                "swap-crosses-live-window" => source!("swap-crosses-live-window"),
                "swap-identity-left-view" => source!("swap-identity-left-view"),
                "swap-identity-right-view" => source!("swap-identity-right-view"),
                "different-store-direct" => source!("different-store-direct"),
                "same-store-reserve" => source!("same-store-reserve"),
                "same-store-reserve-direct" => source!("same-store-reserve-direct"),
                "same-store-pop" => source!("same-store-pop"),
                "same-store-pop-direct" => source!("same-store-pop-direct"),
                "inside-call-last-use" => source!("inside-call-last-use"),
                "inside-call-last-use-direct" => source!("inside-call-last-use-direct"),
                name => panic!("unregistered frozen source {name}"),
            };
            assert_eq!(Fingerprint::of(source.as_bytes()).to_hex(), fields[1]);
            let success = match fields[2] {
                "success" => true,
                "borrow-conflict" => false,
                other => panic!("unknown oracle outcome {other}"),
            };
            let phase = match fields[3] {
                "analyze" => Phase::Analyze,
                "verify" => Phase::Verify,
                other => panic!("unknown exact source phase {other}"),
            };
            if success {
                assert_eq!(phase, Phase::Verify);
            }
            let exit = if fields[4] == "-" {
                None
            } else {
                Some(fields[4].parse().unwrap())
            };
            assert_eq!(success, exit.is_some());
            Case {
                name: fields[0],
                source,
                success,
                phase,
                exit,
            }
        })
        .collect()
}

fn record(case: &Case, route: &str, result: &str) {
    eprintln!("shared-product-parameter {} {route}: {result}", case.name);
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let root = PathBuf::from(root).join("shared-product-parameter");
        fs::create_dir_all(&root).unwrap();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("results.tsv"))
            .unwrap();
        writeln!(
            file,
            "{}\t{}\t{route}\t{result}",
            case.name,
            Fingerprint::of(case.source.as_bytes())
        )
        .unwrap();
    }
}

#[test]
fn source_oracles_are_frozen() {
    let cases = cases();
    assert_eq!(cases.len(), 43);
    assert_eq!(cases.iter().filter(|case| case.success).count(), 23);
}

#[test]
fn frozen_shared_parameter_sources_reach_verify() {
    let mut failures = Vec::new();
    for case in cases().into_iter().filter(|case| case.success) {
        let source = SourceUnit::new(FileId(0), ModuleId(0), case.name, case.source);
        let report = compile_source(&source, &CompileOptions::default());
        let result = format!(
            "{:?}/{:?} verification_runs={}",
            report.status, report.phase, report.verification_runs
        );
        record(&case, "source-verify", &result);
        if report.status != ResultCategory::Success
            || report.phase != Phase::Verify
            || report.verification_runs != 1
        {
            failures.push(format!("{}: {}", case.name, report.to_json()));
        }
    }
    assert!(
        failures.is_empty(),
        "legal sources remain unimplemented:\n{}",
        failures.join("\n")
    );
}

#[test]
fn same_store_invalidation_reaches_the_borrow_conflict_boundary() {
    let mut failures = Vec::new();
    for case in cases().into_iter().filter(|case| !case.success) {
        let source = SourceUnit::new(FileId(0), ModuleId(0), case.name, case.source);
        let report = compile_source(&source, &CompileOptions::default());
        let result = format!(
            "{:?}/{:?} verification_runs={}",
            report.status, report.phase, report.verification_runs
        );
        record(&case, "source-negative", &result);
        if report.status != ResultCategory::SafeReject
            || report.phase != case.phase
            || report.verification_runs != usize::from(case.phase == Phase::Verify)
            || !report
                .diagnostics
                .iter()
                .any(|d| d.phase == case.phase && d.code == "E-BORROW-CONFLICT")
        {
            failures.push(format!("{}: {}", case.name, report.to_json()));
        }
        assert!(!report.artifacts.contains_key("executable"));
    }
    assert!(
        failures.is_empty(),
        "wrong rejection boundary:\n{}",
        failures.join("\n")
    );
}

#[test]
fn source_derived_parameter_holds_survive_raw_and_wire_verification() {
    for case in cases()
        .into_iter()
        .filter(|case| case.success && !case.name.ends_with("-direct"))
    {
        let source = SourceUnit::new(FileId(0), ModuleId(0), case.name, case.source);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &syntax).unwrap();
        let target = zil_base::TargetProfile::host_llvm();
        let limits = Limits::default();
        let module = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&module, &limits).unwrap();
        let render = module
            .functions
            .iter()
            .find(|f| f.name == "render")
            .unwrap();
        let captures = render
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|instruction| match &instruction.kind {
                InstructionKind::MakeClosure { closure, captures } => Some((*closure, captures)),
                _ => None,
            })
            .unwrap();
        assert_eq!(captures.1.len(), 1);
        let capture = captures.1[0];
        assert!(matches!(&render.values[capture.0 as usize],
            Type::Address { mutable: false, inner } if matches!(inner.as_ref(), Type::Package { .. })));
        assert_eq!(
            module.closures[captures.0.0 as usize].kind,
            CallableKind::Fn
        );
        let function_plan = plan
            .functions
            .iter()
            .find(|f| f.function == render.id)
            .unwrap();
        assert_eq!(
            function_plan
                .holds
                .iter()
                .find(|h| h.value == capture)
                .unwrap()
                .roots,
            vec![HoldRoot::Parameter(0)],
            "{}: borrowed parameter must remain Parameter",
            case.name
        );
        let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        zil_verify::verify_module(module, plan, &target, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        record(&case, "source-derived-raw-wire", "Pass Parameter(0)");
    }
}

#[test]
fn same_store_raw_and_wire_conflicts_bypass_the_analyze_rejection() {
    for case in cases().into_iter().filter(|case| {
        matches!(
            case.name,
            "different-store" | "different-store-pop" | "different-store-after-read"
        )
    }) {
        let source = SourceUnit::new(FileId(0), ModuleId(0), case.name, case.source);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &syntax).unwrap();
        let target = zil_base::TargetProfile::host_llvm();
        let limits = Limits::default();
        let mut module = zil_types::check(&core, &target, &limits).unwrap();
        let mut plan = raw_prefix::straight_line_claims(&module);
        let prefix = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
        zil_verify::verify_module_bytes(&prefix, &target, &limits).unwrap();
        let render = module
            .functions
            .iter()
            .find(|f| f.name == "render")
            .unwrap()
            .id;
        let main = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let borrowed_store = main
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|instruction| match instruction.kind {
                InstructionKind::OwnedStoreBorrow { store, .. } => Some(store),
                _ => None,
            })
            .unwrap();
        let mut changed = 0;
        for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::Call {
                function,
                arguments,
            } = &mut instruction.kind
                && *function == render
            {
                assert_eq!(arguments.len(), 2);
                assert_ne!(arguments[1], borrowed_store);
                assert_eq!(
                    main.values[arguments[1].0 as usize],
                    main.values[borrowed_store.0 as usize]
                );
                // Keep the exact legal CFG/holds and change only the actual
                // Store identity supplied alongside its borrowed element.
                arguments[1] = borrowed_store;
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
        plan.module_fingerprint = module.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        // Do not call Analyze on the mutation: its earlier rejection cannot
        // substitute for this independent Verify boundary.
        for (route, result) in [
            (
                "same-store-raw",
                zil_verify::verify_module(module, plan, &target, &limits),
            ),
            (
                "same-store-wire",
                zil_verify::verify_module_bytes(&wire, &target, &limits),
            ),
        ] {
            let diagnostics = result.unwrap_err();
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject
                        && d.phase == Phase::Verify
                        && d.code == "E-BORROW-CONFLICT"),
                "{} {route}: {diagnostics:?}",
                case.name
            );
            record(&case, route, "SafeReject/Verify E-BORROW-CONFLICT");
        }
    }
}

#[test]
fn field_and_owner_raw_wire_boundaries_are_independent_of_analyze() {
    let cases = cases();
    for (positive, negative) in [
        ("different-store-fields", "same-store-field"),
        ("give-different-store", "give-same-store"),
        ("different-owner-db", "move-owner-db"),
        ("inline-db-disjoint", "inline-db-give"),
    ] {
        for (name, expected_success) in [(positive, true), (negative, false)] {
            let case = cases.iter().find(|case| case.name == name).unwrap();
            let source = SourceUnit::new(FileId(0), ModuleId(0), case.name, case.source);
            let syntax = zil_syntax::parse(&source).into_result().unwrap();
            let core = zil_expand::expand(&source, &syntax).unwrap();
            let target = zil_base::TargetProfile::host_llvm();
            let limits = Limits::default();
            let module = zil_types::check(&core, &target, &limits).unwrap();
            let plan = raw_prefix::straight_line_claims(&module);
            let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            // Both the lawful source prefix and its closest alias/owner
            // counterexample go directly to Verify. Analyze is never invoked.
            for (route, result) in [
                (
                    "owner-field-raw",
                    zil_verify::verify_module(module, plan, &target, &limits),
                ),
                (
                    "owner-field-wire",
                    zil_verify::verify_module_bytes(&wire, &target, &limits),
                ),
            ] {
                if expected_success {
                    result.unwrap_or_else(|errors| panic!("{name} {route}: {errors:?}"));
                    record(case, route, "Success/Verify");
                } else {
                    let errors = result.unwrap_err();
                    assert!(
                        errors.iter().any(|d| d.phase == Phase::Verify
                            && d.category == ResultCategory::SafeReject
                            && d.code == "E-BORROW-CONFLICT"),
                        "{name} {route}: {errors:?}"
                    );
                    record(case, route, "SafeReject/Verify E-BORROW-CONFLICT");
                }
            }
        }
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn frozen_positive_sources_run_natively_at_o0_and_o2() {
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    for case in cases().into_iter().filter(|case| case.success) {
        for optimization in [Optimization::O0, Optimization::O2] {
            let path = std::env::temp_dir().join(format!(
                "shared-product-parameter-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            let temporary = Temporary(path);
            let source = SourceUnit::new(FileId(0), ModuleId(0), case.name, case.source);
            let report = compile_source(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(temporary.0.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}: {}",
                case.name,
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let summary = report.native_codegen.as_ref().unwrap();
            assert_eq!(summary.backend, backend);
            assert!(summary.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(!summary.object_reports.is_empty());
                assert!(
                    summary
                        .object_reports
                        .iter()
                        .all(|r| r.contains("\"input_kind\":\"builder\""))
                );
            }
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), case.exit, "{}: {output:?}", case.name);
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{}: {output:?}",
                case.name
            );
            record(
                &case,
                &format!("native-{backend:?}-{optimization:?}"),
                "Pass exit=0 stdout=empty stderr=empty",
            );
        }
    }
}

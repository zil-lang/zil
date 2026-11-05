//! Non-author consumers, included only after the exact production freeze.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::InstructionKind;

const INPUTS: &[(&str, &str, &[u8])] = &[
    (
        "captured",
        include_str!("captured-whole-package.zil"),
        b"AB42\n",
    ),
    ("store", include_str!("store-whole-package.zil"), b"AB42\n"),
    (
        "store-abort",
        include_str!("store-whole-package-abort.zil"),
        b"BA42\n",
    ),
    (
        "two-leaf",
        include_str!("two-leaf-package-helper.zil"),
        b"ABCD42\n",
    ),
    (
        "nested-package",
        include_str!("nested-package-helper.zil"),
        b"ABCD42\n",
    ),
    (
        "enum",
        include_str!("nonlocal-enum-helper.zil"),
        b"ABAC42,43\n",
    ),
    (
        "enum-abort",
        include_str!("nonlocal-enum-abort.zil"),
        b"CAB42\n",
    ),
    (
        "two-role-drop",
        include_str!("two-role-user-drop.zil"),
        b"ABBA91\n",
    ),
    (
        "two-role-loop",
        include_str!("two-role-fnmut-loop.zil"),
        b"45\n",
    ),
    ("lending", include_str!("qualified-lending.zil"), b""),
    (
        "lending-mut",
        include_str!("qualified-lending-mut.zil"),
        b"",
    ),
];

fn input(name: &str, text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), format!("g10110-{name}.zil"), text)
}

#[test]
fn g10110_source_and_honest_raw_wire() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for &(name, text, _) in INPUTS {
        let source = input(name, text);
        let report = compile_source(
            &source,
            &CompileOptions {
                stop_after: Stage::Verify,
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
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand_with_map_limits_target_context(
            &source,
            &syntax,
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &Default::default(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        if name == "store-abort" {
            let mut hostile = raw.clone();
            let block = hostile
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .find(|b| {
                    b.instructions
                        .iter()
                        .any(|i| matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }))
                })
                .expect("real Store-target error path has an Abort");
            let at = block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }))
                .unwrap();
            block.instructions.remove(at);
            assert!(zil_analyze::analyze(&hostile, &limits).is_err());
            let mut claim = plan.clone();
            claim.module_fingerprint = hostile.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&hostile, &claim, &limits).unwrap();
            assert!(zil_verify::verify_module(hostile, claim, &target, &limits).is_err());
            assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
        }
    }
}

fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(
        text.matches(from).count(),
        1,
        "mutation must select one actual source use"
    );
    text.replacen(from, to, 1)
}

#[test]
fn g10110_nearest_source_negatives() {
    let two = include_str!("two-leaf-package-helper.zil");
    let stale = replace_once(
        two,
        "reset(&mut packet);",
        "let stale=packet.second;reset(&mut packet);",
    );
    let stale = replace_once(&stale, "return packet.n;", "return packet.n+stale.code-66;");
    let store = replace_once(
        include_str!("store-whole-package.zil"),
        "return target.n;",
        "store_reserve(items,2);return target.n;",
    );
    let twice = replace_once(
        include_str!("two-role-user-drop.zil"),
        "return f(x)+g(x)",
        "return f(x)+f(x)+g(x)",
    );
    let overlap = replace_once(
        include_str!("qualified-lending-mut.zil"),
        "return old+*b;",
        "return old+*a+*b;",
    );
    let escaped = replace_once(
        include_str!("qualified-lending.zil"),
        "fn apply<life s>(left:&Tag<'s>)->i32",
        "fn apply<life s>(left:&Tag<'s>)->&i32",
    );
    let escaped = replace_once(&escaped, "return *a+*b;", "return a;");
    let escaped = replace_once(&escaped, "return apply(&tag)", "return *apply(&tag)");
    let aliased = replace_once(
        include_str!("two-role-fnmut-loop.zil"),
        "dispatch(&mut f,&mut g,left,3)",
        "dispatch(&mut f,&mut f,left,3)",
    );
    for (name, text, early_consumption) in [
        ("second-leaf-stale-desc", stale, false),
        ("same-store-reserve", store, false),
        ("actual-f-double-consume", twice, true),
        ("overlapping-lending-call", overlap, false),
        ("dead-lending-environment", escaped, false),
        ("two-mut-roles-one-instance", aliased, false),
    ] {
        let report = compile_source(
            &input(name, &text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert!(
            matches!(report.phase, Phase::Types | Phase::Analyze | Phase::Verify)
                || (early_consumption
                    && report.phase == Phase::Expand
                    && report
                        .diagnostics
                        .iter()
                        .any(|d| d.message.contains("consumed"))),
            "negative must reach a semantic obligation: {name}: {}",
            report.to_json()
        );
        eprintln!(
            "G10.1.10 negative {name}: {:?} {:?}",
            report.phase,
            report
                .diagnostics
                .iter()
                .map(|d| &d.code)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn g10110_native_complete_consumers() {
    for &(name, text, stdout) in INPUTS {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let directory = std::env::temp_dir().join(format!(
                    "zil-g10110-{}-{name}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&directory).unwrap();
                let report = compile_source(
                    &input(name, text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("main".into()),
                        output: Some(directory.join("program")),
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
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let output = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{name}: {output:?}");
                // CompileOptions.entry selects the existing full-return-value
                // harness: after explicit Source output it prints main's i32.
                // Every fixture independently computes zero on success.
                let mut expected = stdout.to_vec();
                expected.extend_from_slice(b"0\n");
                assert_eq!(output.stdout, expected, "{name}/{backend:?}/{optimization:?}");
                assert!(output.stderr.is_empty(), "{name}: {output:?}");
                std::fs::remove_dir_all(directory).unwrap();
            }
        }
    }
}

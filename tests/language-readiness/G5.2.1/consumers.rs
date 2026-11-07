//! Source-to-native dyn consumers and source-derived independent raw/Wire controls.
use super::*;
use std::io::Write;
use std::process::Stdio;
use zil_base::TargetProfile;
use zil_driver::NativeBackend;
use zil_ir::{InstructionKind as I, Type};

const TWO: &str = include_str!("dyn-two-provider.zil");
const INPUT: &str = include_str!("dyn-input.zil");

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "g521-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn compile(text: &str, options: &CompileOptions) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "g521.zil", text),
        options,
    )
}

fn backends() -> Vec<NativeBackend> {
    let mut result = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        result.push(NativeBackend::LlvmApi);
    }
    result
}

#[test]
fn g521_runtime_input_selects_two_real_vtables_in_both_backends() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("input");
            let executable = dir.join("program");
            let report = compile(
                INPUT,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
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
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
            assert_eq!(typed.matches("MakeDynBorrow {").count(), 2);
            assert!(typed.contains("CallDynSlot {"));
            for (input, expected) in [(b"1\n".as_slice(), b"20\n".as_slice()), (b"0\n", b"22\n")] {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(input).unwrap();
                let result = child.wait_with_output().unwrap();
                assert_eq!(result.status.code(), Some(0));
                assert_eq!(result.stdout, expected);
                assert!(result.stderr.is_empty(), "{result:?}");
            }
        }
    }
}

fn project_files(main: &'static str) -> [(&'static str, &'static str); 3] {
    [
        ("main", main),
        ("a", include_str!("project/a.zil")),
        ("b", include_str!("project/b.zil")),
    ]
}

#[test]
fn g521_cross_module_default_provider_and_nominal_interface_separation() {
    let files = project_files(include_str!("project/main.zil"));
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("project");
            let executable = dir.join("program");
            let report = compile_project(
                &project(&files),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let result = Command::new(executable).output().unwrap();
            assert_eq!(result.status.code(), Some(0));
            assert_eq!(result.stdout, b"29\n");
            assert!(result.stderr.is_empty());
        }
    }
    let wrong = project_files(include_str!("project/wrong-interface.zil"));
    let report = compile_project(&project(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Expand | Phase::Types),
        "{}",
        report.to_json()
    );
}

#[test]
fn g521_explicit_local_coercion_and_live_receiver_conflicts() {
    let explicit = TWO.replace(
        "return read_it(&a)+read_it(&b)",
        "let d:&dyn Read=&a;return read_it(d)+read_it(&b)",
    );
    let report = compile(&explicit, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let conflicting = TWO.replace(
        "fn main()->i32{let a=A{n:20};let b=B{n:22};return read_it(&a)+read_it(&b)}",
        "fn main()->i32{var a=A{n:20};let d:&dyn Read=&a;a=A{n:21};return read_it(d)}",
    );
    assert_ne!(conflicting, TWO);
    let report = compile(&conflicting, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Analyze | Phase::Verify),
        "{}",
        report.to_json()
    );
}

fn source_candidate() -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    // This fixture has no CT expressions. Parse its real Source and retain the
    // actual producer output; semantic cache intentionally excludes Dyn types.
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "g521.zil", INPUT);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn rejected(
    mut module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    mutate: impl FnOnce(&mut zil_ir::TypedModule),
) {
    mutate(&mut module);
    plan.module_fingerprint = module.fingerprint();
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    let target = module.target.clone();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for errors in [
        zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors
                .iter()
                .any(|e| e.phase == Phase::Verify && e.code == "E-VERIFY-DYN"),
            "{errors:?}"
        );
    }
}

#[test]
fn g521_source_derived_raw_wire_contract_identity_and_slot_controls() {
    let (module, plan) = source_candidate();
    let target = TargetProfile::host_llvm();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default()).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()).unwrap();
    rejected(module.clone(), plan.clone(), |m| {
        m.dyn_interfaces[0].slots[0].contract.effects = None
    });
    rejected(module.clone(), plan.clone(), |m| {
        m.dyn_vtables[0].self_type = m.dyn_vtables[1].self_type.clone()
    });
    rejected(module.clone(), plan.clone(), |m| {
        let instruction = m
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, I::CallDynSlot { .. }))
            .unwrap();
        if let I::CallDynSlot { slot, .. } = &mut instruction.kind {
            *slot = u32::MAX;
        }
    });
    // The raw table is a declaration of its selected target, not a certificate
    // that an arbitrary raw program still implements the original Source.
    let mut other = module.clone();
    let spare = other
        .functions
        .iter()
        .find(|f| f.name.ends_with("spare"))
        .unwrap()
        .id;
    assert!(
        other
            .dyn_vtables
            .iter()
            .all(|t| t.entries[0].function != spare)
    );
    let table = other
        .dyn_vtables
        .iter_mut()
        .find(|t| matches!(&t.self_type,Type::Record{ name,.. } if name.ends_with('A')))
        .unwrap();
    table.entries[0].function = spare;
    let mut other_plan = plan;
    other_plan.module_fingerprint = other.fingerprint();
    zil_analyze::analyze(&other, &Limits::default()).unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&other, &other_plan, &Limits::default()).unwrap();
    zil_verify::verify_module(other, other_plan, &target, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).unwrap();
}

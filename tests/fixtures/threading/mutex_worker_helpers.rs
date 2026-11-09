//! The thread entry ABI stays i32 while ordinary internal results stay ordinary.
use super::*;
use zil_driver::{NativeBackend, Optimization, Stage};
use zil_ir::{InstructionKind, Type};

const PROGRAM: &str = include_str!("mutex-worker-helpers.zil");
fn typed(program: &str) -> zil_ir::TypedModule {
    let input = source(program);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    zil_types::check(
        &core,
        &zil_base::TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap()
}
fn reject_raw(raw: zil_ir::TypedModule, mut candidate: zil_ir::CandidatePlan) {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    candidate.module_fingerprint = raw.fingerprint();
    let a = zil_analyze::analyze(&raw, &limits).unwrap_err();
    let v =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let w = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    for errors in [a, v, w] {
        assert!(
            errors.iter().any(|d| d.code == "E-THREAD-WORKER"),
            "{errors:?}"
        );
    }
}
#[test]
fn mutex_worker_internal_scalar_helpers_reach_source_raw_wire_and_native() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let report = compile_source(&source(PROGRAM), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let raw = typed(PROGRAM);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = std::env::temp_dir().join(format!(
                "mutex-worker-helpers-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            let report = compile_source(
                &source(PROGRAM),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(output.clone()),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let result = std::process::Command::new(output).output().unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            assert!(
                result.stdout.is_empty() && result.stderr.is_empty(),
                "{result:?}"
            );
            runs += 1;
        }
    }
    assert_eq!(runs, 2 + 2 * usize::from(cfg!(feature = "llvm-api")));
}
#[test]
fn mutex_worker_root_still_requires_the_actual_i32_result() {
    let raw = typed(PROGRAM);
    let plan = zil_analyze::analyze(&raw, &Limits::default()).unwrap();
    for name in ["increment", "positive"] {
        let mut changed = raw.clone();
        let root = changed
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .id;
        let mut count = 0;
        for f in &mut changed.functions {
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let InstructionKind::WithScopedThread { worker, .. } = &mut i.kind {
                    *worker = root;
                    count += 1;
                }
            }
        }
        assert_eq!(count, 1);
        reject_raw(changed, plan.clone());
    }
    for name in ["increment", "positive"] {
        let changed = PROGRAM.replace(
            "thread_scope(worker,cell",
            &format!("thread_scope({name},cell"),
        );
        let report = compile_source(&source(&changed), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn mutex_worker_follows_the_real_helper_body_and_keeps_closed_execution() {
    for forbidden in [
        "fn forbidden(cell:Mutex<i32>)->(){let operation=()->i32=>7;operation();}",
        "extern \"C\" fn getpid()->i32;fn forbidden(cell:Mutex<i32>)->(){unsafe{getpid();}}",
    ] {
        let control = format!("{forbidden}{PROGRAM}");
        let raw = typed(&control);
        let plan = zil_analyze::analyze(&raw, &Limits::default()).unwrap();
        zil_verify::verify_module(
            raw.clone(),
            plan.clone(),
            &zil_base::TargetProfile::host_llvm(),
            &Limits::default(),
        )
        .unwrap();
        let mut changed = raw.clone();
        let allowed = changed
            .functions
            .iter()
            .find(|f| f.name == "increment")
            .unwrap()
            .id;
        let denied = changed
            .functions
            .iter()
            .find(|f| f.name == "forbidden")
            .unwrap()
            .id;
        assert_eq!(
            changed.functions[allowed.0 as usize].return_type,
            Type::Unit
        );
        assert_eq!(
            changed.functions[allowed.0 as usize].return_type,
            changed.functions[denied.0 as usize].return_type
        );
        let mut count = 0;
        for f in &mut changed.functions {
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let InstructionKind::Call { function, .. } = &mut i.kind
                    && *function == allowed
                {
                    *function = denied;
                    count += 1;
                }
            }
        }
        assert_eq!(count, 1);
        reject_raw(changed, plan);
        let report = compile_source(
            &source(&control.replace("    increment(cell)", "    forbidden(cell)")),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-THREAD-WORKER"),
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn mutex_worker_helper_admission_does_not_include_suspend_entries() {
    let mut raw = typed(PROGRAM);
    let plan = zil_analyze::analyze(&raw, &Limits::default()).unwrap();
    raw.functions
        .iter_mut()
        .find(|f| f.name == "increment")
        .unwrap()
        .call_kind = zil_ir::CallKind::Suspend;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let mut candidate = plan;
    candidate.module_fingerprint = raw.fingerprint();
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
}

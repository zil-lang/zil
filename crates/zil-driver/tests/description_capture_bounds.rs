//! Actual weak descriptions are call dependencies, not retained address loans.
use std::{fs, process::Command};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const SOURCE: &str = "extern \"C\" fn putchar(n:i32)->i32;\nresource R{code:i32}\nimpl Drop for R{fn drop(self:Self){unsafe{putchar(65)}}}\nfn call<F:Fn()->i32>(value:&F)->i32{return value()}\nfn main()->i32{let item=R{code:42};let read=()=>item.code;return call(&read)-42}";
#[test]
fn genuine_description_capture_has_call_only_dependency_and_real_cleanup() {
    let input = SourceUnit::new(FileId(0), ModuleId(0), "description-capture.zil", SOURCE);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let closure = module
        .closures
        .iter()
        .find(|d| {
            d.captures
                .iter()
                .any(|t| matches!(t, zil_ir::Type::Desc(_)))
        })
        .expect("actual Desc capture");
    let role = module
        .functions
        .iter()
        .filter_map(|f| f.callable_contract.as_ref())
        .flat_map(|c| &c.roles)
        .find(|r| r.closure == closure.id)
        .expect("actual helper role");
    assert!(role.hold.is_empty(), "{role:?}");
    assert_eq!(role.call, [0]);
    assert_eq!(role.writes, [0]);
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-desc-capture-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let report = compile_source(
                &input,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(dir.join("program")),
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
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(!native.object_reports.is_empty());
                assert!(
                    native
                        .object_reports
                        .iter()
                        .all(|r| r.contains("\"input_kind\":\"builder\"")
                            && r.contains("\"verify_before_after\":true"))
                );
            }
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(output.stdout, b"A");
            assert!(output.stderr.is_empty());
        }
    }
    let invalid = SOURCE.replace(
        "return call(&read)-42",
        "let moved=give item;return call(&read)-42",
    );
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "retired-description-capture.zil",
            invalid,
        ),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

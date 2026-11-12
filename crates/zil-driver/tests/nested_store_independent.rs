//! Ordinary finite nested owners through the complete Source/independent
//! raw+Wire/native pipeline. Analyze contributes an untrusted public plan only.
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_path};
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn typed(source: &str) -> zil_ir::TypedModule {
    let sources = [
        ("main", source),
        ("iter", include_str!("../../../lib/std/iter.zil")),
        ("store", include_str!("../../../lib/std/store.zil")),
    ];
    let units: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(id, (name, text))| {
            SourceUnit::new(
                FileId(id as u32),
                ModuleId(id as u32),
                format!("{name}.zil"),
                *text,
            )
        })
        .collect();
    let parsed: Vec<_> = units
        .iter()
        .map(|unit| zil_syntax::parse(unit).into_result().unwrap())
        .collect();
    let syntax: Vec<_> = parsed.iter().collect();
    let imports: Vec<Vec<_>> = syntax
        .iter()
        .map(|module| {
            module
                .items
                .iter()
                .filter_map(|item| {
                    let zil_syntax::ast::ItemKind::Import(import) = &item.kind else {
                        return None;
                    };
                    let name = import.path.last().unwrap();
                    Some(zil_ir::CoreImport {
                        alias: import.alias.clone().unwrap_or_else(|| name.clone()),
                        module: ModuleId(
                            sources.iter().position(|(key, _)| key == name).unwrap() as u32
                        ),
                        span: item.span,
                    })
                })
                .collect()
        })
        .collect();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let modules =
        zil_expand::expand_program_with_limits_target(&units, &syntax, &imports, &limits, &target)
            .unwrap()
            .into_iter()
            .map(|(module, _)| module)
            .collect();
    zil_types::check_program(
        &zil_ir::CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::of(source.as_bytes()),
            modules,
        },
        &target,
        &limits,
    )
    .unwrap()
    .module
}

fn pipeline(source: &str, name: &str, trace: &[u8]) {
    let module = typed(source);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let root = base.join(format!(
                "nested-store-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let input = root.join("main.zil");
            fs::write(&input, source).unwrap();
            let report = compile_path(
                &input,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(root.join("program")),
                    emit_dir: Some(root.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(root.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let machine = report.native_codegen.as_ref().unwrap();
            assert_eq!(machine.backend, backend);
            assert!(machine.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(!machine.object_reports.is_empty());
                assert!(
                    machine
                        .object_reports
                        .iter()
                        .all(|r| r.contains("\"input_kind\":\"builder\"")
                            && r.contains("\"verify_before_after\":true"))
                );
            }
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(root.join("stdout"), &output.stdout).unwrap();
            fs::write(root.join("stderr"), &output.stderr).unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{name}/{backend:?}/{optimization:?}: {output:?}"
            );
            assert_eq!(output.stdout, trace, "{name}/{backend:?}/{optimization:?}");
            assert!(output.stderr.is_empty());
        }
    }
}
#[test]
fn complete_package_manager_chain() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/packages.zil"),
        "packages",
        b"AR",
    );
}
#[test]
fn complete_drain_elements() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/drains.zil"),
        "drains",
        b"AB",
    );
}
#[test]
fn complete_direct_store_owners() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/direct-store.zil"),
        "direct-store",
        b"A",
    );
}
#[test]
fn complete_direct_user_resource_owners() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/direct-ticket.zil"),
        "direct-ticket",
        b"BA",
    );
}
#[test]
fn complete_direct_byte_buffer_owners() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/direct-byte-buffer.zil"),
        "direct-byte-buffer",
        b"",
    );
}

#[test]
fn complete_saved_some_keeps_both_heap_layers_through_helpers() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/lending.zil"),
        "lending",
        b"AR",
    );
}

#[test]
fn ordinary_store_descriptions_share_returned_environment_source_lane() {
    pipeline(
        include_str!("../../../tests/fixtures/nested-store/ordinary-environment.zil"),
        "ordinary-environment",
        b"A",
    );
}

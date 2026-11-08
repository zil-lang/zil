//! Owning Package fields retain exact Array sources without new cleanup debt.
use super::*;
const PACKAGE_ARRAY: &str = include_str!("owned-package-shared-address-array.zil");

#[test]
fn source_raw_and_wire_keep_package_field_and_element_ordinals_distinct() {
    let permuted = PACKAGE_ARRAY
        .replacen("items:[first,second]", "items:[second,first]", 1)
        .replacen("first=11", "second=11", 1)
        .replacen("return *pointer-9", "return *pointer-7", 1);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for (label, program) in [("ordinary", PACKAGE_ARRAY), ("permuted", permuted.as_str())] {
        let report = compile_source(
            &source(label, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("Package Array {label}: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = typed_source(label, program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
}

#[test]
fn package_array_views_preserve_actual_parent_and_each_live_source() {
    let local_escape = PACKAGE_ARRAY.replacen(
        "return Job::<'b>{bytes:give bytes,items:[first,second]}",
        "let local=3;return Job::<'b>{bytes:give bytes,items:[first,&local]}",
        1,
    );
    for (label, program) in [
        (
            "selected source",
            PACKAGE_ARRAY.replacen("first=11", "second=11", 1),
        ),
        (
            "whole window first source",
            PACKAGE_ARRAY.replacen(
                "let pointer=window.items[1]",
                "first=11;let pointer=window.items[1]",
                1,
            ),
        ),
        (
            "whole window second source",
            PACKAGE_ARRAY.replacen(
                "let pointer=window.items[1]",
                "second=11;let pointer=window.items[1]",
                1,
            ),
        ),
        ("short local element", local_escape),
    ] {
        let report = compile_source(
            &source(label, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("Package Array negative {label}: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
    }
}

fn cleanup_programs() -> [(&'static str, String); 2] {
    let definitions = "extern \"C\" fn putchar(c:i32)->i32;resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}enum Fault{Stopped}package Job<life b>{manages bytes:ByteBuffer;manages mark:Mark;items:[&'b i32;2]}fn make<life b>(first:&'b i32,second:&'b i32)->Job<'b>{var bytes=bytes_new();if bytes_reserve(bytes,1){bytes_push(bytes,65u8)}return Job::<'b>{bytes:give bytes,mark:Mark{code:82},items:[first,second]}}fn identity<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{return input}fn relay<life b>(job:Job<'b>)->Job<'b>{return job}";
    [
        (
            "normal",
            format!(
                "{definitions}fn main()->i32{{var first=7;var second=9;let job=relay(make(&first,&second));let window=identity(&job);let pointer=window.items[1];first=11;return *pointer-9}}"
            ),
        ),
        (
            "error",
            format!(
                "{definitions}fn stop<life b>(first:&'b i32,second:&'b i32)->Fault!i32{{let job=relay(make(first,second));let window=identity(&job);if *window.items[1]!=9{{return 3}}return Fault.Stopped}}fn main()->i32{{var first=7;var second=9;match stop(&first,&second){{.ok(_)=>{{return 1}},.err(_)=>{{}}}}first=11;second=12;return first+second-23}}"
            ),
        ),
    ]
}

#[test]
fn native_package_array_normal_error_and_allocation_failure_cleanup_once() {
    use std::os::unix::fs::PermissionsExt;
    let clang = std::env::var_os("ZIL_CLANG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let shim = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/composed-store-closures/allocator_probe.c");
    let quote = |p: &std::path::Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program) in cleanup_programs() {
                let dir = std::env::temp_dir().join(format!(
                    "zil-package-array-{}-{backend:?}-{optimization:?}-{label}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let wrapper = dir.join("probe-clang.sh");
                std::fs::write(
                    &wrapper,
                    format!(
                        "#!/bin/sh\nexec {} \"$@\" -x c {} -Wl,--wrap=realloc -Wl,--wrap=free\n",
                        quote(&clang),
                        quote(&shim)
                    ),
                )
                .unwrap();
                std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
                let executable = dir.join("program");
                let report = compile_source(
                    &source(label, &program),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        clang: Some(wrapper),
                        output: Some(executable.clone()),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{backend:?}/{optimization:?}/{label}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                for fail_at in [0, 1] {
                    let run = std::process::Command::new(&executable)
                        .env("ZIL_TEST_FAIL_AT", fail_at.to_string())
                        .output()
                        .unwrap();
                    assert_eq!(
                        run.status.code(),
                        Some(0),
                        "{backend:?}/{optimization:?}/{label}/{fail_at}: {run:?}"
                    );
                    assert_eq!(run.stdout, b"R");
                    let stats = String::from_utf8(run.stderr).unwrap();
                    let allocations = usize::from(fail_at == 0);
                    for field in [
                        format!("\"allocated\":{allocations}"),
                        format!("\"frees\":{allocations}"),
                        format!("\"failures\":{fail_at}"),
                        "\"live\":0".into(),
                    ] {
                        assert!(
                            stats.contains(&field),
                            "{backend:?}/{optimization:?}/{label}/{fail_at}: {stats}"
                        );
                    }
                    runs += 1;
                }
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
    assert_eq!(runs, 8 + 8 * usize::from(cfg!(feature = "llvm-api")));
    println!("Package Array normal/error/failure native executions: {runs}");
}

#[test]
fn raw_wire_reject_actual_source_writes_and_same_upper_ordinal_substitution() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let baseline = PACKAGE_ARRAY
        .replacen("var first=7", "var spare=0;var first=7", 1)
        .replacen("first=11", "spare=11", 1);
    for mode in [
        "selected write",
        "whole window first",
        "whole window second",
        "swapped ordinal",
    ] {
        let program = if mode.starts_with("whole window") {
            baseline
                .replacen(
                    "let pointer=window.items[1]",
                    "spare=11;let pointer=window.items[1]",
                    1,
                )
                .replacen("\n    spare=11\n", "\n", 1)
        } else if mode == "swapped ordinal" {
            PACKAGE_ARRAY.to_string()
        } else {
            baseline.clone()
        };
        let mut raw = typed_source(mode, &program, &target, &limits);
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        if mode == "swapped ordinal" {
            let mut changes = 0;
            for i in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let zil_ir::InstructionKind::LoadStoredBorrow { path, .. } = &mut i.kind {
                    assert_eq!(path.as_slice(), &[zil_ir::PlaceProjection::Field(1)]);
                    path[0] = zil_ir::PlaceProjection::Field(0);
                    changes += 1;
                }
            }
            assert_eq!(changes, 1);
        } else {
            let name = if mode == "whole window first" {
                "first"
            } else {
                "second"
            };
            let target_local = main.locals.iter().find(|l| l.name == name).unwrap().id;
            let spare = main.locals.iter().find(|l| l.name == "spare").unwrap().id;
            let store = main
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .filter(
                    |i| matches!(i.kind,zil_ir::InstructionKind::Store{local,..} if local==spare),
                )
                .last()
                .unwrap();
            let zil_ir::InstructionKind::Store { local, .. } = &mut store.kind else {
                unreachable!()
            };
            *local = target_local;
        }
        plan.module_fingerprint = raw.fingerprint();
        let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            analyzed
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{mode}: {analyzed:?}"
        );
        let verified =
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
        assert!(
            verified
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{mode}: {verified:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            decoded
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{mode}: {decoded:?}"
        );
    }
}

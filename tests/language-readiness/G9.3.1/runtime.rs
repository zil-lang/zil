//! Actual general-source literals, exported origins, explicit owning allocation.
use super::{Temp, source};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
use zil_base::{Phase, ResultCategory};
use zil_driver::{
    CompilationSession, CompileOptions, NativeBackend, Optimization, Stage, compile_source,
};

const LITERAL: &str = include_str!("literal.zil");
const OWNED: &str = include_str!("owned.zil");

fn record(report: &zil_driver::CompileReport, label: &str) {
    let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    fs::create_dir_all(&root).unwrap();
    let artifacts = report
        .artifacts
        .iter()
        .filter_map(|(name, path)| {
            let bytes = fs::read(path).ok()?;
            Some(format!(
                "{{\"name\":{},\"sha256\":{},\"bytes\":{}}}",
                zil_base::json_string(name),
                zil_base::json_string(&zil_base::Fingerprint::of(&bytes).to_hex()),
                bytes.len()
            ))
        })
        .collect::<Vec<_>>()
        .join(",");
    fs::write(root.join(format!("{label}.json")),format!("{{\"input_fingerprint\":{},\"status\":{},\"verification_runs\":{},\"artifacts\":[{}]}}\n",zil_base::json_string(&report.input_fingerprint.to_hex()),zil_base::json_string(&format!("{:?}",report.status)),report.verification_runs,artifacts)).unwrap();
}

fn checksum(bytes: &[u8]) -> i64 {
    bytes
        .iter()
        .fold(0, |sum, byte| sum * 17 + i64::from(*byte))
}
fn backends() -> Vec<NativeBackend> {
    let mut result = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        result.push(NativeBackend::LlvmApi)
    }
    result
}
fn allocator_wrapper(dir: &Path) -> std::path::PathBuf {
    let clang = std::env::var_os("ZIL_CLANG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let observer = dir.join("allocator.o");
    let result = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/language/173/allocator_probe.c"),
        )
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let wrapper = dir.join("link.sh");
    fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n", quote(&clang), quote(&observer), quote(&clang))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    wrapper
}

#[test]
fn g931_real_source_native_utf8_exact_bytes_and_explicit_allocation() {
    let temp = Temp::new();
    let wrapper = allocator_wrapper(&temp.0);
    let expected =
        checksum("按钮🙂".as_bytes()) + checksum(b"a\0b") + checksum("e\u{301}".as_bytes());
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text, entry) in [
                ("literal", LITERAL, Some("result".into())),
                ("owned", OWNED, None),
                ("capacity", include_str!("capacity-invalid.zil"), None),
            ] {
                let input = temp
                    .0
                    .join(format!("{name}-{backend:?}-{optimization:?}.zil"));
                fs::write(&input, text).unwrap();
                let report = CompilationSession::default().compile_path(
                    &input,
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry,
                        clang: Some(wrapper.clone()),
                        output: Some(input.with_extension("exe")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                record(&report, &format!("{name}-{backend:?}-{optimization:?}"));
                assert!(report.verification_runs > 0);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let output = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(if name == "capacity" { 1 } else { 0 }),
                    "{output:?}"
                );
                let audit = String::from_utf8(output.stderr).unwrap();
                assert!(audit.contains("\"live\":0"), "{audit}");
                if name == "literal" {
                    assert_eq!(output.stdout, format!("{expected}\n").as_bytes());
                    assert!(audit.contains("\"alloc_calls\":0"), "{audit}");
                } else if name == "capacity" {
                    assert!(output.stdout.is_empty());
                    assert!(audit.contains("\"alloc_calls\":0"), "{audit}");
                } else {
                    assert!(output.stdout.is_empty());
                    for failure in [1, 2] {
                        let failed = Command::new(&report.artifacts["executable"])
                            .env("ZIL_TEST_FAIL_AT", failure.to_string())
                            .output()
                            .unwrap();
                        assert_eq!(failed.status.code(), Some(1), "{failed:?}");
                        assert!(failed.stdout.is_empty());
                        let audit = String::from_utf8(failed.stderr).unwrap();
                        assert!(
                            audit.contains("\"live\":0") && audit.contains("\"failures\":1"),
                            "{audit}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn g931_two_module_static_returns_publish_explicit_verified_origin() {
    let temp = Temp::new();
    let library = temp.0.join("labels.zil");
    let entry = temp.0.join("main.zil");
    fs::write(&library, include_str!("static_library.zil")).unwrap();
    fs::write(&entry,"import labels\nfn result()->i64{let text=labels.forward(labels.title());if labels.empty().len()!=0{return -1}return i64(text[0])*1000+text.len()}").unwrap();
    let mut session = CompilationSession::default();
    for backend in backends() {
        let report = session.compile_path(
            &entry,
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                entry: Some("result".into()),
                output: Some(temp.0.join(format!("two-{backend:?}"))),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        record(&report, &format!("two-module-{backend:?}"));
        assert_eq!(report.source_files.len(), 2);
        let origins = report
            .scalar_interfaces
            .iter()
            .find_map(|i| {
                i.result_origins
                    .as_ref()
                    .filter(|origins| origins.iter().any(|o| o.name == "title"))
            })
            .unwrap();
        for name in ["title", "empty"] {
            let origin = origins.iter().find(|o| o.name == name).unwrap();
            assert!(origin.static_storage);
            assert!(origin.parameters.is_empty());
        }
        let forward = origins.iter().find(|o| o.name == "forward").unwrap();
        assert!(!forward.static_storage);
        assert_eq!(forward.parameters, [0]);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"230010\n");
    }
    // Changing an export body to a local address cannot acquire static origin
    // from its unchanged signature or this warm session's previously published interface.
    fs::write(&library,"pub fn title()->&[u8]{let local=[65u8];return &local[..]}pub fn forward(value:&[u8])->&[u8]{return value}pub fn empty()->&[u8]{return \"\"}").unwrap();
    let denied = session.compile_path(&entry, &CompileOptions::default());
    assert_eq!(
        denied.status,
        ResultCategory::SafeReject,
        "{}",
        denied.to_json()
    );
    assert_eq!(denied.phase, Phase::Verify, "{}", denied.to_json());
    assert!(!denied.artifacts.contains_key("executable"));
}

#[test]
fn g931_mutability_local_escape_and_ct_are_not_granted_by_literal_type() {
    let mutable = compile_source(
        &source(include_str!("mutable-invalid.zil")),
        &CompileOptions::default(),
    );
    assert_eq!(
        mutable.status,
        ResultCategory::SafeReject,
        "{}",
        mutable.to_json()
    );
    assert_eq!(mutable.phase, Phase::Types, "{}", mutable.to_json());
    assert_eq!(mutable.diagnostics[0].code, "E-ADDRESS-PERMISSION");
    let local = compile_source(
        &source(include_str!("local-return-invalid.zil")),
        &CompileOptions::default(),
    );
    assert_eq!(
        local.status,
        ResultCategory::SafeReject,
        "{}",
        local.to_json()
    );
    assert_eq!(local.phase, Phase::Verify, "{}", local.to_json());
    let ct = compile_source(
        &source(include_str!("ct-invalid.zil")),
        &CompileOptions::default(),
    );
    assert_eq!(ct.status, ResultCategory::SafeReject, "{}", ct.to_json());
    assert_eq!(ct.phase, Phase::Ct, "{}", ct.to_json());
    assert_eq!(ct.diagnostics[0].code, "E-CT-FORBIDDEN");
    assert!(!ct.artifacts.contains_key("llvm"));
}

#[test]
fn g931_source_byte_budget_exact_and_fresh_retry() {
    let input = source("fn result()->&[u8]{return \"按钮🙂\"}");
    let boundary = input.text.len();
    for n in [boundary, boundary - 1, boundary] {
        let report = compile_source(
            &input,
            &CompileOptions {
                stop_after: Stage::Types,
                limits: zil_base::Limits {
                    max_source_bytes: n,
                    ..zil_base::Limits::default()
                },
                ..CompileOptions::default()
            },
        );
        let expected = if n == boundary {
            ResultCategory::Success
        } else {
            ResultCategory::ResourceLimit
        };
        assert_eq!(
            report.status,
            expected,
            "N={boundary}, n={n}: {}",
            report.to_json()
        );
    }
    println!("UTF8 actual Source byte budget N={boundary}; N-1 ResourceLimit; fresh N accepted");
}

//! Actual ordinary-source standard library consumers and import trust boundaries.
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, NativeBackend, Optimization, ProjectSources, Stage,
};
const ENTRY: &str = include_str!("utf8.zil");
const UTF8: &str = include_str!("../../../../lib/std/utf8.zil");
const EDIT: &str = include_str!("../../../../lib/std/text_edit.zil");
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!("terminal345-{label}-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}
fn project() -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", ENTRY),
            SourceUnit::new(FileId(1), ModuleId(1), "<std.utf8>", UTF8),
            SourceUnit::new(FileId(2), ModuleId(2), "<std.text_edit>", EDIT),
        ],
        paths: BTreeMap::from([
            (vec!["std".into(), "utf8".into()], ModuleId(1)),
            (vec!["std".into(), "text_edit".into()], ModuleId(2)),
        ]),
    }
}
#[test]
fn ordinary_library_native_and_filesystem_hijack_control() {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let path = directory(&format!("native-{backend:?}-{optimization:?}"));
            fs::write(path.join("main.zil"), ENTRY).unwrap();
            fs::create_dir_all(path.join("std")).unwrap();
            fs::write(path.join("std/utf8.zil"), "not valid ZIL; must never load").unwrap();
            let report = CompilationSession::default().compile_path(
                &path.join("main.zil"),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(path.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.source_files.len(), 3);
            assert_eq!(report.verification_runs, 1);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let out = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                out.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {out:?}"
            );
            assert!(out.stdout.is_empty() && out.stderr.is_empty());
        }
    }
}
#[test]
fn ordinary_library_source_identity_and_private_helpers() {
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let mut p = project();
    let mut session = CompilationSession::default();
    let positive = session.compile_project(&p, &options);
    assert_eq!(
        positive.status,
        ResultCategory::Success,
        "{}",
        positive.to_json()
    );
    p.modules[1]
        .text
        .push_str("\n// even an innocuous changed source is a different reserved library\n");
    let forged = session.compile_project(&p, &options);
    assert_eq!(
        forged.status,
        ResultCategory::SafeReject,
        "{}",
        forged.to_json()
    );
    assert!(
        forged
            .diagnostics
            .iter()
            .any(|d| d.code == "E-STD-RESERVED")
    );
    let mut private = project();
    private.modules[0].text =
        "import std.utf8\nfn main()->i32{let b=[65u8];return i32(utf8.next(&b[..],0))}".into();
    let denied = session.compile_project(&private, &options);
    assert_eq!(
        denied.status,
        ResultCategory::SafeReject,
        "{}",
        denied.to_json()
    );
    assert!(!denied.artifacts.contains_key("executable"));
}
#[test]
fn ordinary_library_source_budget_fresh_and_warm() {
    let path = directory("budget");
    fs::write(path.join("main.zil"), ENTRY).unwrap();
    let mut options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let n = ENTRY.len() + UTF8.len() + EDIT.len();
    options.limits.max_source_bytes = n;
    let mut session = CompilationSession::default();
    for _ in 0..2 {
        let r = session.compile_path(&path.join("main.zil"), &options);
        assert_eq!(r.status, ResultCategory::Success, "N={n}: {}", r.to_json());
        assert_eq!(r.verification_runs, 1);
    }
    options.limits.max_source_bytes = n - 1;
    for mut current in [session, CompilationSession::default()] {
        let r = current.compile_path(&path.join("main.zil"), &options);
        assert_eq!(
            r.status,
            ResultCategory::ResourceLimit,
            "N-1={}: {}",
            n - 1,
            r.to_json()
        );
        assert!(!r.artifacts.contains_key("executable"));
    }
}

#[cfg(unix)]
#[test]
fn ordinary_store_remove_transfers_and_cleans_actual_owners() {
    use std::{os::unix::fs::PermissionsExt, path::Path};
    fn quote(p: &Path) -> String {
        format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let input = include_str!("store.zil");
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let path = directory("store-observer");
    let observer = path.join("observer.o");
    let compiled = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/language/173/allocator_probe.c"),
        )
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let wrapper = path.join("link.sh");
    fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory(&format!("store-{backend:?}-{optimization:?}"));
            fs::write(dir.join("main.zil"), input).unwrap();
            let report = CompilationSession::default().compile_path(
                &dir.join("main.zil"),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper.clone()),
                    output: Some(dir.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            for (failure, exit, stdout) in
                [(0, 0, "ACB"), (1, 91, ""), (2, 1, "ABC"), (3, 0, "ACB")]
            {
                let out = Command::new(&report.artifacts["executable"])
                    .env("ZIL_TEST_FAIL_AT", failure.to_string())
                    .output()
                    .unwrap();
                assert_eq!(
                    out.status.code(),
                    Some(exit),
                    "{backend:?}/{optimization:?}/{failure}: {out:?}"
                );
                assert_eq!(
                    out.stdout,
                    stdout.as_bytes(),
                    "{backend:?}/{optimization:?}/{failure}: {out:?}"
                );
                let audit = String::from_utf8(out.stderr).unwrap();
                assert!(audit.contains("\"live\":0"), "{audit}");
                let failures = if failure == 1 || failure == 2 { 1 } else { 0 };
                assert!(
                    audit.contains(&format!("\"failures\":{failures}")),
                    "{audit}"
                );
            }
        }
    }
    let invalid=input.replace("let removed=store.remove(items,1)?","let held=store_at(items,0)\n    let removed=store.remove(items,1)?\n    if held.id!=0{return 96}");
    fs::write(path.join("invalid.zil"), invalid).unwrap();
    let denied = CompilationSession::default()
        .compile_path(&path.join("invalid.zil"), &CompileOptions::default());
    assert_eq!(
        denied.status,
        ResultCategory::SafeReject,
        "{}",
        denied.to_json()
    );
    assert!(
        matches!(
            denied.phase,
            zil_base::Phase::Analyze | zil_base::Phase::Verify
        ),
        "{}",
        denied.to_json()
    );
}

#[cfg(unix)]
#[test]
fn ordinary_store_prepare_failure_keeps_original_values_and_capacity() {
    use std::{os::unix::fs::PermissionsExt, path::Path};
    fn quote(p: &Path) -> String {
        format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    // Reuse the real owning payload declarations. Ignore the failed union only
    // after forcing the actual suffix allocation to fail, then inspect the input.
    let input = include_str!("store.zil")
        .split("fn main()")
        .next()
        .unwrap()
        .to_owned()
        + r#"
extern "C" fn probe_fail_next();
fn main()->i32 {
    let items:Store<Item>=store.new()
    if !store.reserve(items,4){return 91}
    store.push(items,item(0));store.push(items,item(1));store.push(items,item(2))
    let capacity=store.capacity(items)
    unsafe{probe_fail_next()}
    store.remove(items,1)
    if store.len(items)!=3 || store.capacity(items)!=capacity {return 92}
    if store_at(items,0).id!=0 || store_at(items,1).id!=1 || store_at(items,2).id!=2 {return 93}
    store.remove(items,-1)
    store.remove(items,3)
    if store.len(items)!=3 || store.capacity(items)!=capacity {return 94}
    return 0
}
"#;
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let path = directory("store-unchanged");
    let observer = path.join("observer.o");
    let compiled = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/language/173/allocator_probe.c"),
        )
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let wrapper = path.join("link.sh");
    fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory(&format!("store-unchanged-{backend:?}-{optimization:?}"));
            fs::write(dir.join("main.zil"), &input).unwrap();
            let report = CompilationSession::default().compile_path(
                &dir.join("main.zil"),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper.clone()),
                    output: Some(dir.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let out = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(0), "{out:?}");
            assert_eq!(out.stdout, b"ABC");
            let audit = String::from_utf8(out.stderr).unwrap();
            assert!(
                audit.contains("\"alloc_calls\":2")
                    && audit.contains("\"failures\":1")
                    && audit.contains("\"live\":0"),
                "{audit}"
            );
        }
    }
}

#[test]
fn ordinary_iterators_keep_callback_order_short_circuit_and_option_results() {
    let input = include_str!("iterator.zil");
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory(&format!("iterator-{backend:?}-{optimization:?}"));
            fs::write(dir.join("main.zil"), input).unwrap();
            let report = CompilationSession::default().compile_path(
                &dir.join("main.zil"),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(dir.join("program")),
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
            let out = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                out.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {out:?}"
            );
            assert!(out.stdout.is_empty() && out.stderr.is_empty());
        }
    }
    for (label, text) in [
        (
            "parent-write",
            include_str!("iterator_capture_reset_invalid.zil"),
        ),
        (
            "alias",
            "import std.iter\nfn main()->i32{var values=[1,2];var callback=(value:i32)=>{values[0]+=value};iter.for_each(&values[..],&mut callback);return values[0]}",
        ),
    ] {
        let dir = directory(&format!("iterator-invalid-{label}"));
        fs::write(dir.join("main.zil"), text).unwrap();
        let denied = CompilationSession::default()
            .compile_path(&dir.join("main.zil"), &CompileOptions::default());
        assert_eq!(
            denied.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            denied.to_json()
        );
        assert!(
            matches!(
                denied.phase,
                zil_base::Phase::Analyze | zil_base::Phase::Verify
            ),
            "{label}: {}",
            denied.to_json()
        );
    }
}

//! Black-box Source consumers for ordinary mutable POD Store elements.
use super::*;

fn native_all_backends(label: &str, source: &str, expected: &[u8], imported: bool) {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory(&format!("g933-{label}-{backend:?}-{optimization:?}"));
            let input = dir.join("main.zil");
            fs::write(&input, source).unwrap();
            let exe = dir.join("program");
            let options = CompileOptions {
                backend,
                stop_after: Stage::Build,
                optimization,
                output: Some(exe.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            };
            let report = if imported {
                zil_driver::compile_path(&input, &options)
            } else {
                let unit = SourceUnit::new(FileId(0), ModuleId(0), "g933.zil", source);
                compile_source(&unit, &options)
            };
            fs::write(dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert!(report.verification_runs > 0, "{}", report.to_json());
            if imported {
                assert!(report.source_files.len() >= 2, "{}", report.to_json());
            }
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none(), "{}", report.to_json());
            let output = Command::new(exe).output().unwrap();
            fs::write(dir.join("stdout"), &output.stdout).unwrap();
            fs::write(dir.join("stderr"), &output.stderr).unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(output.stdout, expected, "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

#[test]
fn g933_direct_write_helper_reborrow_last_use_and_distinct_root() {
    native_all_backends("direct", include_str!("direct.zil"), b"MUT\n", false);
}

#[test]
fn g933_std_store_helpers_resolve_across_real_module_boundary() {
    native_all_backends("std-store", include_str!("std-store.zil"), b"STDM\n", true);
}

#[test]
fn g933_live_same_root_windows_and_structural_changes_are_rejected() {
    const PREFIX: &str = "struct Cell{value:i32}fn consume(give items:Store<Cell>){}fn destroy(give items:Store<Cell>){}fn main()->i32{let items:Store<Cell>=store_new();store_reserve(items,3);store_push(items,Cell{value:41});store_push(items,Cell{value:42});";
    // Both shared and exclusive views have a legal isolated baseline. The
    // return reads the original view, so each inserted operation is live.
    for (label, view, change) in [
        (
            "shared-other-index",
            "store_at_mut(items,0)",
            "let other=store_at(items,1);let n=other.value;",
        ),
        (
            "mut-other-index",
            "store_at_mut(items,0)",
            "let other=store_at_mut(items,1);let n=other.value;",
        ),
        (
            "mut-after-shared",
            "store_at(items,0)",
            "let other=store_at_mut(items,1);let n=other.value;",
        ),
        (
            "reserve",
            "store_at_mut(items,0)",
            "store_reserve(items,4);",
        ),
        (
            "push",
            "store_at_mut(items,0)",
            "store_push(items,Cell{value:43});",
        ),
        ("pop", "store_at_mut(items,0)", "store_pop(items);"),
        ("give", "store_at_mut(items,0)", "consume(give items);"),
        ("drop", "store_at_mut(items,0)", "destroy(give items);"),
    ] {
        let baseline = format!("{PREFIX}let view={view};return view.value}}");
        let accepted = check(&format!("g933-{label}-baseline"), &baseline, Stage::Verify);
        assert_eq!(
            accepted.status,
            ResultCategory::Success,
            "{}",
            accepted.to_json()
        );
        assert!(accepted.verification_runs > 0);
        let source = format!("{PREFIX}let view={view};{change}return view.value}}");
        let report = check(&format!("g933-{label}"), &source, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-BORROW-CONFLICT"),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

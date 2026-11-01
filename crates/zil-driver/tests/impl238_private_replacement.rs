//! Whole opaque Product replacement keeps source field privacy.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::ResultCategory;
use zil_driver::{CompileOptions, Stage, compile_path};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const LIB: &str = r#"
resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){}}
pub package Owner{manages marker:Mark;tag:i32}
pub fn make()->Owner{return Owner{marker:Mark{code:65},tag:1}}
pub fn relay(value:Owner,n:i32)->Owner{return value}
pub fn number(value:Owner)->i32{return value.tag}
pub fn relay_union(value:void!Owner)->Owner{return make()}
"#;
fn project(source: &str) -> zil_driver::CompileReport {
    let path = std::env::temp_dir().join(format!(
        "privacy238-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("lib.zil"), LIB).unwrap();
    fs::write(path.join("main.zil"), format!("import lib\n{source}")).unwrap();
    let report = compile_path(
        &path.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..Default::default()
        },
    );
    fs::remove_dir_all(path).unwrap();
    report
}
#[test]
fn opaque_independent_and_self_replacement_preserve_private_fields() {
    for body in [
        "value=lib.make();",
        "value=lib.relay_union(value);",
        "value=lib.relay(value,7);",
        "value=lib.relay(value,(()=>{let value=3;return value})());",
        "if true{value=lib.relay(value,7);}value=lib.relay(value,9);",
        "value=lib.relay(value,7);let later=lib.make();value=lib.make();",
    ] {
        let report = project(&format!(
            "fn main()->i32{{var value=lib.make();{body}return lib.number(value)}}"
        ));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{body}: {}",
            report.to_json()
        );
    }
}
#[test]
fn source_private_accesses_are_never_internal_transaction_operations() {
    for body in [
        "let other=lib.make();value=lib.relay(value,other.tag);",
        "value=lib.relay(value,7);let other=lib.make();let observed=other.tag;",
        "value=lib.make();var other=lib.make();other.tag=2;",
        "value=lib.Owner{marker:Mark{code:66},tag:2};",
    ] {
        let report = project(&format!(
            "fn main()->i32{{var value=lib.make();{body}return lib.number(value)}}"
        ));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{body}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-FIELD-PRIVATE"),
            "{body}: {}",
            report.to_json()
        );
    }
}
#[test]
fn original_cross_module_projects_reach_analyze_unchanged() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline");
    for name in ["package_ui", "helper_ui"] {
        let report = compile_path(
            &root.join(name).join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..Default::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
    }
}

#[test]
fn opaque_replacement_with_later_temporaries_runs_on_both_native_levels() {
    use zil_driver::{NativeBackend, Optimization};
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = std::env::temp_dir().join(format!(
            "privacy238-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        let lib = format!(
            "extern \"C\" fn putchar(n:i32)->i32;{}",
            LIB.replace(
                "fn drop(self:Self){}",
                "fn drop(self:Self){unsafe{putchar(.code)}}"
            )
        );
        fs::write(path.join("lib.zil"), lib).unwrap();
        fs::write(path.join("main.zil"), "import lib;fn main()->i32{var value=lib.make();value=lib.make();if true{value=lib.relay(value,7);}let later=lib.make();value=lib.relay(value,9);value=lib.make();value=lib.relay_union(value);return lib.number(value)-1}").unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_path(
            &path.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(path.join("program")),
                ..Default::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let output = std::process::Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"AAAAA", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        fs::remove_dir_all(path).unwrap();
    }
}

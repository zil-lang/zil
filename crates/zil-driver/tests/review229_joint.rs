//! Minimal non-author composition check on the frozen 228/229 candidate.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review229-joint.zil", text)
}
fn output(label: &str) -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review229-joint-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn generated_types_do_not_grant_where_binders_members_or_numeric_rights() {
    for (label, text, accepted_by_231) in [
        (
            "alias-numeric",
            "#for(n in [1]){type T=i32;fn bad<T>(x:T)->T where T:Copy=>x+x;}fn main()->i32=>0",
            false,
        ),
        (
            "nominal-field",
            "#for(n in [1]){struct T{x:i32}fn bad<T>(x:T)->i32 where T:Copy=>x.x;}fn main()->i32=>0",
            false,
        ),
        (
            // Keep the original 229 input bytes and label. The frozen 231
            // profile admits this declared ordinary nominal trait obligation;
            // the function body only returns T and gains no extra operations.
            "invalid-bound",
            "trait Marker{}#for(n in [1]){type Count=i32;fn bad<T>(x:T)->T where T:Copy+Marker=>x;}fn main()->i32=>0",
            true,
        ),
        (
            "missing-bound",
            "trait Marker{}#for(n in [1]){type Count=i32;fn bad<T>(x:T)->T where T:Copy+Missing=>x;}fn main()->i32=>0",
            false,
        ),
        (
            "generic-bound",
            "trait Marker<U>{}#for(n in [1]){type Count=i32;fn bad<T>(x:T)->T where T:Copy+Marker=>x;}fn main()->i32=>0",
            false,
        ),
    ] {
        let p = output(label);
        fs::write(p.join("source.zil"), text).unwrap();
        let report = CompilationSession::default().compile_source(
            &source(text),
            &CompileOptions {
                emit_dir: Some(p.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(p.join("report.json"), report.to_json()).unwrap();
        if accepted_by_231 {
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{label}: {}",
                report.to_json()
            );
            assert_eq!(report.phase, Phase::Verify, "{label}: {}", report.to_json());
            assert_eq!(report.verification_runs, 1);
            continue;
        }
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
}

#[cfg(feature = "llvm-api")]
#[test]
fn generated_array_alias_and_shadowing_where_helper_execute_at_o0_and_o2() {
    use std::process::Command;
    use zil_driver::{NativeBackend, Optimization, Stage};
    let text = "#for(n in [2]){type T=[i32;n];struct Row{values:T}fn duplicate<T>(x:T)->(T,T) where T:Copy=>(x,x);pub fn run()->i32{let pair=duplicate(Row{values:[2,3]});return pair.0.values[0]+pair.1.values[1]-5}}fn main()->i32=>run()";
    for optimization in [Optimization::O0, Optimization::O2] {
        let p = output(&format!("native-{optimization:?}"));
        fs::write(p.join("source.zil"), text).unwrap();
        let report = CompilationSession::default().compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Build,
                backend: NativeBackend::LlvmApi,
                optimization,
                output: Some(p.join("program")),
                emit_dir: Some(p.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(p.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(p.join("stdout.bin"), &result.stdout).unwrap();
        fs::write(p.join("stderr.bin"), &result.stderr).unwrap();
        fs::write(p.join("exit.txt"), format!("{:?}", result.status.code())).unwrap();
        assert_eq!(result.status.code(), Some(0));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

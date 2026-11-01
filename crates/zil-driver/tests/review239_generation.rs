//! Independent generated/handwritten ownership and nominal controls.
use std::{fs, path::PathBuf, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

fn directory(label: &str) -> PathBuf {
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "review239-generated-{label}-{}",
            std::process::id()
        ));
    fs::create_dir_all(&out).unwrap();
    out
}
const RESOURCE: &str = "extern \"C\" fn putchar(c:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code);}}}";
const BODY: &str =
    "let e=E.Pair(R{code:65},R{code:66});match e{E.Pair(a,b)=>{}\nE.Empty()=>{}}return 0";

#[test]
fn generated_manages_and_handwritten_fields_execute_the_same_declared_order() {
    // Predeclared from the two source fields: Pair(a,b) drops a then b => AB,
    // exit 0, no explicit allocation. This assertion precedes machine execution.
    for (label, source) in [
        (
            "handwritten",
            format!("{RESOURCE}enum E{{Empty,Pair(manages R,manages R)}}fn main()->i32{{{BODY}}}"),
        ),
        (
            "generated",
            format!(
                "{RESOURCE}#for(n in [1]){{enum E{{Empty,Pair(manages R,manages R)}}pub fn run()->i32{{{BODY}}}}}fn main()->i32=>run()"
            ),
        ),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory(&format!("{label}-{optimization:?}"));
            fs::write(out.join("source.zil"), &source).unwrap();
            fs::write(out.join("expected.txt"), "stdout=AB\nexit=0\n").unwrap();
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "generated239.zil", source.as_str()),
                &CompileOptions {
                    stop_after: Stage::Build,
                    optimization,
                    backend: if cfg!(feature = "llvm-api") {
                        NativeBackend::LlvmApi
                    } else {
                        NativeBackend::Clang
                    },
                    output: Some(out.join("program")),
                    emit_dir: Some(out.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(out.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let native = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(out.join("stdout"), &native.stdout).unwrap();
            fs::write(out.join("stderr"), &native.stderr).unwrap();
            fs::write(out.join("exit.txt"), format!("{:?}", native.status.code())).unwrap();
            assert_eq!(native.status.code(), Some(0));
            assert_eq!(native.stdout, b"AB");
            assert!(native.stderr.is_empty());
        }
    }
}

#[test]
fn generation_preserves_private_iteration_identity_and_closed_payload_profiles() {
    for (label, source) in [
        (
            "cross-iteration",
            "resource R{n:i32}#for(n in [1,2]){enum E{V(manages R)}#if(n==1){pub fn make()->E{return E.V(R{n:1})}}else{pub fn consume(e:E){}}}fn main(){consume(make())}",
        ),
        (
            "private-escape",
            "resource R{n:i32}#for(n in [1]){enum E{V(manages R)}}fn main(){let e=E.V(R{n:1})}",
        ),
        (
            "bare-buffer",
            "#for(n in [1]){enum E{V(ByteBuffer)}}fn main(){}",
        ),
        (
            "bare-resource",
            "resource R{n:i32}#for(n in [1]){enum E{V(R)}}fn main(){}",
        ),
        (
            "owned-generic",
            "resource R{n:i32}#for(n in [1]){enum E<T>{V(manages R,T)}}fn main(){}",
        ),
    ] {
        let out = directory(label);
        fs::write(out.join("source.zil"), source).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "generated239-reject.zil", source),
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| !d.code.starts_with("E-PARSE")),
            "{label}: {}",
            report.to_json()
        );
    }
}

//! Frozen before the final finite-callable/ordinary-Sum argument repair.
//! This adds ordinary concrete arguments, never callable signatures or State.
use std::{fs, path::PathBuf, process::Command};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const MODEL: &str = "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}enum E{Empty,Pair(manages R,manages R)}package Wrap{state:E;ready:bool}fn sink(give owner:R){}fn make()->E{return E.Pair(R{n:65},R{n:66})}fn fallible()->void!Wrap{return Wrap{state:make(),ready:true}}";
fn directory(label: &str) -> PathBuf {
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review240-helper-{label}-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    out
}
#[test]
fn finite_fn_helper_accepts_complete_enum_product_and_error_arguments() {
    // make supplies A,B. Explicit give to sink or ordinary parameter cleanup
    // closes them in source declaration order. Callback returns 7; main exits 0.
    for (label, helper, argument) in [
        (
            "enum",
            "fn use_value<F:Fn()->i32>(callback:F,value:E)->i32{let n=callback();match value{E.Empty()=>{return n}\nE.Pair(a,b)=>{sink(give a);sink(give b);return n}}}",
            "make()",
        ),
        (
            "product",
            "fn use_value<F:Fn()->i32>(callback:F,value:Wrap)->i32{let n=callback();let e=value.state;match e{E.Empty()=>{return n}\nE.Pair(a,b)=>{sink(give a);sink(give b);return n}}}",
            "Wrap{state:make(),ready:true}",
        ),
        (
            "error",
            "fn use_value<F:Fn()->i32>(callback:F,value:void!Wrap)->i32{return callback()}",
            "fallible()",
        ),
        (
            "enum-give-relay",
            "fn use_value<F:Fn()->i32>(callback:F,value:E)->i32{let n=callback();match value{E.Empty()=>{return n}\nE.Pair(a,b)=>{let first=give a;let second=give b;sink(give first);sink(give second);return n}}}",
            "make()",
        ),
        (
            "enum-whole-move-once",
            "fn use_value<F:Fn()->i32>(callback:F,value:E)->i32{let next=value;let n=callback();match next{E.Empty()=>{return n}\nE.Pair(a,b)=>{sink(give a);sink(give b);return n}}}",
            "make()",
        ),
        (
            "enum-resource-named-F",
            "fn use_value<F:Fn()->i32>(callback:F,value:E)->i32{let n=callback();match value{E.Empty()=>{return n}\nE.Pair(a,b)=>{sink(give a);sink(give b);return n}}}",
            "make()",
        ),
        (
            "enum-resource-named-F-description",
            "fn use_value<F:Fn()->i32>(callback:F,value:E)->i32{let n=callback();match value{E.Empty()=>{return n}\nE.Pair(a,b)=>{let description=a;let code=description.n;sink(give a);sink(give b);return n+code-65}}}",
            "make()",
        ),
    ] {
        let text = format!("{MODEL}{helper}fn main()->i32{{return use_value(()=>7,{argument})-7}}");
        let text = if label.starts_with("enum-resource-named-F") {
            // The Enum field's concrete global resource F and helper binder F
            // are distinct lexical identities despite their equal spelling.
            text.replace("resource R", "resource F")
                .replace("for R", "for F")
                .replace("manages R", "manages F")
                .replace("owner:R", "owner:F")
                .replace("R{", "F{")
        } else {
            text
        };
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory(&format!("{label}-{optimization:?}"));
            fs::write(out.join("source.zil"), &text).unwrap();
            fs::write(out.join("expected.txt"), "stdout=AB\nexit=0\n").unwrap();
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "helper240.zil", text.as_str()),
                &CompileOptions {
                    stop_after: Stage::Build,
                    optimization,
                    backend: if cfg!(feature = "llvm-api") {
                        NativeBackend::LlvmApi
                    } else {
                        NativeBackend::Clang
                    },
                    emit_dir: Some(out.join("phases")),
                    output: Some(out.join("program")),
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
            let result = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(out.join("stdout"), &result.stdout).unwrap();
            fs::write(out.join("stderr"), &result.stderr).unwrap();
            fs::write(out.join("exit.txt"), format!("{:?}", result.status.code())).unwrap();
            assert_eq!(result.status.code(), Some(0));
            assert_eq!(result.stdout, b"AB");
            assert!(result.stderr.is_empty());
        }
    }
}
#[test]
fn ordinary_sum_argument_does_not_expand_callable_or_state_permissions() {
    for (label, helper) in [
        (
            "repeat-managed",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{match e{E.Empty()=>{return f()}\nE.Pair(a,b)=>{sink(give a);sink(give a);return f()}}}",
        ),
        (
            "desc-is-not-owner",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{match e{E.Empty()=>{return f()}\nE.Pair(a,b)=>{let description=a;sink(give description);return f()}}}",
        ),
        (
            "callable-argument",
            "fn bad<F:Fn(E)->i32>(f:F,e:E)->i32{return f(e)}",
        ),
        (
            "callable-result",
            "fn bad<F:Fn()->E>(f:F,e:E)->i32{let other=f();return 0}",
        ),
        ("return-F", "fn bad<F:Fn()->i32>(f:F,e:E)->F{return f}"),
        (
            "state-constructor",
            "package State<F>{callback:F}fn bad<F:Fn()->i32>(f:F,e:E)->State<F>{return State::<F>{callback:f}}",
        ),
        (
            "once-twice",
            "fn bad<F:FnOnce()->i32>(f:F,e:E)->i32{let first=f();return first+f()}",
        ),
        (
            "unused-enum-rematch",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{match e{E.Empty()=>{}\nE.Pair(a,b)=>{}}match e{E.Empty()=>{}\nE.Pair(a,b)=>{}}return f()}",
        ),
        (
            "unused-product-copy",
            "fn bad<F:Fn()->i32>(f:F,value:Wrap)->i32{let saved=value;let again=value;return f()}",
        ),
        (
            "unused-union-copy",
            "fn bad<F:Fn()->i32>(f:F,value:void!E)->i32{let saved=value;let again=value;return f()}",
        ),
        (
            "scope-F-callback-field",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{return f.n}",
        ),
        (
            "scope-F-callback-give",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{sink(give f);return 0}",
        ),
        (
            "scope-F-description-give",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{match e{E.Empty()=>{return f()}\nE.Pair(a,b)=>{let description=a;sink(give description);return f()}}}",
        ),
        (
            "scope-F-stale-desc-name",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{match e{E.Empty()=>{return f()}\nE.Pair(a,b)=>{let description=a;sink(give a);return description.n}}}",
        ),
        (
            "scope-F-stale-desc-group",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{match e{E.Empty()=>{return f()}\nE.Pair(a,b)=>{let description=(a);sink(give a);return (description).n}}}",
        ),
        (
            "scope-F-stale-desc-chain",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{match e{E.Empty()=>{return f()}\nE.Pair(a,b)=>{let first=a;let description=first;sink(give a);return description.n}}}",
        ),
        (
            "scope-F-stale-desc-if",
            "fn bad<F:Fn()->i32>(f:F,e:E)->i32{match e{E.Empty()=>{return f()}\nE.Pair(a,b)=>{let first=a;let second=b;let description=if f()==0{(first)}else{second};sink(give a);return description.n}}}",
        ),
    ] {
        let text = format!("{MODEL}{helper}fn main(){{}}");
        let text = if label.starts_with("scope-F-") {
            text.replace("resource R", "resource F")
                .replace("for R", "for F")
                .replace("manages R", "manages F")
                .replace("owner:R", "owner:F")
                .replace("R{", "F{")
        } else {
            text
        };
        let out = directory(label);
        fs::write(out.join("source.zil"), &text).unwrap();
        let report = compile_source(
            &SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "helper240-reject.zil",
                text.as_str(),
            ),
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
        assert!(
            report.diagnostics.iter().all(|d| d.phase != Phase::Parse),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

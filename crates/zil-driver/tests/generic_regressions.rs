use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};
fn source(s: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review.zil", s)
}
fn expand(s: &str, limits: &Limits) -> zil_base::CompileResult<zil_expand::ExpandedModule> {
    let src = source(s);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    zil_expand::expand_with_map_limits(&src, &ast, limits)
}
#[test]
fn generic_record_default_cannot_gain_numeric_constraint_from_instance() {
    let r = expand(
        "struct Box<T> { value:T=3 } fn result()->i32 {let x=Box::<i32>{};return x.value}",
        &Limits::default(),
    );
    assert!(
        r.is_err(),
        "unconstrained T was given an integer default by concrete specialization"
    );
}
#[test]
fn duplicate_generic_and_ordinary_function_must_reject() {
    let r = compile_source(
        &source("fn id<T>(x:T)->T=>x\nfn id(x:i32)->i32=>x+1\nfn result()->i32=>id(2)"),
        &CompileOptions::default(),
    );
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
}
#[test]
fn unused_generic_body_respects_selected_depth() {
    let r = expand(
        "fn unused<T>(x:T)->T=>((((((((x))))))))\nfn result()->i32=>1",
        &Limits {
            max_call_depth: 2,
            ..Limits::default()
        },
    );
    let errors = r.unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}
#[test]
fn unused_generic_body_respects_selected_work() {
    let r = expand(
        "fn unused<T>(x:T)->i32=>1+2+3+4+5+6+7+8+9+10+11+12\nfn result()->i32=>1",
        &Limits {
            max_nodes: 5,
            ..Limits::default()
        },
    );
    let errors = r.unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}
#[test]
fn unused_generic_concrete_bad_operation_is_not_discarded() {
    let r = compile_source(
        &source("fn bad<T>(x:T)->bool=>true+true\nfn result()->i32=>1"),
        &CompileOptions::default(),
    );
    assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
}

#[test]
fn substituted_type_depth_is_reserved_before_instance_cloning() {
    let result = format!("{}T{}", "(".repeat(13), ")".repeat(13));
    let argument = format!("{}i32{}", "(".repeat(6), ")".repeat(6));
    let text = format!("fn id<T>(x:T)->{result}=>x\nfn result()->i32=>id::<{argument}>(1)");
    assert!(expand(&text, &Limits::default()).is_ok());
    let errors = expand(
        &text,
        &Limits {
            max_call_depth: 16,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}
#[test]
fn selected_analysis_work_and_unused_signature_lifetimes_are_checked() {
    let errors = expand(
        "fn id<T>(x:T)->T=>x\nfn result()->i32=>1",
        &Limits {
            max_analysis_steps: 8,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    let errors = expand(
        "fn unused<T>(x:&'a T)->i32=>1\nfn result()->i32=>1",
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::Unsupported);
}
#[test]
fn concrete_unary_and_binary_errors_remain_errors_when_unused() {
    for text in [
        "fn bad<T>(x:T)->bool=>-true\nfn result()->i32=>1",
        "fn bad<T>(x:T)->i32=>!3\nfn result()->i32=>1",
        "fn bad<T>(x:T)->bool=>true<false\nfn result()->i32=>1",
        "fn bad<T>(x:T)->i32=>3&&4\nfn result()->i32=>1",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn unused_generic_control_flow_is_checked_before_template_removal() {
    for text in [
        "fn unused<T>(x:T)->i32 {} fn result()->i32=>0",
        "fn unused<T>(x:T)->i32 {return 1;true+true} fn result()->i32=>0",
        "fn unused<T>(x:T,b:bool)->T {if b{return x}} fn result()->i32=>0",
        "fn unused<T>(x:T,b:bool)->T {if b{return x}else{return x} x} fn result()->i32=>0",
        "fn unused<T>(x:T,x:i32)->i32=>x; fn result()->i32=>0",
    ] {
        let r = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.phase, zil_base::Phase::Expand, "{}", r.to_json());
    }
}

#[test]
fn generic_injection_295_frozen_pending_sources_are_accepted() {
    let mut failures = Vec::new();
    for text in [
        include_str!("../../../tests/generic-error-enum/pending/unique-binding-value.zil"),
        include_str!("../../../tests/generic-error-enum/pending/unique-call-argument.zil"),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        if report.status != ResultCategory::Success || report.verification_runs != 1 {
            failures.push(report.to_json());
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn generic_injection_295_native_contexts_preserve_value_and_order() {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for (name, text, status, output) in [
        (
            "binding-error",
            include_str!("../../../tests/generic-error-enum/native-context/binding-error.zil"),
            1,
            "ABB",
        ),
        (
            "binding-success",
            include_str!("../../../tests/generic-error-enum/native-context/binding-success.zil"),
            0,
            "ABBD7",
        ),
        (
            "binding-cleanup-error",
            include_str!(
                "../../../tests/generic-error-enum/native-context/binding-cleanup-error.zil"
            ),
            1,
            "ABBE",
        ),
        (
            "binding-cleanup-success",
            include_str!(
                "../../../tests/generic-error-enum/native-context/binding-cleanup-success.zil"
            ),
            0,
            "ABBD7E",
        ),
        (
            "call-error",
            include_str!("../../../tests/generic-error-enum/native-context/call-error.zil"),
            1,
            "ABC",
        ),
        (
            "call-success",
            include_str!("../../../tests/generic-error-enum/native-context/call-success.zil"),
            0,
            "ABCD7",
        ),
        (
            "call-payload-explicit",
            include_str!(
                "../../../tests/generic-error-enum/native-context/call-payload-explicit.zil"
            ),
            1,
            "ABC",
        ),
        (
            "call-payload-error",
            include_str!("../../../tests/generic-error-enum/native-context/call-payload-error.zil"),
            1,
            "ABC",
        ),
        (
            "lookahead-forward",
            include_str!("../../../tests/generic-error-enum/native-context/lookahead-forward.zil"),
            0,
            "BC",
        ),
        (
            "lookahead-reverse",
            include_str!("../../../tests/generic-error-enum/native-context/lookahead-reverse.zil"),
            0,
            "CB",
        ),
    ] {
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-295-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend: *backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1, "{name}");
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, *backend);
                assert!(codegen.fallback.is_none());
                let actual = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(status), "{name}: {actual:?}");
                assert_eq!(actual.stdout, output.as_bytes(), "{name}: {actual:?}");
                assert!(actual.stderr.is_empty(), "{name}: {actual:?}");
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn generic_injection_295_context_identity_and_unused_controls() {
    let positives = [
        "enum Choice<T>{None,Some(T)} type Problem=Choice<i32>;fn run<F:Fn()->i32>(f:F)->Problem!i32{let v:Problem!i32=(((Choice.None)));return v}fn main(){let f=()->i32=>1;run(f)}",
        "enum Choice<T>{None,Some(T)} fn keep(v:Choice<i32>!i32)->Choice<i32>!i32{return v}fn run<F:Fn()->i32>(f:F,v:Choice<i32>!i32)->Choice<i32>!i32{return keep(v)}fn main(){let f=()->i32=>1;run(f,7)}",
        "enum Box<T>{None,Some(T)} fn sink(v:Box<i32>!Box<i64>)->i32{return 0} fn run<F:Fn()->i32>(f:F)->i32{return sink(Box::<i64>.None)} fn main(){let f=()->i32=>0;run(f)}",
    ];
    for text in positives {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn generic_injection_295_ambiguous_and_wrong_nominal_controls_reject() {
    let negatives = [
        "enum Fault{Bad} fn run<F:Fn()->i32>(f:F)->i32{let x:Fault!Fault=Fault.Bad;return 0}fn main(){}",
        "enum Fault{Bad} enum Other{Bad} fn run<F:Fn()->i32>(f:F)->i32{let x:Fault!i32=Other.Bad;return 0}fn main(){}",
        "enum Box<T>{None,Some(T)} fn sink(v:Box<i32>!Box<i64>)->i32{return 0} fn run<F:Fn()->i32>(f:F)->i32{return sink(Box.None)} fn main(){}",
        "enum Box<T>{None,Some(T)} fn sink(v:Box<u8>!Box<i64>)->i32{return 0} fn run<F:Fn()->i32>(f:F)->i32{return sink(Box.Some(7))} fn main(){}",
        "enum Box<T>{None,Some(T)} fn require<T>(x:T)->i32{return 0} fn run<F:Fn()->i32>(f:F)->i32{return require(Box.None)} fn main(){}",
        "enum Choice<T>{None,Some(T)} fn sink(v:Choice<i32>!i32)->i32{return 0} fn run<F:Fn()->i64>(f:F)->i32{return sink(Choice.Some(f()))} fn main(){}",
    ];
    for text in negatives {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Expand,
            "{text}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.code == "E-GENERIC-ARGUMENT"),
            "{text}: {}",
            report.to_json()
        );
    }
}

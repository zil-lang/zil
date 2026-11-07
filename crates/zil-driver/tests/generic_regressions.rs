use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};
#[path = "../../../tests/language-readiness/terminal/342/loans.rs"]
mod owned_error_loans;
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
    // Undeclared life is now diagnosed precisely; keep the original source.
    assert_eq!(errors[0].category, ResultCategory::SafeReject);
    assert_eq!(errors[0].phase, zil_base::Phase::Expand);
    assert_eq!(errors[0].code, "E-LIFE-DECLARATION");
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
fn generic_context_injection_frozen_pending_sources_are_accepted() {
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
fn generic_context_injection_native_contexts_preserve_value_and_order() {
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
fn generic_context_injection_context_identity_and_unused_controls() {
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
fn generic_context_injection_ambiguous_and_wrong_nominal_controls_reject() {
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

#[cfg(target_os = "linux")]
fn replacement_recovery_project(entry: &str) -> zil_driver::ProjectSources {
    use std::{collections::BTreeMap, fs, path::PathBuf};
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/recovery317");
    let names = [entry, "config", "component", "bridge"];
    let mut paths = BTreeMap::new();
    let modules = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let index = index as u32;
            let path = root.join(format!("{name}.zil"));
            if index != 0 {
                paths.insert(vec![(*name).into()], ModuleId(index));
            }
            SourceUnit::new(
                FileId(index),
                ModuleId(index),
                path.to_string_lossy().into_owned(),
                fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    zil_driver::ProjectSources {
        root: ModuleId(0),
        modules,
        paths,
    }
}

#[test]
fn ordinary_package_address_call_inside_closure_keeps_contextual_signature() {
    let text = "resource R{n:i32}package P{manages r:R;n:i32}fn bump(p:&mut P,n:i32){p.n+=n}fn result()->i32{let f=()->i32=>{var p=P{r:R{n:1},n:20};bump(&mut p,22);return p.n};return f()}";
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let denied = text.replace("bump(&mut p,22)", "bump(&p,22)");
    let report = compile_source(&source(&denied), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn replacement_recovery_component_source_and_nearest_rejections() {
    let report = zil_driver::compile_project(
        &replacement_recovery_project("main"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for entry in ["controls/shared_window_end", "controls/callback_alias_end"] {
        let report = zil_driver::compile_project(
            &replacement_recovery_project(entry),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{entry}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    for (entry, phase, code, text, verification_runs) in [
        (
            "shared_window",
            zil_base::Phase::Verify,
            "E-BORROW-CONFLICT",
            "panel",
            1,
        ),
        (
            "callback_alias",
            zil_base::Phase::Analyze,
            "E-BORROW-CONFLICT",
            "bridge.run(&mut handler,body)",
            0,
        ),
        (
            "nominal_config",
            zil_base::Phase::Ct,
            "E-CT-CATEGORY",
            "Other::<i32>{value:1}",
            0,
        ),
        (
            "phantom_config",
            zil_base::Phase::Ct,
            "E-CT-CATEGORY",
            "config.Config::<i32,i64>{value:1}",
            0,
        ),
    ] {
        let input = replacement_recovery_project(&format!("negative/{entry}"));
        let report = zil_driver::compile_project(&input, &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{entry}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{entry}: {}", report.to_json());
        assert_eq!(
            report.verification_runs,
            verification_runs,
            "{entry}: {}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.phase == phase
                    && diagnostic.code == code
                    && diagnostic.span.is_some_and(|span| {
                        span.file == FileId(0)
                            && input.modules[0]
                                .text
                                .get(span.start..span.end)
                                .is_some_and(|source| source.contains(text))
                    })
            }),
            "{entry}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
#[cfg(target_os = "linux")]
fn replacement_recovery_component_native_success_error_and_null_cleanup() {
    use std::{
        fs,
        io::Write,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/recovery317");
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
            let directory = retained
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(format!(
                    "recovery317-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir(&directory).unwrap();
            let host = directory.join("host.o");
            let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                .arg(fixture.join("host.c"))
                .arg("-o")
                .arg(&host)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            let wrapper = directory.join("link.sh");
            fs::write(&wrapper, format!(
                "#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {}\nfi\nexec {} \"$@\"\n",
                quote(&clang), quote(&host), quote(&clang)
            )).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = zil_driver::compile_project(
                &replacement_recovery_project("main"),
                &CompileOptions {
                    backend,
                    optimization,
                    clang: Some(wrapper),
                    stop_after: Stage::Build,
                    emit_dir: retained.as_ref().map(|_| directory.join("ir")),
                    output: Some(directory.join("program")),
                    ..CompileOptions::default()
                },
            );
            if retained.is_some() {
                fs::write(directory.join("compile.json"), report.to_json()).unwrap();
            }
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(!codegen.object_reports.is_empty());
                assert!(codegen.object_reports.iter().all(|report| {
                    report.contains("\"input_kind\":\"builder\"")
                        && report.contains("\"verify_before_after\":true")
                        && report.contains("\"parse_ns\":0,")
                }));
            }
            for (mode, status, stdout) in [
                (b'0', 0, "REVSPBCAH"),
                (b'1', 1, "REVFPBCAH"),
                (b'2', 2, "RNBAH"),
            ] {
                let mut child = Command::new(&report.artifacts["executable"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(&[mode]).unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while child.try_wait().unwrap().is_none() {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("317 event loop timed out");
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                let actual = child.wait_with_output().unwrap();
                if retained.is_some() {
                    fs::write(directory.join(format!("{mode}-stdout")), &actual.stdout).unwrap();
                    fs::write(
                        directory.join(format!("{mode}-status")),
                        actual.status.to_string(),
                    )
                    .unwrap();
                }
                assert_eq!(
                    actual.status.code(),
                    Some(status),
                    "mode {mode}: {actual:?}"
                );
                assert_eq!(actual.stdout, stdout.as_bytes(), "mode {mode}: {actual:?}");
                assert!(actual.stderr.is_empty(), "mode {mode}: {actual:?}");
            }
            if retained.is_none() {
                fs::remove_dir_all(directory).unwrap();
            }
        }
    }
}

#[cfg(target_os = "linux")]
const THREAD_RETURNED_FACTORY: &str = "fn worker(n:i32)->i32{return n+1}fn make()->impl Fn()->i32{let result=thread_scope(worker,7,()->i32=>1);let x=result.1;return move()->i32=>x}fn main()->i32{let f=make();return f()-8}";

#[test]
#[cfg(target_os = "linux")]
fn thread_local_parent_review_direct_factory_scalar_graph_and_once_body() {
    let report = compile_source(&source(THREAD_RETURNED_FACTORY), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let src = source(THREAD_RETURNED_FACTORY);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let factory = typed.functions.iter().find(|f| f.name == "make").unwrap();
    assert!(factory.environment.is_none());
    assert!(factory.callable_contract.is_none());
    assert!(factory.callable_return_contract.is_some());

    for (name, text, code) in [
        (
            "unreachable_address",
            "fn worker(n:i32)->i32{if false{let p=&n;return *p}return n+1}fn main()->i32{let r=thread_scope(worker,7,()->i32=>1);return r.1-8}",
            "E-THREAD-WORKER",
        ),
        (
            "once_reuse",
            "resource Mark{n:i32}impl Drop for Mark{fn drop(self:Self){}}fn worker(n:i32)->i32{return n+1}fn main()->i32{let a=Mark{n:7};let body=move()->i32=>a.n;let first=thread_scope(worker,7,body);let second=thread_scope(worker,9,body);return first.1+second.1}",
            "E-UNINITIALIZED",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Analyze,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == code && d.phase == zil_base::Phase::Analyze && d.span.is_some()),
            "{name}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[cfg(target_os = "linux")]
fn thread_local_parent_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/mainline/concurrency325")
}

#[cfg(target_os = "linux")]
fn thread_local_parent_project() -> zil_driver::ProjectSources {
    use std::collections::BTreeMap;
    let mut paths = BTreeMap::new();
    let modules = ["main", "worker", "observer", "component"]
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let index = index as u32;
            let path = thread_local_parent_root().join(format!("{name}.zil"));
            if index != 0 {
                paths.insert(vec![(*name).into()], ModuleId(index));
            }
            SourceUnit::new(
                FileId(index),
                ModuleId(index),
                path.to_string_lossy().into_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    zil_driver::ProjectSources {
        root: ModuleId(0),
        modules,
        paths,
    }
}

#[test]
#[cfg(target_os = "linux")]
fn thread_local_parent_source_and_worker_boundaries() {
    let report =
        zil_driver::compile_project(&thread_local_parent_project(), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let mut private = thread_local_parent_project();
    private.modules[0].text = private.modules[0]
        .text
        .replace("worker.compute", "worker.step");
    let report = zil_driver::compile_project(&private, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-MODULE-PRIVATE"),
        "{}",
        report.to_json()
    );
    let text =
        std::fs::read_to_string(thread_local_parent_root().join("controls/parent_borrow.zil"))
            .unwrap();
    let report = compile_source(&source(&text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for (name, phase, code) in [
        ("foreign_worker", zil_base::Phase::Types, "E-THREAD-WORKER"),
        ("stored_worker", zil_base::Phase::Types, "E-THREAD-WORKER"),
        ("wrong_body", zil_base::Phase::Types, "E-THREAD-BODY"),
        (
            "transitive_foreign",
            zil_base::Phase::Analyze,
            "E-THREAD-WORKER",
        ),
        (
            "address_worker",
            zil_base::Phase::Analyze,
            "E-THREAD-WORKER",
        ),
        (
            "body_borrow_conflict",
            zil_base::Phase::Verify,
            "E-BORROW-CONFLICT",
        ),
        ("ct_thread", zil_base::Phase::Ct, "E-CT-FORBIDDEN"),
    ] {
        let path = thread_local_parent_root().join(format!("negative/{name}.zil"));
        let input = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            path.to_string_lossy().into_owned(),
            std::fs::read_to_string(&path).unwrap(),
        );
        let report = compile_source(&input, &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{name}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == code
                && d.phase == phase
                && d.span.is_some_and(|span| span.file == FileId(0))),
            "{name}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
    // 332 promotes this exact historical source; the path and bytes stay fixed.
    let text =
        std::fs::read_to_string(thread_local_parent_root().join("pending/parent_closure.zil"))
            .unwrap();
    let report = compile_source(&source(&text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let text =
        std::fs::read_to_string(thread_local_parent_root().join("pending/callable_wrapper.zil"))
            .unwrap();
    let report = compile_source(&source(&text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Expand);
    assert!(report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"));
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
#[cfg(target_os = "linux")]
fn thread_local_parent_raw_wire_worker_identity_and_once_body() {
    use zil_ir::InstructionKind;
    let src = source(
        "resource Mark{code:i32} impl Drop for Mark{fn drop(self:Self){}} fn worker(value:i32)->i32{return value+1} fn main()->i32{let a=Mark{code:4};let b=Mark{code:7};let first=move()->i32=>a.code;let second=move()->i32=>b.code;let x=thread_scope(worker,3,first);let y=thread_scope(worker,5,second);return x.1+y.2}",
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let (decoded, decoded_plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    let verified = zil_verify::verify_module(decoded, decoded_plan, &target, &limits).unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    assert_eq!(
        lir.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
            .count(),
        2
    );
    let caller = typed
        .functions
        .iter()
        .position(|f| {
            f.blocks.iter().any(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
            })
        })
        .unwrap();
    for variant in [0, 1, 2] {
        let mut forged = typed.clone();
        let teardown = forged.resources.iter().find_map(|r| r.teardown).unwrap();
        let mut body = None;
        let mut count = 0;
        for instruction in forged.functions[caller]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
        {
            if let InstructionKind::WithScopedThread {
                worker,
                body: actual,
                ..
            } = &mut instruction.kind
            {
                if variant == 0 {
                    *worker = zil_ir::FunctionId(u32::MAX);
                    break;
                }
                if variant == 1 {
                    *worker = teardown;
                    break;
                }
                count += 1;
                if count == 1 {
                    body = Some(*actual);
                } else {
                    *actual = body.unwrap();
                    break;
                }
            }
        }
        let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "variant {variant}: {errors:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let errors = zil_verify::verify_module(forged, claim, &target, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "variant {variant}: {errors:?}"
        );
    }
    // 332 admits this well-formed concrete parent entry after reconstructing
    // its actual binding; the added environment bit alone is not authority.
    let mut future = typed.clone();
    let id = zil_ir::ClosureId(future.closures.len() as u32);
    future.functions[caller].environment = Some(id);
    future.closures.push(zil_ir::ClosureDefinition {
        storage_life: None,
        id,
        function: future.functions[caller].id,
        captures: vec![],
        parameters: vec![],
        result: zil_ir::Type::I32,
        kind: zil_ir::CallableKind::Fn,
        lending_captures: vec![],
    });
    let future_plan = zil_analyze::analyze(&future, &limits).unwrap();
    zil_verify::verify_module(future.clone(), future_plan.clone(), &target, &limits).unwrap();
    for variant in 0..5 {
        let mut forged = future.clone();
        match variant {
            0 => forged.functions[caller].environment = Some(zil_ir::ClosureId(u32::MAX)),
            1 => forged.closures[id.0 as usize].function = zil_ir::FunctionId(u32::MAX),
            2 => forged.closures[id.0 as usize].result = zil_ir::Type::Bool,
            3 => forged.closures[id.0 as usize]
                .parameters
                .push(zil_ir::Type::I32),
            4 => forged.functions[caller].result_mode = zil_ir::PassingMode::Move,
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "parent variant {variant}: {errors:?}"
        );
        let mut claim = future_plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let errors = zil_verify::verify_module(forged, claim, &target, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "parent variant {variant}: {errors:?}"
        );
    }
    // Exact worker traversal work comes from the current request, not a fresh per-worker budget.
    for verify in [false, true] {
        let mut low = 0;
        let mut high = limits.max_analysis_steps;
        while low + 1 < high {
            let selected = Limits {
                max_analysis_steps: (low + high) / 2,
                ..limits.clone()
            };
            let accepted = if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &selected).is_ok()
            } else {
                zil_analyze::analyze(&typed, &selected).is_ok()
            };
            if accepted {
                high = selected.max_analysis_steps;
            } else {
                low = selected.max_analysis_steps;
            }
        }
        for _ in 0..2 {
            let selected = Limits {
                max_analysis_steps: high,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &selected).unwrap();
            } else {
                zil_analyze::analyze(&typed, &selected).unwrap();
            }
        }
        let selected = Limits {
            max_analysis_steps: high - 1,
            ..limits.clone()
        };
        let errors = if verify {
            zil_verify::verify_module(typed.clone(), plan.clone(), &target, &selected).unwrap_err()
        } else {
            zil_analyze::analyze(&typed, &selected).unwrap_err()
        };
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "verify={verify} N={high}: {errors:?}"
        );
        eprintln!(
            "thread325 {} work N={high}",
            if verify { "Verify" } else { "Analyze" }
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn thread_local_parent_native_os_join_parent_cleanup_and_failure() {
    thread_native_parent_observer("direct");
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_native_concrete_parent_cleanup_and_failure() {
    thread_native_parent_observer("owned");
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_native_nested_roots_and_failures() {
    thread_native_parent_observer("nested");
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_cleanup_native_shared_pointer_load_cleanup_and_failure() {
    thread_native_parent_observer("shared");
}

#[cfg(target_os = "linux")]
fn thread_native_parent_observer(profile: &str) {
    let shared = profile == "shared";
    let concrete_parent = profile != "direct" && !shared;
    let nested = profile == "nested";
    use std::{
        fs,
        io::Write,
        os::unix::{fs::PermissionsExt, process::ExitStatusExt},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
            let directory = retained
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(format!(
                    "thread325-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir(&directory).unwrap();
            let observer = directory.join("observer.o");
            let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread", "-c"])
                .arg(thread_local_parent_root().join(if shared {
                    "shared339/observer.c"
                } else if nested {
                    "parent332/nested_observer.c"
                } else {
                    "observer.c"
                }))
                .arg("-o")
                .arg(&observer)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            let wrapper = directory.join("link.sh");
            fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -pthread -Wl,--wrap=pthread_create -Wl,--wrap=pthread_join\nfi\nexec {} \"$@\"\n", quote(&clang), quote(&observer), quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let units =
                if !nested && backend == NativeBackend::LlvmApi && optimization == Optimization::O2
                {
                    4
                } else {
                    1
                };
            let mut project = thread_local_parent_project();
            if shared {
                project.modules = ["native_main", "native_worker"]
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        let path = thread_local_parent_root().join(format!("shared339/{name}.zil"));
                        SourceUnit::new(
                            FileId(index as u32),
                            ModuleId(index as u32),
                            path.to_string_lossy().into_owned(),
                            fs::read_to_string(path).unwrap(),
                        )
                    })
                    .collect();
                project.paths.clear();
                project.paths.insert(vec!["worker".into()], ModuleId(1));
            }
            if concrete_parent {
                let path = thread_local_parent_root().join(if nested {
                    "parent332/nested_owned.zil"
                } else {
                    "parent332/owned_main.zil"
                });
                project.modules[0] = SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    path.to_string_lossy().into_owned(),
                    fs::read_to_string(path).unwrap(),
                );
            }
            if nested {
                project.modules.truncate(1);
                project.paths.clear();
            }
            let report = zil_driver::compile_project(
                &project,
                &CompileOptions {
                    backend,
                    optimization,
                    clang: Some(wrapper),
                    stop_after: Stage::Build,
                    codegen: zil_backend_llvm::CodegenOptions { units, jobs: 1 },
                    output: Some(directory.join("program")),
                    emit_dir: retained.as_ref().map(|_| directory.join("ir")),
                    ..CompileOptions::default()
                },
            );
            if retained.is_some() {
                fs::write(directory.join("compile.json"), report.to_json()).unwrap();
            }
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none(), "{}", report.to_json());
            assert_eq!(codegen.requested_units, units);
            if units > 1 {
                assert!(codegen.actual_units > 1, "{}", report.to_json());
            }
            if backend == NativeBackend::LlvmApi {
                assert!(!codegen.object_reports.is_empty());
                assert!(codegen.object_reports.iter().all(|r| {
                    r.contains("\"input_kind\":\"builder\"")
                        && r.contains("\"verify_before_after\":true")
                        && r.contains("\"parse_ns\":0,")
                }));
            }
            let cases = if shared {
                &[
                    (b'0', Some(0), None, "CBDJA"),
                    (b'1', Some(1), None, "CBDJA"),
                    (b'2', Some(2), None, "CDA"),
                    (b'3', None, Some(6), "CBDX"),
                ][..]
            } else if nested {
                &[
                    (b'0', Some(0), None, "CBCbIjaOJA"),
                    (b'1', Some(3), None, "CBCIaOJA"),
                    (b'2', Some(2), None, "COA"),
                    (b'3', None, Some(6), "CBCbIx"),
                    (b'4', None, Some(6), "CBCbIjaOX"),
                ][..]
            } else {
                &[
                    (b'0', Some(0), None, "PECBDJA"),
                    (b'1', Some(1), None, "PECBDJA"),
                    (b'2', Some(2), None, "PECDA"),
                    (b'3', None, Some(6), "PECBDX"),
                ][..]
            };
            for &(mode, status, signal, stdout) in cases {
                let mut child = Command::new(&report.artifacts["executable"])
                    .current_dir(&directory)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(&[mode]).unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while child.try_wait().unwrap().is_none() {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("thread325 join timed out");
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                let actual = child.wait_with_output().unwrap();
                if retained.is_some() {
                    fs::write(directory.join(format!("{mode}-stdout")), &actual.stdout).unwrap();
                    fs::write(
                        directory.join(format!("{mode}-status")),
                        actual.status.to_string(),
                    )
                    .unwrap();
                }
                assert_eq!(
                    actual.status.code(),
                    status,
                    "{backend:?}/{optimization:?} mode {mode}: {actual:?}"
                );
                assert_eq!(actual.status.signal(), signal, "{actual:?}");
                assert_eq!(actual.stdout, stdout.as_bytes(), "{actual:?}");
                assert!(actual.stderr.is_empty(), "{actual:?}");
            }
            // Parent borrowing, life/trait composition, and a Direct factory reach both machines.
            let controls = if shared {
                &[
                    "shared_child",
                    "two_scopes",
                    "nested_shared",
                    "helper_disjoint",
                    "copied_lender",
                    "owned_lender_retained",
                    "ancestor_shared",
                    "disjoint_write",
                    "nested_disjoint",
                    "parent_parameter",
                    "parent_return",
                    "parent_capture",
                ][..]
            } else if nested {
                &[][..]
            } else if concrete_parent {
                &["kinds", "nested", "borrow", "parent_closure"][..]
            } else {
                &["parent_borrow", "life_trait", "return_factory"][..]
            };
            for control in controls {
                let text = if shared {
                    fs::read_to_string(
                        thread_local_parent_root().join(format!("shared339/{control}.zil")),
                    )
                    .unwrap()
                } else if concrete_parent {
                    let path = if *control == "parent_closure" {
                        "pending/parent_closure.zil".to_owned()
                    } else {
                        format!("parent332/{control}.zil")
                    };
                    fs::read_to_string(thread_local_parent_root().join(path)).unwrap()
                } else if *control == "return_factory" {
                    THREAD_RETURNED_FACTORY.to_owned()
                } else {
                    fs::read_to_string(
                        thread_local_parent_root().join(format!("controls/{control}.zil")),
                    )
                    .unwrap()
                };
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(directory.join(control)),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let actual = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(if *control == "parent_closure" { 14 } else { 0 }),
                    "{control}: {actual:?}"
                );
                assert!(
                    actual.stdout.is_empty() && actual.stderr.is_empty(),
                    "{actual:?}"
                );
            }
            if retained.is_none() {
                fs::remove_dir_all(directory).unwrap();
            }
        }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_concrete_parent_kinds_and_nested_source() {
    use zil_ir::{CallableKind, InstructionKind};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    for name in ["kinds", "nested", "borrow"] {
        let text = std::fs::read_to_string(
            thread_local_parent_root().join(format!("parent332/{name}.zil")),
        )
        .unwrap();
        let src = source(&text);
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let core = zil_expand::expand(&src, &ast).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let kinds = typed
            .functions
            .iter()
            .filter(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
            })
            .map(|f| typed.closures[f.environment.unwrap().0 as usize].kind)
            .collect::<Vec<_>>();
        if name == "kinds" {
            assert!(kinds.contains(&CallableKind::Fn), "{kinds:?}");
            assert!(kinds.contains(&CallableKind::FnMut), "{kinds:?}");
            assert!(kinds.contains(&CallableKind::FnOnce), "{kinds:?}");
        } else if name == "nested" {
            assert_eq!(kinds.len(), 2);
        } else {
            assert_eq!(kinds.len(), 1);
        }
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        if name == "kinds" {
            thread_parent_budget(&typed, &plan);
            // Forcing a shared entry must not copy the parent's owned body.
            let mut forged = typed.clone();
            let once = forged
                .closures
                .iter_mut()
                .find(|c| {
                    c.kind == CallableKind::FnOnce
                        && c.captures
                            .iter()
                            .any(|t| matches!(t, zil_ir::Type::Managed(_)))
                })
                .unwrap();
            once.kind = CallableKind::Fn;
            let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
            assert_eq!(errors[0].category, ResultCategory::SafeReject, "{errors:?}");
            let mut claim = plan.clone();
            claim.module_fingerprint = forged.fingerprint();
            let errors = zil_verify::verify_module(forged, claim, &target, &limits).unwrap_err();
            assert_eq!(errors[0].category, ResultCategory::SafeReject, "{errors:?}");
        }
        let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let verified = zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
        zil_lower::lower(&verified).unwrap();
    }
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_parent_boundaries() {
    for (name, phase, code) in [
        ("once_reuse", zil_base::Phase::Analyze, "E-UNINITIALIZED"),
        ("thin_body", zil_base::Phase::Types, "E-THREAD-BODY"),
        ("ct_parent", zil_base::Phase::Ct, "E-CT-FORBIDDEN"),
        ("nested_worker", zil_base::Phase::Analyze, "E-THREAD-WORKER"),
        (
            "borrow_conflict",
            zil_base::Phase::Analyze,
            "E-BORROW-CONFLICT",
        ),
    ] {
        let text = std::fs::read_to_string(
            thread_local_parent_root().join(format!("parent332/{name}.zil")),
        )
        .unwrap();
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{name}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{name}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
        if name == "borrow_conflict" {
            let positive = text.replace("        *input=4\n", "");
            let report = compile_source(&source(&positive), &CompileOptions::default());
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
        }
    }
}

const THREAD_PARENT_CAPTURED_BODY: &str = "resource Token{value:i32}impl Drop for Token{fn drop(self:Self){}}fn worker(value:i32)->i32{return value+1}fn main()->i32{let token=Token{value:42};let body=move()->i32=>token.value;let other=(offset:i32)->i32=>offset;let parent=move(offset:i32)->i32=>{let flag=true;let result=thread_scope(worker,offset,(body));if !flag{return 1}return result.1+result.2};return parent(5)-48}";

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_reviewer_types_work_is_shared_across_names_and_parent_entries() {
    fn program(worker: &str, capture: &str, count: usize, dummy_captures: usize) -> String {
        let mut text = format!(
            "resource Token{{value:i32}}impl Drop for Token{{fn drop(self:Self){{}}}}fn {worker}(value:i32)->i32{{return value+1}}fn main()->i32{{var total=0;"
        );
        for index in 0..count {
            for dummy in 0..dummy_captures {
                text.push_str(&format!("let dummy{index}_{dummy}=0;"));
            }
            let padding = (0..dummy_captures)
                .map(|dummy| format!("let padding{dummy}=dummy{index}_{dummy};"))
                .collect::<String>();
            text.push_str(&format!("let token{index}=Token{{value:42}};let {capture}{index}=move()->i32=>token{index}.value;let parent{index}=move(offset:i32)->i32=>{{let result=thread_scope({worker},offset,{capture}{index});return result.1+result.2}};total+=parent{index}(5);"));
            if dummy_captures != 0 {
                text = text.replace(
                    &format!("let parent{index}=move(offset:i32)->i32=>{{let result="),
                    &format!("let parent{index}=move(offset:i32)->i32=>{{{padding}let result="),
                );
            }
        }
        text.push_str(&format!("return total-{};}}", 48 * count));
        text
    }
    let long_worker = format!("worker_{}", "w".repeat(128));
    let long_capture = format!("body_{}", "c".repeat(128));
    let wide_capture = format!("body_{}", "c".repeat(512));
    let nested="resource Token{value:i32}impl Drop for Token{fn drop(self:Self){}}fn worker(value:i32)->i32{return value+1}fn main()->i32{let token=Token{value:42};let deepest=move()->i32=>token.value;let inner=move()->i32=>{let result=thread_scope(worker,1,deepest);return result.1+result.2};let outer=move()->i32=>{let result=thread_scope(worker,2,inner);return result.1+result.2};return outer()-47}".to_owned();
    let mut measured = Vec::new();
    for (label, text) in [
        ("short", program("worker", "body", 1, 0)),
        ("long-worker", program(&long_worker, "body", 1, 0)),
        ("long-capture", program("worker", &long_capture, 1, 0)),
        ("four-parents", program(&long_worker, &long_capture, 4, 0)),
        (
            "two-wide-parents",
            program(&long_worker, &wide_capture, 2, 24),
        ),
        ("nested", nested),
    ] {
        let src = source(&text);
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let core = zil_expand::expand(&src, &ast).unwrap();
        let limits = Limits::default();
        let target = zil_base::TargetProfile::default();
        let run = |n| {
            zil_types::check(
                &core,
                &target,
                &Limits {
                    max_analysis_steps: n,
                    ..limits.clone()
                },
            )
        };
        let expected = run(limits.max_analysis_steps).unwrap();
        if label == "two-wide-parents" {
            let parents = expected
                .functions
                .iter()
                .filter(|f| {
                    f.blocks
                        .iter()
                        .flat_map(|b| &b.instructions)
                        .any(|i| matches!(i.kind, zil_ir::InstructionKind::WithScopedThread { .. }))
                })
                .collect::<Vec<_>>();
            assert_eq!(parents.len(), 2);
            for parent in parents {
                assert_eq!(
                    expected.closures[parent.environment.unwrap().0 as usize]
                        .captures
                        .len(),
                    25
                );
            }
            let plan = zil_analyze::analyze(&expected, &limits).unwrap();
            zil_verify::verify_module(expected.clone(), plan, &target, &limits).unwrap();
        }
        let (mut low, mut high) = (0, limits.max_analysis_steps);
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(!errors.is_empty());
                    assert!(
                        errors.iter().all(|d| d.phase == zil_base::Phase::Types
                            && d.category == ResultCategory::ResourceLimit),
                        "{label}/{middle}: {errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        let errors = run(high - 1).unwrap_err();
        assert!(!errors.is_empty());
        assert!(
            errors.iter().all(|d| d.phase == zil_base::Phase::Types
                && d.category == ResultCategory::ResourceLimit),
            "{label}: {errors:?}"
        );
        assert_eq!(run(high).unwrap(), expected);
        eprintln!(
            "review332 Types {label} boundary error: {}",
            errors[0].message
        );
        eprintln!("review332 Types {label} N={high}, N-1 ResourceLimit, fresh N identical");
        measured.push((label, high));
    }
    eprintln!("review332 Types boundaries: {measured:?}");
    let wide = measured
        .iter()
        .find(|(label, _)| *label == "two-wide-parents")
        .unwrap()
        .1;
    // Each parent has 24 actual Copy captures before its owned body capture.
    // These are two separate environment lookups in the same Types request.
    // cb17 gives each Graph a fresh allowance, so a dynamic N search alone
    // would incorrectly pass its missing cumulative charge.
    let required = 2 * 25 * (wide_capture.len() + 2);
    assert!(
        wide >= required,
        "two real parent lookups cost at least {required} shared steps, Types admitted at {wide}"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_reviewer_real_owned_body_forces_parent_once_before_actual_entry() {
    let src = source(THREAD_PARENT_CAPTURED_BODY);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let parent = typed
        .functions
        .iter()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, zil_ir::InstructionKind::WithScopedThread { .. }))
        })
        .unwrap();
    let definition = &typed.closures[parent.environment.unwrap().0 as usize];
    assert_eq!(definition.function, parent.id);
    assert_eq!(definition.parameters, [zil_ir::Type::I32]);
    assert_eq!(definition.kind, zil_ir::CallableKind::FnOnce);
    assert!(
        definition
            .captures
            .iter()
            .any(|t| matches!(t, zil_ir::Type::Closure(_)))
    );
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed, plan, &target, &limits).unwrap();
    let reused = THREAD_PARENT_CAPTURED_BODY.replace(
        "return parent(5)-48",
        "let first=parent(5);return first+parent(6)-97",
    );
    let report = compile_source(&source(&reused), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.phase,
        zil_base::Phase::Analyze,
        "{}",
        report.to_json()
    );
    assert_eq!(report.diagnostics[0].code, "E-UNINITIALIZED");
    let helper =
        std::fs::read_to_string(thread_local_parent_root().join("pending/callable_wrapper.zil"))
            .unwrap();
    let report = compile_source(&source(&helper), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.phase,
        zil_base::Phase::Expand,
        "{}",
        report.to_json()
    );
    assert_eq!(report.diagnostics[0].code, "E-UNSUPPORTED");
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    let directory =
        std::env::temp_dir().join(format!("zil-review332-owned-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_source(
                &src,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(directory.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let mut child = Command::new(&report.artifacts["executable"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("independent captured parent join timed out");
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {output:?}"
            );
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_reviewer_raw_parent_metadata_and_shared_once_claims_are_not_proofs() {
    use zil_ir::{
        CallableKind, ClosureId, FunctionId, InstructionKind, LocalId, PassingMode, Type,
    };
    let src = source(THREAD_PARENT_CAPTURED_BODY);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let parent = typed
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
        })
        .unwrap();
    let id = typed.functions[parent].environment.unwrap();
    let other = typed
        .closures
        .iter()
        .find(|c| c.id != id && c.parameters == [Type::I32] && c.result == Type::I32)
        .unwrap()
        .id;
    let positive = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&positive, &target, &limits).unwrap();
    let expected = [
        ("E-ANALYZE-IR", "E-VERIFY-ID"),
        ("E-ANALYZE-IR", "E-VERIFY-CLOSURE-ENTRY"),
        ("E-THREAD-WORKER", "E-VERIFY-CLOSURE-ENTRY"),
        ("E-THREAD-WORKER", "E-VERIFY-CLOSURE"),
        ("E-THREAD-WORKER", "E-VERIFY-CLOSURE-ENTRY"),
        ("E-THREAD-WORKER", "E-VERIFY-CLOSURE-ENTRY"),
        ("E-ANALYZE-IR", "E-VERIFY-ID"),
        ("E-THREAD-WORKER", "E-VERIFY-RESOURCE"),
        ("E-THREAD-WORKER", "E-VERIFY-CLOSURE-ENTRY"),
        ("E-ANALYZE-IR", "E-VERIFY-CLOSURE-ENTRY"),
        ("E-ANALYZE-IR", "E-VERIFY-CLOSURE-CALL"),
    ];
    for (variant, (analyze_code, verify_code)) in expected.into_iter().enumerate() {
        let mut changed = typed.clone();
        match variant {
            0 => changed.functions[parent].id = FunctionId(0),
            1 => {
                changed.functions[parent].environment =
                    Some(ClosureId(changed.closures.len() as u32))
            }
            2 => changed.functions[parent].environment = Some(other),
            3 => changed.closures[id.0 as usize].id = other,
            4 => {
                changed.closures[id.0 as usize].function =
                    changed.closures[other.0 as usize].function
            }
            5 => changed.closures[id.0 as usize].parameters[0] = Type::Bool,
            6 => {
                changed.functions[parent].parameters[0] =
                    LocalId(changed.functions[parent].locals.len() as u32)
            }
            7 => changed.functions[parent].result_mode = PassingMode::Move,
            8 => changed.functions[parent].return_type = Type::Bool,
            9 => changed.functions[parent].environment = None,
            10 => changed.closures[id.0 as usize].kind = CallableKind::Fn,
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&changed, &limits).unwrap_err();
        assert_eq!(
            errors[0].phase,
            zil_base::Phase::Analyze,
            "{variant}: {errors:?}"
        );
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "{variant}: {errors:?}"
        );
        assert_eq!(errors[0].code, analyze_code, "{variant}: {errors:?}");
        let mut claim = plan.clone();
        claim.module_fingerprint = changed.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&changed, &claim, &limits).unwrap();
        for (form, result) in [
            (
                "Verify raw",
                zil_verify::verify_module(changed.clone(), claim, &target, &limits),
            ),
            (
                "Verify Wire",
                zil_verify::verify_module_bytes(&bytes, &target, &limits),
            ),
        ] {
            let errors = result.unwrap_err();
            assert_eq!(
                errors[0].phase,
                zil_base::Phase::Verify,
                "{variant}/{form}: {errors:?}"
            );
            assert_eq!(
                errors[0].category,
                ResultCategory::SafeReject,
                "{variant}/{form}: {errors:?}"
            );
            assert_eq!(errors[0].code, verify_code, "{variant}/{form}: {errors:?}");
        }
    }
}

#[cfg(target_os = "linux")]
fn thread_parent_budget(typed: &zil_ir::TypedModule, plan: &zil_ir::CandidatePlan) {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    for verify in [false, true] {
        let mut low = 0;
        let mut high = limits.max_analysis_steps;
        while low + 1 < high {
            let selected = Limits {
                max_analysis_steps: (low + high) / 2,
                ..limits.clone()
            };
            let accepted = if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &selected).is_ok()
            } else {
                zil_analyze::analyze(typed, &selected).is_ok()
            };
            if accepted {
                high = selected.max_analysis_steps;
            } else {
                low = selected.max_analysis_steps;
            }
        }
        for amount in [high, high - 1, high] {
            let selected = Limits {
                max_analysis_steps: amount,
                ..limits.clone()
            };
            let result = if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &selected)
                    .map(|_| ())
            } else {
                zil_analyze::analyze(typed, &selected).map(|_| ())
            };
            if amount == high {
                result.unwrap();
            } else {
                assert_eq!(
                    result.unwrap_err()[0].category,
                    ResultCategory::ResourceLimit
                );
            }
        }
        eprintln!(
            "thread332 {} cumulative parent work N={high}",
            if verify { "Verify" } else { "Analyze" }
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_cleanup_shared_source_and_independent_windows() {
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    for name in [
        "shared_child",
        "two_scopes",
        "nested_shared",
        "helper_disjoint",
        "copied_lender",
        "owned_lender_retained",
        "ancestor_shared",
        "disjoint_write",
        "nested_disjoint",
        "parent_parameter",
        "parent_return",
        "parent_capture",
        "scalar_control",
    ] {
        let text = std::fs::read_to_string(
            thread_local_parent_root().join(format!("shared339/{name}.zil")),
        )
        .unwrap();
        let src = source(&text);
        let report = compile_source(&src, &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let core = zil_expand::expand(&src, &ast).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        let (raw, claim) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let verified = zil_verify::verify_module(raw, claim, &target, &limits).unwrap();
        zil_lower::lower(&verified).unwrap();
        if name == "disjoint_write" {
            thread_parent_budget(&typed, &plan);
            // Substitute the child argument with a shared reborrow of the
            // body's actual exclusive y capture. No source producer approves it.
            let mut forged = typed.clone();
            let caller = forged
                .functions
                .iter_mut()
                .find(|f| {
                    f.blocks
                        .iter()
                        .flat_map(|b| &b.instructions)
                        .any(|i| matches!(i.kind, zil_ir::InstructionKind::WithScopedThread { .. }))
                })
                .unwrap();
            let writable = caller
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match i.kind {
                    zil_ir::InstructionKind::BorrowLocalMut { .. } => i.result,
                    _ => None,
                })
                .unwrap();
            let value = zil_ir::ValueId(caller.values.len() as u32);
            caller.values.push(zil_ir::Type::Address {
                mutable: false,
                inner: Box::new(zil_ir::Type::I32),
            });
            for block in &mut caller.blocks {
                if let Some(index) = block.instructions.iter().position(|i| {
                    matches!(i.kind, zil_ir::InstructionKind::WithScopedThread { .. })
                }) {
                    let span = block.instructions[index].span;
                    block.instructions.insert(
                        index,
                        zil_ir::Instruction {
                            result: Some(value),
                            kind: zil_ir::InstructionKind::ReborrowAddress {
                                address: writable,
                                mutable: false,
                            },
                            span,
                        },
                    );
                    let zil_ir::InstructionKind::WithScopedThread { argument, .. } =
                        &mut block.instructions[index + 1].kind
                    else {
                        unreachable!()
                    };
                    *argument = value;
                }
            }
            let caller_id = caller.id;
            let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
            assert!(
                errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
                "{errors:?}"
            );
            let mut claim = plan.clone();
            claim.module_fingerprint = forged.fingerprint();
            let function = &mut claim.functions[caller_id.0 as usize];
            let roots = function.holds[writable.0 as usize].roots.clone();
            function.holds.push(zil_ir::ValueHold { value, roots });
            let errors = zil_verify::verify_module(forged, claim, &target, &limits).unwrap_err();
            assert!(
                errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
                "{errors:?}"
            );
        }
    }
    for (name, category, code) in [
        (
            "body_write",
            ResultCategory::SafeReject,
            "E-BORROW-CONFLICT",
        ),
        (
            "ancestor_write",
            ResultCategory::SafeReject,
            "E-BORROW-CONFLICT",
        ),
        (
            "helper_writer",
            ResultCategory::SafeReject,
            "E-BORROW-CONFLICT",
        ),
        (
            "owned_lender_cleanup",
            ResultCategory::SafeReject,
            "E-BORROW-CONFLICT",
        ),
        (
            "owner_cleanup",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "lending_pending",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "nested_writer",
            ResultCategory::SafeReject,
            "E-BORROW-CONFLICT",
        ),
        (
            "mutable_worker",
            ResultCategory::SafeReject,
            "E-THREAD-WORKER",
        ),
        (
            "projection_pending",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
    ] {
        let text = std::fs::read_to_string(
            thread_local_parent_root().join(format!("shared339/{name}.zil")),
        )
        .unwrap();
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(report.status, category, "{name}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{name}: {}",
            report.to_json()
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_cleanup_raw_dynamic_origin_is_unsupported_before_lowering() {
    use zil_ir::InstructionKind;
    let src = source(
        "struct Item{value:i32}fn read(input:&i32)->i32{return *input}fn scalar(input:i32)->i32{return input}fn main()->i32{let item=Item{value:9};let pointer=&item.value;let body=()->i32=>0;let result=thread_scope(scalar,5,body);let observed=read(pointer);return result.1+observed}",
    );
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    let mut claim = zil_analyze::analyze(&typed, &limits).unwrap();
    let reader = typed
        .functions
        .iter()
        .find(|f| f.name.ends_with("read"))
        .unwrap()
        .id;
    let caller = typed
        .functions
        .iter_mut()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
        })
        .unwrap();
    let pointer = caller
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::AddressField { .. } => i.result,
            _ => None,
        })
        .unwrap();
    for instruction in caller.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::WithScopedThread {
            worker, argument, ..
        } = &mut instruction.kind
        {
            *worker = reader;
            *argument = pointer;
        }
    }
    let errors = zil_analyze::analyze(&typed, &limits).unwrap_err();
    assert_eq!(
        errors[0].category,
        ResultCategory::Unsupported,
        "{errors:?}"
    );
    claim.module_fingerprint = typed.fingerprint();
    let errors = zil_verify::verify_module(typed, claim, &target, &limits).unwrap_err();
    assert_eq!(
        errors[0].category,
        ResultCategory::Unsupported,
        "{errors:?}"
    );
    assert_eq!(errors[0].phase, zil_base::Phase::Verify);
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_cleanup_types_shared_budget_and_scalar_control() {
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    for name in [
        "shared_child",
        "scalar_control",
        "disjoint_write",
        "parent_capture",
        "two_scopes",
        "nested_shared",
    ] {
        let text = std::fs::read_to_string(
            thread_local_parent_root().join(format!("shared339/{name}.zil")),
        )
        .unwrap();
        let src = source(&text);
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let core = zil_expand::expand(&src, &ast).unwrap();
        let run = |n| {
            zil_types::check(
                &core,
                &target,
                &Limits {
                    max_analysis_steps: n,
                    ..limits.clone()
                },
            )
        };
        let expected = run(limits.max_analysis_steps).unwrap();
        let (mut low, mut high) = (0, limits.max_analysis_steps);
        while low + 1 < high {
            let n = low + (high - low) / 2;
            match run(n) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = n
                }
                Err(e) => {
                    assert_eq!(e[0].category, ResultCategory::ResourceLimit, "{e:?}");
                    low = n
                }
            }
        }
        for n in [high, high - 1, high] {
            match run(n) {
                Ok(actual) => {
                    assert_eq!(n, high);
                    assert_eq!(actual, expected)
                }
                Err(e) => {
                    assert_eq!(n, high - 1);
                    assert_eq!(e[0].category, ResultCategory::ResourceLimit);
                }
            }
        }
        eprintln!("thread339 {name} Types N={high}, N-1 ResourceLimit, fresh N identical");
        let plan = zil_analyze::analyze(&expected, &limits).unwrap();
        if [
            "shared_child",
            "scalar_control",
            "two_scopes",
            "nested_shared",
        ]
        .contains(&name)
        {
            eprintln!("thread339 {name} kernel budgets follow");
            thread_parent_budget(&expected, &plan);
        }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn thread_parent_cleanup_helper_return_mutable_roots_are_joint_in_both_kernels() {
    use zil_ir::InstructionKind;
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let text =
        std::fs::read_to_string(thread_local_parent_root().join("shared339/helper_disjoint.zil"))
            .unwrap();
    let src = source(&text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    let mut claim = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), claim.clone(), &target, &limits).unwrap();
    let caller = typed
        .functions
        .iter_mut()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
        })
        .unwrap();
    let poison = caller
        .locals
        .iter()
        .find(|local| local.name == "poison")
        .unwrap()
        .id;
    let actual = caller
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::Store { local, value } if local == poison => Some(value),
            _ => None,
        })
        .unwrap();
    for instruction in caller.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::WithScopedThread { argument, .. } = &mut instruction.kind {
            *argument = actual;
        }
    }
    let errors = zil_analyze::analyze(&typed, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
        "{errors:?}"
    );
    claim.module_fingerprint = typed.fingerprint();
    let errors = zil_verify::verify_module(typed, claim, &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
        "{errors:?}"
    );
    let text =
        std::fs::read_to_string(thread_local_parent_root().join("shared339/helper_writer.zil"))
            .unwrap();
    let src = source(&text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let errors = zil_analyze::analyze(&typed, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
        "{errors:?}"
    );
}

#[test]
#[cfg(all(target_os = "linux", feature = "llvm-api"))]
fn thread_parent_cleanup_machine_cache_keeps_scalar_and_pointer_payloads_distinct() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let dir = std::env::temp_dir().join(format!("thread339-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (index, name) in ["scalar_control", "shared_child", "shared_child"]
        .iter()
        .enumerate()
    {
        let text = std::fs::read_to_string(
            thread_local_parent_root().join(format!("shared339/{name}.zil")),
        )
        .unwrap();
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                backend: NativeBackend::LlvmApi,
                optimization: Optimization::O2,
                stop_after: Stage::Build,
                object_cache: Some(dir.join("objects")),
                output: Some(dir.join(format!("program{index}"))),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, NativeBackend::LlvmApi);
        assert!(codegen.fallback.is_none());
        let result = std::process::Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
        if index == 2 {
            assert!(
                codegen
                    .object_reports
                    .iter()
                    .any(|r| r.contains("\"input_kind\":\"object-cache\"")),
                "{}",
                report.to_json()
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn async_registration_sealed_sync_scope_source_reaches_independent_kernel() {
    for text in [
        include_str!("../../../tests/language-readiness/terminal/343/atomic-scope.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/mutex-scope.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/atomic-contention.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/mutex-contention.zil"),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
#[cfg(target_os = "linux")]
fn async_registration_sealed_sync_native_contention() {
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
    for text in [
        include_str!("../../../tests/language-readiness/terminal/343/atomic-scope.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/mutex-scope.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/atomic-contention.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/mutex-contention.zil"),
    ] {
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil343-source-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&dir).unwrap();
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
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, *backend);
                assert!(codegen.fallback.is_none());
                let actual = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(0),
                    "{backend:?} {optimization:?}: {actual:?}"
                );
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn async_registration_sealed_sync_source_and_raw_boundaries() {
    use zil_ir::{
        AtomicOrdering as O, InstructionKind as I, SyncCellKind as K, SyncI32Op as P, Type,
    };
    for text in [
        "fn main()->i32 {let value=7;return atomic_load(&value)}",
        "fn unused(cell:&AtomicI32)->i32 {return mutex_load(cell)} fn main()->i32=>0",
        "fn main()->i32 {let atomic_load=7;return atomic_load}",
        "fn worker(cell:&MutexI32)->i32=>mutex_load(cell) fn main()->i32 {let r=atomic_scope(0,(cell:&AtomicI32)->i32=>{let t=thread_scope(worker,cell,()->i32=>0);return t.1});return r.1}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
    let src = source(include_str!(
        "../../../tests/language-readiness/terminal/343/atomic-contention.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let (decoded, claim) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(decoded, typed);
    let verified = zil_verify::verify_module(decoded, claim, &target, &limits).unwrap();
    zil_lower::lower(&verified).unwrap();
    let worker = typed
        .functions
        .iter()
        .position(|f| f.name == "worker")
        .unwrap();
    for variant in 0..6 {
        let mut forged = typed.clone();
        match variant {
            0 | 1 | 2 | 4 => {
                let function = &mut forged.functions[worker];
                let integer = zil_ir::ValueId(
                    function
                        .values
                        .iter()
                        .position(|ty| *ty == Type::I32)
                        .unwrap() as u32,
                );
                let instruction = function
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::SyncI32 { .. }))
                    .unwrap();
                if let I::SyncI32 {
                    op,
                    address,
                    value,
                    ordering,
                } = &mut instruction.kind
                {
                    match variant {
                        0 => *address = integer,
                        1 => {
                            *op = P::Load;
                            *ordering = O::Release;
                            *value = None
                        }
                        2 => {
                            *op = P::Load;
                            *value = Some(integer)
                        }
                        4 => {
                            let address = *address;
                            instruction.kind = I::ReadAddress { address }
                        }
                        _ => unreachable!(),
                    }
                }
            }
            3 => {
                let instruction = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::WithSyncCell { .. }))
                    .unwrap();
                if let I::WithSyncCell { kind, .. } = &mut instruction.kind {
                    *kind = K::MutexI32
                }
            }
            5 => {
                let instruction = forged.functions[worker]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::SyncI32 { .. }))
                    .unwrap();
                let I::SyncI32 { address, .. } = instruction.kind else {
                    unreachable!()
                };
                let result = instruction.result.unwrap();
                instruction.kind = I::ReborrowAddress {
                    address,
                    mutable: false,
                };
                forged.functions[worker].values[result.0 as usize] = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::SyncCell(K::MutexI32)),
                };
            }
            _ => unreachable!(),
        }
        let analyzed = zil_analyze::analyze(&forged, &limits).unwrap_err();
        assert_eq!(
            analyzed[0].category,
            ResultCategory::SafeReject,
            "raw Analyze {variant}: {analyzed:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let encoded = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        for wire in [false, true] {
            let (input, claim) = if wire {
                zil_ir::wire::decode_module_candidate(&encoded, &limits).unwrap()
            } else {
                (forged.clone(), claim.clone())
            };
            let errors = zil_verify::verify_module(input, claim, &target, &limits).unwrap_err();
            assert_eq!(
                errors[0].category,
                ResultCategory::SafeReject,
                "Verify {variant} wire={wire}: {errors:?}"
            );
        }
    }
    thread_parent_budget(&typed, &plan);
}

#[test]
#[cfg(target_os = "linux")]
fn async_registration_mutex_native_failures_and_owned_body_cleanup() {
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, process::ExitStatusExt},
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi)
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil343-failure-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let observer = dir.join("observer.o");
            let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread", "-c"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/language-readiness/terminal/343/sync-observer.c"),
                )
                .arg("-o")
                .arg(&observer)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -pthread -Wl,--wrap=pthread_mutex_init -Wl,--wrap=pthread_mutex_lock -Wl,--wrap=pthread_mutex_unlock -Wl,--wrap=pthread_mutex_destroy\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = compile_source(
                &source(include_str!(
                    "../../../tests/language-readiness/terminal/343/mutex-failure.zil"
                )),
                &CompileOptions {
                    backend,
                    optimization,
                    clang: Some(wrapper),
                    stop_after: Stage::Build,
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
            for (mode, code, signal, stdout) in [
                (0, Some(0), None, "IBLUDX"),
                (1, Some(2), None, "ID"),
                (2, None, Some(6), "IBL"),
                (3, None, Some(6), "IBLU"),
                (4, None, Some(6), "IBLUDX"),
            ] {
                let result = Command::new(&report.artifacts["executable"])
                    .env("ZIL343_SYNC_FAILURE", mode.to_string())
                    .output()
                    .unwrap();
                assert_eq!(
                    result.status.code(),
                    code,
                    "{backend:?}/{optimization:?}/mode{mode}: {result:?}"
                );
                assert_eq!(
                    result.status.signal(),
                    signal,
                    "{backend:?}/{optimization:?}/mode{mode}: {result:?}"
                );
                assert_eq!(
                    result.stdout,
                    stdout.as_bytes(),
                    "{backend:?}/{optimization:?}/mode{mode}: {result:?}"
                );
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn async_registration_source_to_persistent_native_frame() {
    let text = include_str!("../../../tests/language-readiness/terminal/343/task-poll.zil");
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn async_registration_native_yields_resume_once() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for text in [
        include_str!("../../../tests/language-readiness/terminal/343/task-poll.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/task-control.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/task-cancel.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/task-unused.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/task-move.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/task-static-callback.zil"),
        include_str!("../../../tests/language-readiness/terminal/343/task-nonyield.zil"),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil343-task-{}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
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
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(0),
                    "{backend:?} {optimization:?}: {actual:?}"
                );
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn async_registration_source_rejects_direct_paths_and_overlapping_heads() {
    for text in [
        include_str!("../../../tests/language-readiness/terminal/343/task-reject-foreign-ref.zil"),
        "fn worker(x:i32)->i32 {task_yield(); return x} fn main()->i32=>worker(7)",
        "fn worker(x:i32)->i32 {task_yield(); return x} fn main()->i32 {let f:fn(i32)->i32=worker;return f(7)}",
        "fn worker(x:i32)->i32 {task_yield(); return x} fn main()->i32 {let r=thread_scope(worker,7,()->i32=>0);return r.1}",
        "fn worker(x:i32)->i32 {task_yield();return x} fn main()->i32 {var task=task_start(worker,7); let a=&mut task; let b=&mut task; task_poll(a); task_poll(b); return 0}",
        "fn worker(x:i32)->i32 {task_yield();return x} fn main()->i32 {var task=task_start(worker,7); let a=&mut task; task_finish_destroy(give task); task_poll(a); return 0}",
        "fn worker(x:i32)->i32 {task_yield();return x} fn main()->i32 {var task=task_start(worker,7); task_poll_close(&mut task);task_finish_destroy(give task); task_finish_destroy(give task); return 0}",
        "fn worker(x:i32)->i32 {task_yield();return x} fn main()->i32 {let task=task_start(worker,7); task_poll(&mut task);return 0}",
        "fn main()->i32 {let task_start=7;return task_start}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}\n{}",
            report.to_json()
        );
    }
}

#[test]
fn task_poll_raw_wire_and_exact_kernel_budgets() {
    use zil_ir::{InstructionKind as I, Type};
    let text = include_str!("../../../tests/language-readiness/terminal/343/task-poll.zil");
    let src = source(text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let (decoded, claim) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    assert_eq!(decoded, typed);
    let verified = zil_verify::verify_module(decoded, claim, &target, &limits).unwrap();
    assert_eq!(verified.task_worker_ids().len(), 1);
    zil_lower::lower(&verified).unwrap();
    let worker = typed
        .functions
        .iter()
        .position(|f| f.name == "worker")
        .unwrap();
    let main = typed
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    for variant in 0..10 {
        let mut forged = typed.clone();
        match variant {
            0 => {
                let op = forged.functions[main]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskStart { .. }))
                    .unwrap();
                if let I::TaskStart { worker, .. } = &mut op.kind {
                    *worker = zil_ir::FunctionId(main as u32);
                }
            }
            1 => {
                let op = forged.functions[main]
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, I::TaskPoll { .. }))
                    .unwrap();
                let I::TaskPoll { task } = op.kind else {
                    unreachable!()
                };
                forged.functions[main].values[task.0 as usize] =
                    Type::Desc(Box::new(Type::TaskI32));
            }
            2 => {
                let op = forged.functions[worker]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskYield))
                    .unwrap();
                let value = op.result.unwrap();
                forged.functions[worker].values[value.0 as usize] = Type::Bool;
            }
            3 => forged.resources[0].teardown = Some(zil_ir::FunctionId(worker as u32)),
            4 => {
                let op = forged.functions[main]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskValid { .. }))
                    .unwrap();
                op.kind = I::FunctionRef {
                    function: zil_ir::FunctionId(worker as u32),
                };
                let result = op.result.unwrap();
                forged.functions[main].values[result.0 as usize] = Type::FunctionPointer {
                    parameters: vec![Type::I32],
                    result: Box::new(Type::I32),
                };
            }
            5 => {
                let op = forged.functions[main]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskPoll { .. }))
                    .unwrap();
                let I::TaskPoll { task: address } = op.kind else {
                    unreachable!()
                };
                op.kind = I::ReadAddress { address };
                let result = op.result.unwrap();
                forged.functions[main].values[result.0 as usize] =
                    Type::Managed(Box::new(Type::TaskI32));
            }
            6 => {
                let op = forged.functions[main]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskStart { .. }))
                    .unwrap();
                op.kind = I::MakeManaged { fields: vec![] };
            }
            7 => {
                forged.resources.clear();
            }
            8 => {
                let duplicate = forged.resources[0].clone();
                forged.resources.push(duplicate);
            }
            9 => {
                let op = forged.functions[main]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskValid { .. }))
                    .unwrap();
                op.kind = I::ForeignFunctionRef {
                    function: zil_ir::FunctionId(worker as u32),
                };
                let result = op.result.unwrap();
                forged.functions[main].values[result.0 as usize] = Type::ForeignCallback {
                    parameters: vec![Type::I32],
                    result: Box::new(Type::I32),
                };
            }
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "Analyze raw {variant}: {errors:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let encoded = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        for wire in [false, true] {
            let (input, claim) = if wire {
                zil_ir::wire::decode_module_candidate(&encoded, &limits).unwrap()
            } else {
                (forged.clone(), claim.clone())
            };
            let errors = zil_verify::verify_module(input, claim, &target, &limits).unwrap_err();
            assert_eq!(
                errors[0].category,
                ResultCategory::SafeReject,
                "Verify raw {variant} wire={wire}: {errors:?}"
            );
        }
    }
    for verify in [false, true] {
        let accepts = |steps| {
            let budget = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &budget).map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &budget).map(|_| ())
            }
        };
        let (mut low, mut high) = (0, limits.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if accepts(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        accepts(low).unwrap();
        let errors = accepts(low - 1).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "verify={verify}, N={low}: {errors:?}"
        );
        accepts(low).unwrap();
        eprintln!("task343 exact kernel verify={verify} N={low}");
    }
}

#[test]
#[cfg(target_os = "linux")]
fn async_registration_native_allocation_failure_drop_and_closed_destroy() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (index, text, failure, code, stdout) in [
                (
                    0,
                    include_str!("../../../tests/language-readiness/terminal/343/task-null.zil"),
                    true,
                    Some(0),
                    "AA",
                ),
                (
                    1,
                    include_str!(
                        "../../../tests/language-readiness/terminal/343/task-default-drop.zil"
                    ),
                    false,
                    Some(0),
                    "AVF",
                ),
                (
                    2,
                    include_str!(
                        "../../../tests/language-readiness/terminal/343/task-early-destroy.zil"
                    ),
                    false,
                    None,
                    "A",
                ),
                (
                    3,
                    include_str!(
                        "../../../tests/language-readiness/terminal/343/task-default-drop.zil"
                    ),
                    true,
                    Some(10),
                    "AN",
                ),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "zil343-task-failure-{}-{backend:?}-{optimization:?}-{index}",
                    std::process::id()
                ));
                fs::create_dir_all(&dir).unwrap();
                let observer = dir.join("observer.o");
                let compiled = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../tests/language-readiness/terminal/343/task-observer.c"),
                    )
                    .arg("-o")
                    .arg(&observer)
                    .output()
                    .unwrap();
                assert!(compiled.status.success(), "{compiled:?}");
                let wrapper = dir.join("link.sh");
                fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        clang: Some(wrapper),
                        stop_after: Stage::Build,
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
                assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let mut command = Command::new(&report.artifacts["executable"]);
                if failure {
                    command.env("ZIL343_TASK_ALLOC_FAIL", "1");
                }
                let actual = command.output().unwrap();
                assert_eq!(
                    actual.status.code(),
                    code,
                    "{backend:?} {optimization:?} {index}: {actual:?}"
                );
                assert_eq!(
                    actual.stdout,
                    stdout.as_bytes(),
                    "{backend:?} {optimization:?} {index}: {actual:?}"
                );
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

fn async_registration_export_program() -> zil_ir::TypedProgram {
    let src = source(include_str!(
        "../../../tests/language-readiness/terminal/343/task-export.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let worker = core
        .functions
        .iter()
        .find(|f| f.name == "worker")
        .unwrap()
        .id;
    zil_types::check_program(
        &zil_ir::CoreProgram {
            root: src.module_id,
            source_fingerprint: core.source_fingerprint,
            modules: vec![zil_ir::ProgramModule {
                core,
                imports: vec![],
                exports: vec![worker],
            }],
        },
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap()
}

#[test]
fn async_registration_public_semantics_are_independently_bound() {
    use std::sync::Arc;
    use zil_ir::{PassingMode, ScalarFunction, Type};
    let typed = async_registration_export_program();
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    assert_eq!(typed.task_interfaces.len(), 1);
    assert_eq!(
        typed.task_interfaces[0].exports[0].worker,
        zil_ir::FunctionId(0)
    );
    assert!(typed.interfaces.iter().all(|i| i.exports.is_empty()));
    let project = zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![source(include_str!(
            "../../../tests/language-readiness/terminal/343/task-export.zil"
        ))],
        paths: Default::default(),
    };
    let output = std::env::temp_dir().join(format!(
        "zil343-reject-suspend-entry-{}",
        std::process::id()
    ));
    let direct = zil_driver::compile_project(
        &project,
        &CompileOptions {
            entry: Some("worker".into()),
            stop_after: zil_driver::Stage::Build,
            output: Some(output.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        direct.status,
        ResultCategory::SafeReject,
        "{}",
        direct.to_json()
    );
    assert!(
        direct
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TASK-DIRECT-EXPORT"),
        "{}",
        direct.to_json()
    );
    assert!(!output.exists());
    let plan = zil_analyze::analyze_program(&typed, &limits).unwrap();
    let checked = zil_verify::verify_shared_program_module(
        Arc::new(typed.clone()),
        Arc::new(plan.clone()),
        &target,
        &limits,
    )
    .unwrap();
    assert!(checked.is_task_export(zil_ir::FunctionId(0)));
    assert_eq!(checked.task_interfaces(), typed.task_interfaces);
    let mut renamed = typed.clone();
    renamed.task_interfaces[0].exports[0].name = "alias".into();
    let alias = zil_verify::verify_shared_program_module(
        Arc::new(renamed),
        Arc::new(plan.clone()),
        &target,
        &limits,
    )
    .unwrap();
    assert_ne!(
        checked.task_abi_input_fingerprint(),
        alias.task_abi_input_fingerprint()
    );
    for variant in 0..7 {
        let mut forged = typed.clone();
        match variant {
            0 => forged.task_interfaces[0].exports[0].abi_version = 0,
            1 => forged.task_interfaces[0].exports[0].worker = zil_ir::FunctionId(2),
            2 => forged.task_interfaces[0].exports[0].parameter.1 = Type::Bool,
            3 => forged.task_interfaces[0].exports[0].result.0 = PassingMode::Copy,
            4 => {
                let mut e = forged.task_interfaces[0].exports[0].clone();
                e.name = "again".into();
                forged.task_interfaces[0].exports.push(e);
            }
            5 => {
                let duplicate = forged.task_interfaces[0].clone();
                forged.task_interfaces.push(duplicate);
            }
            6 => forged.interfaces[0].exports.push(ScalarFunction {
                storage_life: None,
                name: "worker".into(),
                function: zil_ir::FunctionId(0),
                parameters: vec![(PassingMode::Infer, Type::I32)],
                result: Type::I32,
                result_mode: PassingMode::Infer,
                reference_return_bound: None,
            }),
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze_program(&forged, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "Analyze {variant}: {errors:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&forged.module, &plan, &limits).unwrap();
        for decode in [false, true] {
            let mut input = forged.clone();
            let mut claim = plan.clone();
            if decode {
                (input.module, claim) =
                    zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
            }
            let errors = zil_verify::verify_shared_program_module(
                Arc::new(input),
                Arc::new(claim),
                &target,
                &limits,
            )
            .unwrap_err();
            assert_eq!(
                errors[0].category,
                ResultCategory::SafeReject,
                "Verify {variant} wire={decode}: {errors:?}"
            );
        }
    }
    for verify in [false, true] {
        let accepts = |steps| {
            let budget = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_shared_program_module(
                    Arc::new(typed.clone()),
                    Arc::new(plan.clone()),
                    &target,
                    &budget,
                )
                .map(|_| ())
            } else {
                zil_analyze::analyze_program(&typed, &budget).map(|_| ())
            }
        };
        let (mut low, mut high) = (0, limits.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if accepts(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        accepts(low).unwrap();
        let errors = accepts(low - 1).unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        accepts(low).unwrap();
        eprintln!("task343 public exact kernel verify={verify} N={low}");
    }
}

#[test]
#[cfg(target_os = "linux")]
fn async_registration_external_five_entry_sabi_and_physical_layout() {
    async_registration_run_external_sabi(false);
}

#[test]
#[cfg(target_os = "linux")]
fn async_child_external_child_sabi_and_physical_layout() {
    async_registration_run_external_sabi(true);
}

#[cfg(target_os = "linux")]
fn async_registration_run_external_sabi(child: bool) {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    fn c_type(ty: &str) -> String {
        match ty {
            "ptr" => "void *".into(),
            "i1" => "_Bool".into(),
            "i8" => "uint8_t".into(),
            "i16" => "uint16_t".into(),
            "i32" => "int32_t".into(),
            "i64" => "int64_t".into(),
            _ => {
                let body = ty
                    .strip_prefix("{ ")
                    .and_then(|s| s.strip_suffix(" }"))
                    .expect("closed literal frame type");
                let (mut depth, mut start, mut fields) = (0, 0, vec![]);
                for (offset, ch) in body.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        ',' if depth == 0 => {
                            fields.push(body[start..offset].trim());
                            start = offset + 1;
                        }
                        _ => {}
                    }
                }
                if !body.is_empty() {
                    fields.push(body[start..].trim());
                }
                format!(
                    "struct {{{}}}",
                    fields
                        .into_iter()
                        .enumerate()
                        .map(|(i, t)| format!("{} f{i};", c_type(t)))
                        .collect::<String>()
                )
            }
        }
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(if child {
        "../../tests/language-readiness/terminal/343/task-child-export-caller.c"
    } else {
        "../../tests/language-readiness/terminal/343/task-export-caller.c"
    });
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil343-task-export-{}-{child}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let observer = dir.join("caller.o");
            let built = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                .arg(&fixture)
                .arg("-o")
                .arg(&observer)
                .output()
                .unwrap();
            assert!(built.status.success(), "{built:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let project = zil_driver::ProjectSources {
                root: ModuleId(0),
                modules: vec![source(if child {
                    include_str!(
                        "../../../tests/language-readiness/terminal/343/task-child-export.zil"
                    )
                } else {
                    include_str!("../../../tests/language-readiness/terminal/343/task-export.zil")
                })],
                paths: Default::default(),
            };
            let options = CompileOptions {
                backend,
                optimization,
                clang: Some(wrapper),
                stop_after: Stage::Build,
                output: Some(dir.join("program")),
                emit_dir: Some(dir.join("artifacts")),
                object_cache: (backend == NativeBackend::LlvmApi).then(|| dir.join("objects")),
                ..CompileOptions::default()
            };
            let report = zil_driver::compile_project(&project, &options);
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let cg = report.native_codegen.as_ref().unwrap();
            assert_eq!(cg.backend, backend);
            assert!(cg.fallback.is_none());
            assert_eq!(cg.task_abis.len(), 1);
            let abi = &cg.task_abis[0];
            assert!(
                fs::read_to_string(&report.artifacts["task-abi"])
                    .unwrap()
                    .contains(&abi.codegen_key.to_hex())
            );
            assert_eq!(abi.worker, zil_ir::FunctionId(if child { 2 } else { 0 }));
            assert_eq!(abi.entrypoints.len(), 5);
            assert_eq!(abi.name, "worker");
            assert_eq!(abi.data_layout, zil_backend_llvm::TASK_DATA_LAYOUT);
            if !child
                && backend == NativeBackend::LlvmApi
                && optimization == Optimization::O2
                && let Some(path) = std::env::var_os("ZIL343_TASK_ABI_RECEIPT")
            {
                fs::write(path, format!("{}\n", abi.to_json())).unwrap();
            }
            assert_ne!(abi.codegen_key, zil_base::Fingerprint::ZERO);
            assert_ne!(abi.frame_layout_fingerprint, zil_base::Fingerprint::ZERO);
            let run = Command::new(&report.artifacts["executable"])
                .env("ZIL343_EXPECT_FRAME_SIZE", abi.frame_size.to_string())
                .output()
                .unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "{backend:?} {optimization:?}: {run:?}"
            );
            assert_eq!(
                Command::new(&report.artifacts["executable"])
                    .env("ZIL343_TASK_EXPORT_ALLOC_FAIL", "1")
                    .status()
                    .unwrap()
                    .code(),
                Some(3)
            );
            if backend == NativeBackend::LlvmApi {
                assert!(cg.object_cache.enabled);
                let warm = zil_driver::compile_project(&project, &options);
                assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
                let hit = warm.native_codegen.as_ref().unwrap();
                assert!(hit.object_cache.hits > 0, "{}", warm.to_json());
                assert_eq!(hit.task_abis[0], *abi);
                assert_eq!(
                    Command::new(&warm.artifacts["executable"])
                        .status()
                        .unwrap()
                        .code(),
                    Some(0)
                );
                let mut changed = project.clone();
                changed.modules[0] = source(&changed.modules[0].text.replace(
                    if child {
                        "seed+1"
                    } else {
                        "i32(wide)+i32(small)"
                    },
                    if child {
                        "seed+%1"
                    } else {
                        "i32(wide)+%i32(small)"
                    },
                ));
                let updated = zil_driver::compile_project(&changed, &options);
                assert_eq!(
                    updated.status,
                    ResultCategory::Success,
                    "{}",
                    updated.to_json()
                );
                let actual = updated.native_codegen.as_ref().unwrap();
                assert!(actual.object_cache.misses > 0);
                assert_ne!(
                    actual.task_abis[0].frame_layout_fingerprint,
                    abi.frame_layout_fingerprint
                );
                assert_ne!(actual.task_abis[0].codegen_key, abi.codegen_key);
                assert_eq!(
                    Command::new(&updated.artifacts["executable"])
                        .status()
                        .unwrap()
                        .code(),
                    Some(0)
                );
            }
            let ir = fs::read_to_string(&report.artifacts["llvm"])
                .unwrap_or_else(|_| fs::read_to_string(dir.join("program.ll")).unwrap());
            assert!(!ir.contains(&format!("define i32 @zil.fn.{}(", abi.worker.0)));
            if backend == NativeBackend::LlvmApi {
                assert!(
                    cg.object_reports
                        .iter()
                        .all(|r| r.contains(zil_backend_llvm::TASK_DATA_LAYOUT))
                );
            }
            let actual_layout = Command::new(&clang)
                .args([
                    "--target=x86_64-unknown-linux-gnu",
                    "-x",
                    "c",
                    "-S",
                    "-emit-llvm",
                    "/dev/null",
                    "-o",
                    "-",
                ])
                .output()
                .unwrap();
            assert!(actual_layout.status.success());
            assert!(
                String::from_utf8(actual_layout.stdout)
                    .unwrap()
                    .contains(&format!("target datalayout = \"{}\"", abi.data_layout))
            );
            let mut c="#include <stddef.h>\n#include <stdint.h>\n#include <stdbool.h>\ntypedef struct {\n".to_owned();
            for field in &abi.frame_fields {
                c.push_str(&format!(
                    "{} f{};\n",
                    c_type(&field.machine_type),
                    field.index
                ));
            }
            c.push_str("} Frame;\nint main(void){\n");
            c.push_str(&format!(
                "if(sizeof(Frame)!={} || _Alignof(Frame)!={}) return 1;\n",
                abi.frame_size, abi.frame_alignment
            ));
            for field in &abi.frame_fields {
                c.push_str(&format!(
                    "if(offsetof(Frame,f{})!={} || sizeof(((Frame*)0)->f{})!={}) return 2;\n",
                    field.index, field.offset, field.index, field.size
                ));
            }
            c.push_str("return 0;}\n");
            let probe = dir.join("layout.c");
            fs::write(&probe, c).unwrap();
            let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
                .arg(&probe)
                .arg("-o")
                .arg(dir.join("layout"))
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            assert!(Command::new(dir.join("layout")).status().unwrap().success());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn async_registration_returned_generic_owner_reaches_both_kernels() {
    let text =
        include_str!("../../../tests/language-readiness/terminal/343/registration-return.zil");
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn async_registration_returned_stable_owner_native_and_failures() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/terminal/343/registration-observer.c");
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (two, composed) in [(false, false), (true, false), (false, true)] {
                let fixture = if two {
                    fixture.with_file_name("registration-two-observer.c")
                } else {
                    fixture.clone()
                };
                let dir = std::env::temp_dir().join(format!(
                    "zil343-registration-{}-{backend:?}-{optimization:?}-{two}-{composed}",
                    std::process::id()
                ));
                fs::create_dir_all(&dir).unwrap();
                let observer = dir.join("observer.o");
                let c = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(&fixture)
                    .arg("-o")
                    .arg(&observer)
                    .output()
                    .unwrap();
                assert!(c.status.success(), "{c:?}");
                let wrapper = dir.join("link.sh");
                fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
                for default_drop in [false, true] {
                    let text = if composed {
                        include_str!(
                            "../../../tests/language-readiness/terminal/343/registration-composed.zil"
                        )
                    } else if two {
                        include_str!(
                            "../../../tests/language-readiness/terminal/343/registration-two.zil"
                        )
                    } else {
                        include_str!(
                            "../../../tests/language-readiness/terminal/343/registration-return.zil"
                        )
                    };
                    let text = if default_drop {
                        text.replace("registration_close(give owner);", "")
                            .replace("registration_close(give first);", "")
                            .replace("registration_close(give second);", "")
                    } else {
                        text.to_owned()
                    };
                    let project = zil_driver::ProjectSources {
                        root: ModuleId(0),
                        modules: vec![source(&text)],
                        paths: Default::default(),
                    };
                    let report = zil_driver::compile_project(
                        &project,
                        &CompileOptions {
                            backend,
                            optimization,
                            clang: Some(wrapper.clone()),
                            stop_after: Stage::Build,
                            output: Some(dir.join("program")),
                            emit_dir: Some(dir.join("artifacts")),
                            ..CompileOptions::default()
                        },
                    );
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
                    assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                    for failure in [
                        None,
                        Some("ZIL343_REG_ALLOC_FAIL"),
                        Some("ZIL343_REG_INSTALL_FAIL"),
                    ] {
                        if two && failure.is_some() {
                            continue;
                        }
                        let mut command = Command::new(&report.artifacts["executable"]);
                        if let Some(failure) = failure {
                            command.env(failure, "1");
                        }
                        let result = command.output().unwrap();
                        assert_eq!(
                            result.status.code(),
                            Some(if failure.is_some() { 3 } else { 0 }),
                            "{backend:?} {optimization:?} default_drop={default_drop} failure={failure:?}: {result:?}"
                        );
                    }
                }
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn async_registration_source_definition_moves_and_closed_environment_rejects() {
    let original =
        include_str!("../../../tests/language-readiness/terminal/343/registration-return.zil");
    let unused = original.split("fn main").next().unwrap().to_owned() + "fn main() {}";
    let neighbors=[
        unused.replace(" + ClosedEnvironment",""),
        original.replace("let owner=install(f)","let owner=#(c_register(registration_register,registration_close_backend,f))"),
        format!("trait ClosedEnvironment {{}}\n{unused}"),
        format!("struct Fake{{value:i32}} impl ClosedEnvironment for Fake {{}}\n{unused}"),
        unused.replace("FnMut(i32)->i32 + ClosedEnvironment","FnOnce(i32)->i32 + ClosedEnvironment"),
        original.replace("let f=move(value:i32)","let f=(value:i32)"),
        original.replace("if !registration_valid(owner)","let duplicate=owner; registration_close(give duplicate); if !registration_valid(owner)"),
        original.replace("registration_close(give owner);","registration_close(give owner); registration_close(give owner);"),
        original.replace("registration_close(give owner);","let view=owner; registration_close(give owner); registration_valid(view);"),
        original.replace("registration_close(give owner);","registration_close(owner);"),
        original.replace("return c_register(registration_register,registration_close_backend,f)","return c_register(registration_close_backend,registration_register,f)"),
        original.replace("unsafe { return c_register","{ return c_register"),
        original.replace("fn main()->i32 {","fn main()->i32 { let c_register=1;"),
        original.replace("c_register(registration_register,registration_close_backend,f)","c_register(registration_register,registration_close_backend,f); return c_register(registration_register,registration_close_backend,f)"),
    ];
    for (index, text) in neighbors.into_iter().enumerate() {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "neighbor{index}: {}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::InfraError,
            "neighbor{index}: {}",
            report.to_json()
        );
    }
}

#[test]
fn returned_registration_raw_wire_and_exact_kernel_budgets() {
    use zil_ir::{InstructionKind as I, Type};
    let src = source(include_str!(
        "../../../tests/language-readiness/terminal/343/registration-return.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let mut missing_bound = core.clone();
    missing_bound
        .functions
        .iter_mut()
        .find(|f| f.name == "install")
        .unwrap()
        .callable_parameters[0]
        .closed_environment = false;
    assert!(zil_types::check(&missing_bound, &target, &limits).is_err());
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let (decoded, claim) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    assert_eq!(decoded, typed);
    let checked = zil_verify::verify_module(decoded, claim, &target, &limits).unwrap();
    assert!(checked.uses_retained_registration());
    zil_lower::lower(&checked).unwrap();
    let start = typed
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, I::RegistrationStart { .. }))
        })
        .unwrap();
    let main = typed
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let resource = typed
        .resources
        .iter()
        .position(|r| matches!(r.payload, Type::Registration { .. }))
        .unwrap();
    for variant in 0..14 {
        let mut forged = typed.clone();
        match variant {
            0 => {
                let i = forged.functions[start]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::RegistrationStart { .. }))
                    .unwrap();
                if let I::RegistrationStart {
                    register, close, ..
                } = &mut i.kind
                {
                    *register = *close;
                }
            }
            1 => {
                let i = forged.functions[start]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::RegistrationStart { .. }))
                    .unwrap();
                if let I::RegistrationStart { region, .. } = &mut i.kind {
                    *region = u32::MAX;
                }
            }
            2 => {
                let i = forged.functions[start]
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, I::RegistrationStart { .. }))
                    .unwrap();
                let I::RegistrationStart { callback, .. } = i.kind else {
                    unreachable!()
                };
                forged.functions[start].values[callback.0 as usize] = Type::I32;
            }
            3 => forged.resources[resource].name = "@builtin:Registration:forged".into(),
            4 => {
                let mut duplicate = forged.resources[resource].clone();
                duplicate.name.push_str(":shadow");
                forged.resources.push(duplicate);
            }
            5 => {
                let i = forged.functions[start]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::RegistrationStart { .. }))
                    .unwrap();
                i.kind = I::MakeManaged { fields: vec![] };
            }
            6 => {
                let i = forged.functions[main]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::RegistrationValid { .. }))
                    .unwrap();
                let I::RegistrationValid { registration } = i.kind else {
                    unreachable!()
                };
                i.kind = I::ReadDesc {
                    desc: registration,
                    index: 0,
                };
                let v = i.result.unwrap();
                forged.functions[main].values[v.0 as usize] = Type::I32;
            }
            7 => {
                let desc = forged.functions[main]
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| {
                        if let I::RegistrationValid { registration } = i.kind {
                            Some(registration)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let i = forged.functions[main]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::RegistrationClose { .. }))
                    .unwrap();
                i.kind = I::RegistrationClose { registration: desc };
            }
            8 => forged.resources[resource].teardown = Some(zil_ir::FunctionId(main as u32)),
            9 => {
                let Type::Registration { environment } = &forged.resources[resource].payload else {
                    unreachable!()
                };
                let Type::Closure(id) = environment.as_ref() else {
                    unreachable!()
                };
                forged.closures[id.0 as usize].captures[0] = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                };
            }
            10 => {
                let f = &mut forged.functions[main];
                let b = f
                    .blocks
                    .iter_mut()
                    .find(|b| {
                        b.instructions
                            .iter()
                            .any(|i| matches!(i.kind, I::RegistrationClose { .. }))
                    })
                    .unwrap();
                let n = b
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, I::RegistrationClose { .. }))
                    .unwrap();
                let extra = b.instructions[n].clone();
                b.instructions.insert(n, extra);
            }
            11 => {
                let Type::Registration { environment } = &mut forged.resources[resource].payload
                else {
                    unreachable!()
                };
                **environment = Type::ForeignHandle;
            }
            12 => {
                let parameter = forged.functions[start].parameters[0];
                let inner = forged.functions[start].locals[parameter.0 as usize]
                    .ty
                    .clone();
                forged.functions[start].locals[parameter.0 as usize].ty = Type::Address {
                    mutable: true,
                    inner: Box::new(inner),
                };
            }
            13 => {
                let f = &mut forged.functions[start];
                let original = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, I::RegistrationStart { .. }))
                    .unwrap()
                    .clone();
                let old = original.result.unwrap();
                let second = zil_ir::ValueId(f.values.len() as u32);
                f.values.push(f.values[old.0 as usize].clone());
                let unit = zil_ir::ValueId(f.values.len() as u32);
                f.values.push(Type::Unit);
                for b in &mut f.blocks {
                    if let Some(at) = b
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, I::RegistrationStart { .. }))
                    {
                        let mut duplicate = original.clone();
                        duplicate.result = Some(second);
                        b.instructions.insert(at + 1, duplicate);
                        b.instructions.insert(
                            at + 2,
                            zil_ir::Instruction {
                                result: Some(unit),
                                kind: I::RegistrationClose { registration: old },
                                span: original.span,
                            },
                        );
                    }
                    if b.terminator == zil_ir::Terminator::Return(Some(old)) {
                        b.terminator = zil_ir::Terminator::Return(Some(second));
                    }
                }
            }
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "Analyze raw{variant}: {errors:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        for decoded in [false, true] {
            let (input, claim) = if decoded {
                zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
            } else {
                (forged.clone(), claim.clone())
            };
            let errors = zil_verify::verify_module(input, claim, &target, &limits).unwrap_err();
            assert_eq!(
                errors[0].category,
                ResultCategory::SafeReject,
                "Verify raw{variant} decoded={decoded}: {errors:?}"
            );
        }
    }
    for verify in [false, true] {
        let accepts = |steps| {
            let budget = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &budget).map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &budget).map(|_| ())
            }
        };
        let (mut low, mut high) = (0, limits.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if accepts(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        accepts(low).unwrap();
        assert_eq!(
            accepts(low - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        accepts(low).unwrap();
        eprintln!("registration343 exact kernel verify={verify} N={low}");
    }
}

#[test]
fn async_registration_copy_instances_and_return_branch_neighbors() {
    let original =
        include_str!("../../../tests/language-readiness/terminal/343/registration-return.zil");
    let declarations = original.split("fn install").next().unwrap();
    let copy = format!(
        "{declarations}\nfn main()->i32 {{let base=37;let f=move(x:i32)->i32=>x+base;unsafe{{let first=c_register(registration_register,registration_close_backend,f);let second=c_register(registration_register,registration_close_backend,f);registration_close(give first);registration_close(give second);}}return 0}}"
    );
    let branch=original.replace("(f:F)->Registration<F>","(f:F,choice:bool)->Registration<F>").replace("unsafe { return c_register(registration_register,registration_close_backend,f) }","unsafe { if choice { return c_register(registration_register,registration_close_backend,f) } return c_register(registration_register,registration_close_backend,f) }").replace("install(f)","install(f,true)");
    for text in [copy, branch] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn async_registration_source_library_preserves_actual_returned_environment() {
    let original =
        include_str!("../../../tests/language-readiness/terminal/343/registration-return.zil");
    let helper = original
        .split("fn install")
        .nth(1)
        .unwrap()
        .split("fn main")
        .next()
        .unwrap();
    let roles = original.lines().take(2).collect::<Vec<_>>().join("\n");
    let library = format!("{roles}\npub fn install{helper}");
    let prefix = original
        .split("fn install")
        .next()
        .unwrap()
        .lines()
        .skip(2)
        .collect::<Vec<_>>()
        .join("\n");
    let main = original.split("fn main").nth(1).unwrap();
    let caller = format!("import registrar\n{prefix}\nfn main{main}")
        .replace("install(f)", "registrar.install(f)");
    let project = zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            source(&caller),
            SourceUnit::new(FileId(1), ModuleId(1), "registrar.zil", &library),
        ],
        paths: std::collections::BTreeMap::from([(vec!["registrar".into()], ModuleId(1))]),
    };
    let report = zil_driver::compile_project(&project, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn async_registration_composed_code_capture_keeps_actual_data_and_life_boundaries() {
    let positive =
        include_str!("../../../tests/language-readiness/terminal/343/registration-composed.zil");
    let report = compile_source(&source(positive), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    for (name, text) in [
        (
            "address",
            include_str!(
                "../../../tests/language-readiness/terminal/343/registration-address-capture.zil"
            ),
        ),
        (
            "desc",
            include_str!(
                "../../../tests/language-readiness/terminal/343/registration-desc-capture.zil"
            ),
        ),
        (
            "slice",
            include_str!(
                "../../../tests/language-readiness/terminal/343/registration-slice-capture.zil"
            ),
        ),
        (
            "wrong-root",
            include_str!(
                "../../../tests/language-readiness/terminal/343/registration-bfp-wrong-result-root.zil"
            ),
        ),
        (
            "suspend",
            include_str!(
                "../../../tests/language-readiness/terminal/343/registration-transitive-suspend.zil"
            ),
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        eprintln!(
            "registration composition negative {name}: {}",
            report.to_json()
        );
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{name}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
#[cfg(target_os = "linux")]
fn async_child_native_static_frames_resume_and_cancel() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let original = include_str!("../../../tests/language-readiness/terminal/343/task-child.zil");
    let prefix = original.split("fn main").next().unwrap();
    let mut sources = vec![original.to_owned()];
    for after in 0..=8 {
        sources.push(format!(
            "{prefix}
fn main()->i32 {{
    var task=task_start(worker,7)
    if !task_valid(task) {{return 10}}
    var count=0
    while count<{after} {{
        let (status,value)=task_poll(&mut task)
        if status!=0 || value!=0 {{return 11}}
        count=count+1
    }}
    task_request_cancel(&mut task)
    task_request_cancel(&mut task)
    let (status,value)=task_poll(&mut task)
    if status!=3 || value!=0 {{return 12}}
    if !task_poll_close(&mut task) {{return 13}}
    task_finish_destroy(give task)
    return 0
}}"
        ));
    }
    sources.push("fn plain(x:i32)->i32=>x+1
fn empty(x:i32)->i32 contract {effects(suspend);resume(local)} {return x+2}
fn worker(x:i32)->i32 {let a=task_await(plain,x);return task_await(empty,a)}
fn main()->i32 {var t=task_start(worker,7);let direct=plain(7);let (s,v)=task_poll(&mut t);if s!=1 || v!=10 || direct!=8{return 1} if !task_poll_close(&mut t){return 2} task_finish_destroy(give t);return 0}".to_owned());
    for (case, text) in sources.iter().enumerate() {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil343-child-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "case {case}: {}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(0),
                    "case {case} {backend:?} {optimization:?}: {actual:?}"
                );
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn async_child_source_and_raw_effect_boundaries() {
    use zil_ir::{CallKind, InstructionKind as I, Type};
    for text in [
        "fn worker(x:i32)->i32 contract{effects()} {task_yield();return x} fn main(){}",
        "fn worker(x:i32)->i32 contract{effects(alloc)} {task_yield();return x} fn main(){}",
        "fn worker(x:i32)->i32 contract{effects(direct)} {return x} fn main(){}",
        "fn worker(x:i32)->i32 contract{effects(alloc);resume(local)} {return x} fn main(){}",
        "fn worker(x:i32)->i32 contract{effects(suspend);effects(suspend)} {return x} fn main(){}",
        "fn worker(x:i32)->i32 contract{resume(transfer)} {return x} fn main(){}",
        "fn worker(x:i32)->i32 contract{resume(local)} {return x} fn main()->i32=>worker(7)",
        "fn worker(x:i32)->i32 contract{effects(suspend)} {return x} fn main()->i32{let f:fn(i32)->i32=worker;return f(7)}",
        "fn worker(x:i32)->i32 contract{effects(suspend)} {return x} fn main(){let f=c_callback(worker)}",
        "fn worker(x:i32)->i32 contract{effects(suspend)} {return x} fn main()->i32{let r=thread_scope(worker,7,()->i32=>0);return r.1}",
        "fn worker(x:i32)->i32 {return task_await(worker,x)} fn main(){}",
        "fn unused<F:Fn(i32)->i32>(f:F,x:i32)->i32 contract{effects()} {task_yield();return x} fn main(){}",
        "fn worker(x:i32)->i32 contract{effects(suspend)} {return x} fn unused<F:Fn(i32)->i32>(f:F,x:i32)->i32 contract{effects()} {return worker(x)} fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::InfraError,
            "{}",
            report.to_json()
        );
    }
    let src = source(include_str!(
        "../../../tests/language-readiness/terminal/343/task-child.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let encoded = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let (decoded, claim) = zil_ir::wire::decode_module_candidate(&encoded, &limits).unwrap();
    assert_eq!(decoded, typed);
    let verified = zil_verify::verify_module(decoded, claim, &target, &limits).unwrap();
    assert!(verified.is_suspend(zil_ir::FunctionId(2)));
    assert_eq!(verified.task_worker_ids().len(), 3);
    zil_lower::lower(&verified).unwrap();
    for case in 0..9 {
        let mut raw = typed.clone();
        if case == 0 {
            raw.functions[2].call_kind = CallKind::Direct;
        } else if case == 1 {
            raw.functions[0].call_kind = CallKind::Direct;
        } else {
            let current = &mut raw.functions[2];
            let op = current
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, I::SuspendCall { .. }))
                .unwrap();
            let I::SuspendCall {
                function,
                arguments,
            } = &mut op.kind
            else {
                unreachable!()
            };
            match case {
                2 => {
                    op.kind = I::Call {
                        function: *function,
                        arguments: arguments.clone(),
                    }
                }
                3 => *function = zil_ir::FunctionId(2),
                4 => arguments.clear(),
                5 => arguments.push(arguments[0]),
                6 => current.values[op.result.unwrap().0 as usize] = Type::Bool,
                7 => *function = zil_ir::FunctionId(u32::MAX),
                8 => *function = zil_ir::FunctionId(3),
                _ => unreachable!(),
            }
        }
        let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "Analyze raw {case}: {errors:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        for wire in [false, true] {
            let (input, claim) = if wire {
                zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
            } else {
                (raw.clone(), claim.clone())
            };
            let errors = zil_verify::verify_module(input, claim, &target, &limits).unwrap_err();
            assert_eq!(
                errors[0].category,
                ResultCategory::SafeReject,
                "Verify raw {case} Wire={wire}: {errors:?}"
            );
        }
    }
    for verify in [false, true] {
        let accepts = |steps| {
            let bounds = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &bounds).map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &bounds).map(|_| ())
            }
        };
        let (mut low, mut high) = (1, limits.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if accepts(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        accepts(low).unwrap();
        assert_eq!(
            accepts(low - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        accepts(low).unwrap();
        eprintln!("child343 exact kernel verify={verify} N={low}");
    }
}

#[test]
fn async_child_life_transport_is_not_silently_admitted() {
    use zil_ir::{FunctionLifeContract, StorageLifeContract};
    let src = source(
        "fn apply<F:Fn(i32)->i32>(f:F,x:i32)->i32 {return f(x)} fn main()->i32 {let f=(x:i32)->i32=>x+1;return apply(f,7)}",
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let relation = StorageLifeContract::from_flat(FunctionLifeContract {
        binder_count: 0,
        parameter_lives: vec![None],
        result_life: None,
    })
    .unwrap();
    let mut header = core.clone();
    header
        .functions
        .iter_mut()
        .find(|f| !f.callable_parameters.is_empty())
        .unwrap()
        .callable_parameters[0]
        .life = Some(relation.clone());
    assert!(zil_types::check(&header, &target, &limits).is_err());
    let mut raw = typed;
    raw.functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap()
        .roles[0]
        .life = Some(relation);
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    let mut claim = plan;
    claim.module_fingerprint = raw.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    let (input, claim) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert!(zil_verify::verify_module(input, claim, &target, &limits).is_err());
}

#[test]
fn async_child_effect_free_suspend_interface_and_incremental_identity() {
    use zil_ir::{CallKind, CoreProgram, ProgramModule};
    let src =
        source("pub fn worker(x:i32)->i32 {let a=x+1;let b=a+2;let c=b+3;return c} fn main(){}");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let worker = core.functions[0].id;
    let original = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            core,
            imports: vec![],
            exports: vec![worker],
        }],
    };
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let mut session = zil_types::FunctionSession::default();
    let prepared = zil_types::infer_program_for_ct(&original, &limits)
        .unwrap()
        .into_prepared();
    let direct = session
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    assert!(direct.task_interfaces.is_empty());
    session
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    assert!(session.last_stats().reused_functions > 0);
    // Raw header mutation deliberately preserves source fingerprint/body/spans:
    // the declaration field itself must invalidate the Types work product.
    let mut changed = original;
    changed.modules[0].core.functions[0].call_kind = Some(CallKind::Suspend);
    let prepared = zil_types::infer_program_for_ct(&changed, &limits)
        .unwrap()
        .into_prepared();
    let suspended = session
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    assert_eq!(session.last_stats().reused_functions, 0);
    assert_eq!(
        suspended.task_interfaces[0].exports[0].worker,
        zil_ir::FunctionId(0)
    );
    assert!(suspended.interfaces[0].exports.is_empty());
    let plan = zil_analyze::analyze_program(&suspended, &limits).unwrap();
    let verified = zil_verify::verify_shared_program_module(
        std::sync::Arc::new(suspended.clone()),
        std::sync::Arc::new(plan.clone()),
        &target,
        &limits,
    )
    .unwrap();
    assert!(verified.is_suspend(zil_ir::FunctionId(0)));
    let mut forged = suspended;
    forged.task_interfaces.clear();
    forged.interfaces[0].exports = direct.interfaces[0].exports.clone();
    assert!(zil_analyze::analyze_program(&forged, &limits).is_err());
    assert!(
        zil_verify::verify_shared_program_module(
            std::sync::Arc::new(forged),
            std::sync::Arc::new(plan),
            &target,
            &limits
        )
        .is_err()
    );
    let report = compile_source(
        &source("fn worker(x:i32)->i32 contract{resume(local)} {return x+1} fn main(){}"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn async_child_native_allocation_failure_and_embedded_cleanup() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    let original = include_str!("../../../tests/language-readiness/terminal/343/task-child.zil");
    let prefix = original.split("fn main").next().unwrap();
    let mut cases = vec![original.to_owned()];
    for explicit in [false, true] {
        cases.push(format!("{prefix}
fn main()->i32 {{var t=task_start(worker,7);if !task_valid(t){{return 10}}
let (status,value)=task_poll(&mut t);if status!=0 || value!=0{{return 11}}
{} return 0}}",if explicit{"task_request_cancel(&mut t);if !task_poll_close(&mut t){return 12} task_finish_destroy(give t);"}else{""}));
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil343-child-fault-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let object = dir.join("observer.o");
            let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/language-readiness/terminal/343/task-observer.c"),
                )
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&object),quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            for (case, text) in cases.iter().enumerate() {
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        clang: Some(wrapper.clone()),
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "case {case}: {}",
                    report.to_json()
                );
                let cg = report.native_codegen.as_ref().unwrap();
                assert_eq!(cg.backend, backend);
                assert!(cg.fallback.is_none());
                for failure in [false, true] {
                    let mut command = Command::new(&report.artifacts["executable"]);
                    if failure {
                        command.env("ZIL343_TASK_ALLOC_FAIL", "1");
                    }
                    let actual = command.output().unwrap();
                    assert_eq!(
                        actual.status.code(),
                        Some(if failure { 10 } else { 0 }),
                        "case {case} {backend:?} {optimization:?} failure={failure}: {actual:?}"
                    );
                    assert_eq!(
                        actual.stdout,
                        if failure {
                            b"A".as_slice()
                        } else {
                            b"AF".as_slice()
                        },
                        "only the parent owns an allocation"
                    );
                }
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn async_registration_scalar_library_helpers_keep_sealed_identity() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let main = source(include_str!(
        "../../../tests/language-readiness/terminal/343/sync-probes/library/main.zil"
    ));
    let library = include_str!(
        "../../../tests/language-readiness/terminal/343/sync-probes/library/sync_ops.zil"
    );
    let project = zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            main,
            SourceUnit::new(FileId(1), ModuleId(1), "sync_ops.zil", library),
        ],
        paths: std::collections::BTreeMap::from([(vec!["sync_ops".into()], ModuleId(1))]),
    };
    let report = zil_driver::compile_project(&project, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let mut fingerprints = vec![];
    for cell in ["AtomicI32", "MutexI32"] {
        let src = source(&format!(
            "pub fn observe(cell:&{cell})->i32 {{return 0}} fn main(){{}}"
        ));
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let core = zil_expand::expand(&src, &ast).unwrap();
        let export = core.functions[0].id;
        let typed = zil_types::check_program(
            &zil_ir::CoreProgram {
                root: ModuleId(0),
                source_fingerprint: core.source_fingerprint,
                modules: vec![zil_ir::ProgramModule {
                    core,
                    imports: vec![],
                    exports: vec![export],
                }],
            },
            &zil_base::TargetProfile::default(),
            &Limits::default(),
        )
        .unwrap();
        fingerprints.push(typed.interfaces[0].signature_fingerprint);
    }
    assert_ne!(fingerprints[0], fingerprints[1]);
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil343-sync-library-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let report = zil_driver::compile_project(
                &project,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
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
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let actual = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                actual.status.code(),
                Some(0),
                "{backend:?} {optimization:?}: {actual:?}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[path = "../../../tests/language-readiness/terminal/343/task_general.rs"]
mod general_async_registration;

#[path = "../../../tests/language-readiness/task-composition/G8.6.1/verify_frames.rs"]
mod task_verify_frame_verify_frames;

#[path = "../../../tests/language-readiness/terminal/343/cas.rs"]
mod cas;

#[test]
fn terminal_life_values_life_values_keep_default_worker_stack() {
    const CHILD: &str = "ZIL_G10_1_7_STACK_CHILD";
    if let Ok(mode) = std::env::var(CHILD) {
        std::thread::spawn(move || {
            let directory = std::env::temp_dir().join(format!("zil-terminal-life-values-{}-{mode}", std::process::id()));
            std::fs::create_dir_all(&directory).unwrap();
            for phantom in [false, true] {
                for copy in [false, true] {
                    let depths: &[usize] = if mode == "binary" {
                        // Left-associated binary syntax remains parseable past
                        // the call-nesting boundary; keep its nearby cases too.
                        &[32, 64, 96, 112, 124, 126, 192, 240]
                    } else {
                        &[32, 64, 96, 112, 124, 126]
                    };
                    for &depth in depths {
                        let expression = if mode == "binary" {
                            std::iter::repeat_n("1", depth).collect::<Vec<_>>().join("+")
                        } else {
                            let call = match mode.as_str() { "generic" => "same", "conversion" => "i32", _ => "plain" };
                            format!("{}0{}", format!("{call}(").repeat(depth), ")".repeat(depth))
                        };
                        let gate = if phantom { "struct Phantom<life s>{n:i32}" } else { "" };
                        let (generics, parameters, arguments) = if copy {
                            ("life a,T:Copy", "p:&'a i32,witness:T", "&n,n")
                        } else { ("life a", "p:&'a i32", "&n") };
                        let text = format!("{gate}fn same<T:Copy>(x:T)->T{{return x}}fn plain(x:i32)->i32{{return x}}fn value<{generics}>({parameters})->i32{{return {expression}}}fn main()->i32{{let n=0;return value({arguments})}}");
                        let path = directory.join(format!("{phantom}-{copy}-{depth}.zil"));
                        std::fs::write(&path, text).unwrap();
                        let options = zil_driver::CompileOptions { stop_after: zil_driver::Stage::Verify, ..Default::default() };
                        let report = zil_driver::compile_path(&path, &options);
                        let exhausted = depth == 126 && mode != "binary";
                        let expected = if exhausted { ResultCategory::ResourceLimit } else { ResultCategory::Success };
                        assert_eq!(report.status, expected, "{mode}/{phantom}/{copy}/{depth}: {}", report.to_json());
                        assert_eq!(report.verification_runs, usize::from(!exhausted));
                        if exhausted { assert_eq!(report.phase, zil_base::Phase::Parse); }
                    }
                }
            }
        }).join().unwrap();
        return;
    }
    for mode in ["plain", "generic", "binary", "conversion"] {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "terminal_life_values_life_values_keep_default_worker_stack",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("RUST_MIN_STACK")
            .env_remove("LD_PRELOAD")
            .env(CHILD, mode)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{mode}: {} {}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
    }
}

#[test]
fn public42_unused_roles_and_unauthorized_pollable_candidates_raw_and_wire() {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    use zil_ir::{InstructionKind as I, ScopedCallbackRole as R, Type};
    let text = r#"
extern "C" fn register_slot(cb:extern "C" fn(c_handle,i32)->i64,context:c_handle)->c_handle contract{provides(static_callbacks());effects(ffi);};
extern "C" fn request_slot(token:c_handle) contract{provides(static_callbacks());effects(ffi);};
extern "C" fn drain_slot(token:c_handle)->i32 contract{provides(static_callbacks());effects(ffi);};
fn main()->i32{unsafe{let callback=(value:i32)->i32=>value+1;return callback(1);}}
"#;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let unit = source(text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let aliases = Default::default();
    let core = zil_expand::expand_with_map_limits_target_context(
        &unit,
        &syntax,
        &limits,
        &target,
        SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &aliases,
            declarations: DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let (decoded, claim) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_analyze::analyze(&decoded, &limits).unwrap();
    zil_verify::verify_module(decoded, claim, &target, &limits).unwrap();
    // The current stored-nominal profile excludes foreign declaration bodies,
    // so give its unused actual table an independently successful source root.
    let nominal_text = "struct Note<T:Copy,life a>{n:i32} fn nominal<life a>(input:&'a i32)->i32{let item:Note<i32,'a>=Note::<i32,'a>{n:*input};return item.n;} fn main()->i32{let callback=(value:i32)->i32=>value+1;let input=1;return callback(nominal(&input));}";
    for control in [text, nominal_text] {
        let report = compile_source(&source(control), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    let nominal_unit = source(nominal_text);
    let nominal_syntax = zil_syntax::parse(&nominal_unit).into_result().unwrap();
    let nominal_core = zil_expand::expand_with_map_limits_target_context(
        &nominal_unit,
        &nominal_syntax,
        &limits,
        &target,
        SourceIdentityContext::ClosedSnapshot {
            root: nominal_unit.module_id,
            aliases: &aliases,
            declarations: DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let nominal_typed = zil_types::check(&nominal_core, &target, &limits).unwrap();
    let nominal_plan = zil_analyze::analyze(&nominal_typed, &limits).unwrap();
    zil_verify::verify_module(
        nominal_typed.clone(),
        nominal_plan.clone(),
        &target,
        &limits,
    )
    .unwrap();
    let function = |name: &str| typed.functions.iter().position(|f| f.name == name).unwrap();
    let main = function("main");
    let register = function("register_slot");
    let request = function("request_slot");
    let drain = function("drain_slot");
    let closure = typed.closures[0].id;
    let payload = Type::AsyncRegistration {
        environment: Box::new(Type::Closure(closure)),
    };
    let callback = typed.functions[main]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, I::MakeClosure { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let region = typed.functions[main]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let I::UnsafeEnter { region } = i.kind {
                Some(region)
            } else {
                None
            }
        })
        .unwrap();
    for case in 0..8 {
        let (mut raw, mut claim) = if case == 5 {
            (nominal_typed.clone(), nominal_plan.clone())
        } else {
            (typed.clone(), plan.clone())
        };
        match case {
            0..=2 => {
                let (index, role) = [
                    (register, R::AsyncRegister),
                    (request, R::AsyncRequestClose),
                    (drain, R::AsyncPollDrain),
                ][case];
                raw.functions[index]
                    .foreign
                    .as_mut()
                    .unwrap()
                    .scoped_callback = Some(role);
            }
            3 => {
                let f = &mut raw.functions[main];
                f.locals.push(zil_ir::LocalDecl {
                    id: zil_ir::LocalId(f.locals.len() as u32),
                    name: "unused_pollable_head".into(),
                    ty: Type::Managed(Box::new(payload.clone())),
                    mutable: false,
                    span: f.span,
                });
            }
            4 => raw.resources.push(zil_ir::ResourceDefinition {
                name: "@fixture:unused-pollable-registration".into(),
                payload: payload.clone(),
                teardown: None,
            }),
            5 => {
                let actual = raw
                    .life_instances
                    .iter_mut()
                    .flat_map(|instance| &mut instance.arguments)
                    .find_map(|a| {
                        if let zil_ir::LifeNominalActual::Type(t) = a {
                            Some(t)
                        } else {
                            None
                        }
                    })
                    .expect("real source retains its phantom type argument");
                *actual = Type::AsyncRegistration {
                    environment: Box::new(Type::Closure(raw.closures[0].id)),
                };
            }
            6 | 7 => {
                // Isolate the operation guard: retain the real callback/provider
                // IDs but introduce no Async result type that would mask it.
                // This raw candidate is never claimed as source admission.
                let i = zil_ir::Instruction {
                    result: None,
                    span: raw.functions[main].span,
                    kind: I::AsyncRegistrationStart {
                        register: raw.functions[register].id,
                        request_close: raw.functions[request].id,
                        poll_drain: raw.functions[drain].id,
                        callback,
                        region,
                    },
                };
                if case == 6 {
                    let b = raw.functions[main]
                        .blocks
                        .iter_mut()
                        .find(|b| {
                            b.instructions
                                .iter()
                                .any(|i| matches!(i.kind, I::UnsafeExit { .. }))
                        })
                        .unwrap();
                    let at = b
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, I::UnsafeExit { .. }))
                        .unwrap();
                    b.instructions.insert(at, i);
                } else {
                    let f = &mut raw.functions[main];
                    let id = zil_ir::BlockId(f.blocks.len() as u32);
                    f.blocks.push(zil_ir::BasicBlock {
                        id,
                        instructions: vec![],
                        terminator: zil_ir::Terminator::Trap,
                    });
                    claim.functions[main].blocks.push(zil_ir::BlockPlan {
                        block: id,
                        initialized_in: vec![],
                        initialized_out: vec![],
                    });
                    // The extra block itself and its truthful plan must pass
                    // before adding the unauthorized async opcode into that block.
                    claim.module_fingerprint = raw.fingerprint();
                    zil_analyze::analyze(&raw, &limits).unwrap();
                    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits)
                        .unwrap();
                    raw.functions[main]
                        .blocks
                        .last_mut()
                        .unwrap()
                        .instructions
                        .push(i);
                }
            }
            _ => unreachable!(),
        }
        claim.module_fingerprint = raw.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        let (decoded, wire_claim) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        for (wire, input, candidate) in [(false, raw, claim), (true, decoded, wire_claim)] {
            if case <= 2 {
                // Role declarations carry unsafe promises; unused valid roles
                // are admitted independently of constructor/owner authority.
                zil_analyze::analyze(&input, &limits).unwrap();
                zil_verify::verify_module(input, candidate, &target, &limits).unwrap();
                continue;
            }
            for (kernel, errors) in [
                (
                    "Analyze",
                    zil_analyze::analyze(&input, &limits).unwrap_err(),
                ),
                (
                    "Verify",
                    zil_verify::verify_module(input, candidate, &target, &limits).unwrap_err(),
                ),
            ] {
                assert!(
                    errors.iter().all(|d| matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )),
                    "case={case} wire={wire} kernel={kernel}: {errors:?}"
                );
                eprintln!(
                    "public42 refusal case={case} wire={wire} kernel={kernel}: {} {}",
                    errors[0].code, errors[0].message
                );
            }
        }
    }
}

#[test]
fn async_registration_explicit_source_smoke() {
    let src = source(include_str!(
        "../../../tests/language-readiness/async-registration/explicit.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let (raw, claim) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    zil_verify::verify_module(raw, claim, &target, &limits).unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn async_registration_task_queued_cancel_native() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-async841-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let observer = dir.join("host.o");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let early_async = include_str!(
                "../../../tests/language-readiness/async-registration/queued-export.zil"
            )
            .replace(
                "task_yield();",
                "task_yield();async_registration_finish_destroy(give owner);",
            );
            let child_sources: Vec<_> = [
                include_str!("../../../tests/language-readiness/async-registration/queued-cancel.zil"),
                include_str!("../../../tests/language-readiness/async-registration/queued-return.zil"),
                include_str!("../../../tests/language-readiness/async-registration/queued-result.zil"),
            ].into_iter().enumerate().map(|(index, source)| {
                let wrapper = if index == 2 {
                    "fn worker(seed:i32)->Answer contract{result(create);} {let answer=task_await(child,seed);return give answer}\n"
                } else {
                    "fn worker(seed:i32)->i32 {return task_await(child,seed)}\n"
                };
                source.replace("fn worker(seed:i32)", "fn child(seed:i32)").replace("fn drive(", &format!("{wrapper}fn drive("))
            }).collect();
            for (mode, text) in [
                ("child_cancel", child_sources[0].as_str()),
                ("child_return", child_sources[1].as_str()),
                ("child_result", child_sources[2].as_str()),
                (
                    "cancel",
                    include_str!(
                        "../../../tests/language-readiness/async-registration/queued-cancel.zil"
                    ),
                ),
                (
                    "return",
                    include_str!(
                        "../../../tests/language-readiness/async-registration/queued-return.zil"
                    ),
                ),
                (
                    "result",
                    include_str!(
                        "../../../tests/language-readiness/async-registration/queued-result.zil"
                    ),
                ),
                (
                    "export",
                    include_str!(
                        "../../../tests/language-readiness/async-registration/queued-export.zil"
                    ),
                ),
                ("early_async", early_async.as_str()),
            ] {
                let c = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(
                        if mode == "export" || mode == "early_async" {
                            "../../tests/language-readiness/async-registration/queued-export-host.c"
                        } else {
                            "../../tests/language-readiness/async-registration/queued-host.c"
                        },
                    ))
                    .arg("-o")
                    .arg(&observer)
                    .output()
                    .unwrap();
                assert!(c.status.success(), "{c:?}");
                let report = zil_driver::compile_project(
                    &zil_driver::ProjectSources {
                        root: ModuleId(0),
                        modules: vec![source(text)],
                        paths: Default::default(),
                    },
                    &CompileOptions {
                        backend,
                        optimization,
                        clang: Some(wrapper.clone()),
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        emit_dir: Some(dir.join("artifacts")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let runs: Vec<Option<(&str, &str)>> = if mode == "export" {
                    vec![
                        None,
                        Some(("ZIL_ASYNC_EXPORT_RETURN", "1")),
                        Some(("ZIL_ASYNC_ALLOC_FAIL", "1")),
                        Some(("ZIL_ASYNC_ALLOC_FAIL", "2")),
                        Some(("ZIL_ASYNC_INSTALL_FAIL", "1")),
                        Some(("ZIL_ASYNC_INVALID_DRAIN", "1")),
                        Some(("ZIL_ASYNC_EARLY_TASK_FINISH", "1")),
                    ]
                } else {
                    vec![None]
                };
                for failure in runs {
                    let mut command = Command::new(&report.artifacts["executable"]);
                    if ["return", "result", "child_return", "child_result"].contains(&mode) {
                        command.env("ZIL_ASYNC_NORMAL", "1");
                    }
                    if mode == "result" || mode == "child_result" {
                        command.env("ZIL_ASYNC_RESULT", "1");
                    }
                    if let Some((key, value)) = failure {
                        command.env(key, value);
                    }
                    let traps = mode == "early_async"
                        || failure.is_some_and(|(key, _)| {
                            key == "ZIL_ASYNC_INVALID_DRAIN" || key == "ZIL_ASYNC_EARLY_TASK_FINISH"
                        });
                    if mode == "early_async" {
                        command.env("ZIL_ASYNC_EARLY_ASYNC_FINISH", "1");
                    }
                    if traps {
                        command.env("ZIL_ASYNC_TRACE", "1");
                    }
                    let actual = command.output().unwrap();
                    if traps {
                        assert_eq!(
                            actual.status.code(),
                            None,
                            "{backend:?} {optimization:?} mode={mode} failure={failure:?}: {actual:?}"
                        );
                        assert!(
                            !actual.stdout.contains(&b'F') && !actual.stdout.contains(&b'D'),
                            "no free or environment Drop before Closed: {actual:?}"
                        );
                        assert!(
                            String::from_utf8_lossy(&actual.stdout).contains("live=2 drops=0"),
                            "{actual:?}"
                        );
                    } else {
                        assert_eq!(
                            actual.status.code(),
                            Some(0),
                            "{backend:?} {optimization:?} mode={mode} failure={failure:?}: {actual:?}"
                        );
                        let expected = match failure {
                            Some(("ZIL_ASYNC_ALLOC_FAIL", "1")) => {
                                "task allocation failure; no installed source\n"
                            }
                            Some(("ZIL_ASYNC_ALLOC_FAIL", "2"))
                            | Some(("ZIL_ASYNC_INSTALL_FAIL", _)) => {
                                "registration failure; owned environment dropped exactly once\n"
                            }
                            _ => "queued late entry rejected; no-future before two frees\n",
                        };
                        assert_eq!(actual.stdout, expected.as_bytes(), "{actual:?}");
                    }
                }
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn async_registration_source_neighbors_and_unused_definitions() {
    let original =
        include_str!("../../../tests/language-readiness/async-registration/explicit.zil");
    let cases=[
        original.replace("c_register_async(async_register,async_request,async_drain,callback)","c_register_async(async_register,async_drain,async_request,callback)"),
        original.replace("let callback=move", "let callback="),
        original.replace("var count=37;", "unsafe {async_drain(c_null());} var count=37;"),
        original.replace("unsafe {", "{"),
        original.replace("async_registration_finish_destroy(give owner);", ""),
        original.replace("async_registration_finish_destroy(give owner);", "let pair=(give owner,1);"),
        original.replace("async_registration_finish_destroy(give owner);", "let cleanup=move()->i32=>{async_registration_finish_destroy(give owner);return 0};"),
        original.replace("async_registration_finish_destroy(give owner);", "registration_close(give owner);"),
        original.replace("async_registration_finish_destroy(give owner);", "async_registration_finish_destroy(owner);"),
        original.replace("async_registration_finish_destroy(give owner);", "async_registration_finish_destroy(give owner);async_registration_finish_destroy(give owner);"),
        original.replace("async_registration_request_close(&mut owner)","async_registration_request_close(owner)"),
        original.replace("var owner=", "let owner="),
        original.replace("var count=37;", "let c_register_async=1;var count=37;"),
        format!("{}fn main(){{}}",original.split("fn main").next().unwrap().replace("async_drain(token:c_handle)->i32","async_drain(token:c_handle)->bool")),
        format!("{}fn main(){{}}",original.split("fn main").next().unwrap().replace("static_callbacks(),async_callback_poll_drain()","async_callback_poll_drain()")),
    ];
    for (index, text) in cases.iter().enumerate() {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "neighbor {index}: {}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::InfraError,
            "neighbor {index}: {}",
            report.to_json()
        );
        eprintln!("async_registration neighbor {index}: {:?}", report.status);
    }
    for text in [
        original.replace(
            "c_register_async(async_register,async_request,async_drain,callback)",
            "#(c_register_async(async_register,async_request,async_drain,callback))",
        ),
        original.replace("fn main()", "fn create_and_close()")
            + "fn main()->i32=>#(create_and_close())",
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, zil_base::Phase::Ct, "{}", report.to_json());
    }
    for text in [
        include_str!("../../../tests/language-readiness/async-registration/legacy-sync-direct.zil"),
        include_str!(
            "../../../tests/language-readiness/async-registration/legacy-sync-factory.zil"
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    let declarations = original.split("fn main").next().unwrap();
    let generic = format!(
        "{declarations}fn install<F:FnMut(i32)->i32 + ClosedEnvironment>(f:F)->AsyncRegistration<F> contract{{result(create);}} {{unsafe{{return c_register_async(async_register,async_request,async_drain,f)}}}}\nfn main()->i32 {{var count=37;let callback=move(value:i32)->i32=>{{count+=1;return value+count}};var owner=install(callback);async_registration_request_close(&mut owner);async_registration_poll_close(&mut owner);async_registration_finish_destroy(give owner);return 0}}"
    );
    let early = original
        .replace("async_registration_request_close(&mut owner);", "")
        .replace("async_registration_poll_close(&mut owner);", "");
    assert_eq!(
        compile_source(&source(&early), &CompileOptions::default()).status,
        ResultCategory::Success,
        "explicit early finish is guarded by runtime CLOSED, not an invented static Poll refinement"
    );
    let report = compile_source(&source(&generic), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let unused = generic.split("fn main").next().unwrap().to_owned() + "fn main(){}";
    assert_eq!(
        compile_source(&source(&unused), &CompileOptions::default()).status,
        ResultCategory::Success
    );
    for text in [
        unused.replace(" + ClosedEnvironment", ""),
        unused.replace("FnMut(i32)", "FnOnce(i32)"),
        unused.replace(
            "async_register,async_request,async_drain,f",
            "async_register,async_drain,async_request,f",
        ),
        unused.replace(" + ClosedEnvironment", " + ClosedEnvironment + Transfer"),
        unused.replace(" + ClosedEnvironment", " + ClosedEnvironment + Share"),
    ] {
        assert_ne!(
            compile_source(&source(&text), &CompileOptions::default()).status,
            ResultCategory::Success
        );
    }
}

#[test]
fn async_registration_raw_wire_and_exact_budgets() {
    use zil_ir::{InstructionKind as I, Type};
    let src = source(include_str!(
        "../../../tests/language-readiness/async-registration/explicit.zil"
    ));
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let main = typed
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    for variant in 0..15 {
        let mut raw = typed.clone();
        match variant {
            0 => {
                let i = raw.functions[main]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::AsyncRegistrationStart { .. }))
                    .unwrap();
                if let I::AsyncRegistrationStart {
                    request_close,
                    poll_drain,
                    ..
                } = &mut i.kind
                {
                    std::mem::swap(request_close, poll_drain);
                }
            }
            1 => {
                let drain = raw
                    .functions
                    .iter_mut()
                    .find(|f| f.name == "async_drain")
                    .unwrap();
                drain.return_type = Type::Unit;
            }
            2 => {
                let resource = raw
                    .resources
                    .iter_mut()
                    .find(|r| matches!(r.payload, Type::AsyncRegistration { .. }))
                    .unwrap();
                resource.name = "@builtin:Registration:0".into();
            }
            3 => {
                let f = &mut raw.functions[main];
                let i = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, I::AsyncRegistrationPollClose { .. }))
                    .unwrap();
                let v = i.result.unwrap();
                let Type::Enum { name, .. } = &mut f.values[v.0 as usize] else {
                    panic!()
                };
                *name = "@user:Poll".into();
            }
            4 => {
                raw.closures[0].kind = zil_ir::CallableKind::FnOnce;
            }
            5 => {
                let capture = &mut raw.closures[0].captures[0];
                *capture = Type::Address {
                    mutable: false,
                    inner: Box::new(capture.clone()),
                };
            }
            6 => {
                let owner = raw
                    .resources
                    .iter()
                    .find(|r| matches!(r.payload, Type::AsyncRegistration { .. }))
                    .unwrap()
                    .payload
                    .clone();
                raw.functions[main].values.push(Type::Package {
                    name: "@user:pollable-package".into(),
                    fields: vec![("owner".into(), Type::Managed(Box::new(owner)))],
                });
            }
            7 => {
                let owner = raw
                    .resources
                    .iter()
                    .find(|r| matches!(r.payload, Type::AsyncRegistration { .. }))
                    .unwrap()
                    .payload
                    .clone();
                raw.functions[main].values.push(Type::OwnedStore {
                    element: Box::new(Type::Managed(Box::new(owner))),
                });
            }
            8 => {
                let f = &mut raw.functions[main];
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::AsyncRegistrationStart { .. }))
                    .unwrap();
                if let I::AsyncRegistrationStart { region, .. } = &mut i.kind {
                    *region += 1;
                }
            }
            9 => {
                let f = &mut raw.functions[main];
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::AsyncRegistrationStart { .. }))
                    .unwrap();
                i.result = None;
            }
            10..=12 => {
                let payload = raw
                    .resources
                    .iter()
                    .find(|r| matches!(r.payload, Type::AsyncRegistration { .. }))
                    .unwrap()
                    .payload
                    .clone();
                let owner = Type::Managed(Box::new(payload));
                let aggregate = match variant {
                    10 => Type::Tuple(vec![owner, Type::I32]),
                    11 => Type::Enum {
                        name: "@user:pollable-enum".into(),
                        variants: vec![("Owner".into(), vec![owner])],
                    },
                    _ => Type::ErrorUnion {
                        error: Box::new(Type::I32),
                        result: Box::new(owner),
                    },
                };
                raw.functions[main].values.push(aggregate);
            }
            13 => {
                let payload = raw
                    .resources
                    .iter()
                    .find(|r| matches!(r.payload, Type::AsyncRegistration { .. }))
                    .unwrap()
                    .payload
                    .clone();
                raw.closures[0].captures[0] = Type::Managed(Box::new(payload));
            }
            14 => {
                raw.closures[0].captures[0] = Type::Managed(Box::new(Type::TaskI32));
            }
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            if (6..=7).contains(&variant) {
                ResultCategory::Unsupported
            } else {
                ResultCategory::SafeReject
            },
            "Analyze raw {variant}: {errors:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        for decoded in [false, true] {
            let (input, claim) = if decoded {
                zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
            } else {
                (raw.clone(), claim.clone())
            };
            let errors = zil_verify::verify_module(input, claim, &target, &limits).unwrap_err();
            assert_eq!(
                errors[0].category,
                if variant == 13 {
                    ResultCategory::Unsupported
                } else {
                    ResultCategory::SafeReject
                },
                "Verify raw {variant} wire={decoded}: {errors:?}"
            );
        }
    }
    // A parent's exclusive address cannot be used while its actual child
    // reborrow is live. The same operations through the child remain valid.
    for poll in [false, true] {
        for parent_access in [false, true] {
            let mut raw = typed.clone();
            let f = &mut raw.functions[main];
            let child = zil_ir::ValueId(f.values.len() as u32);
            let (block, position) = f
                .blocks
                .iter()
                .enumerate()
                .find_map(|(b, block)| {
                    block
                        .instructions
                        .iter()
                        .position(|i| {
                            if poll {
                                matches!(i.kind, I::AsyncRegistrationPollClose { .. })
                            } else {
                                matches!(i.kind, I::AsyncRegistrationRequestClose { .. })
                            }
                        })
                        .map(|i| (b, i))
                })
                .unwrap();
            let mut operation = f.blocks[block].instructions[position].clone();
            let address = match operation.kind {
                I::AsyncRegistrationRequestClose { registration }
                | I::AsyncRegistrationPollClose { registration } => registration,
                _ => unreachable!(),
            };
            f.values.push(f.values[address.0 as usize].clone());
            if let Some(result) = operation.result.filter(|_| parent_access) {
                let result_ty = f.values[result.0 as usize].clone();
                operation.result = Some(zil_ir::ValueId(f.values.len() as u32));
                f.values.push(result_ty);
            }
            match &mut operation.kind {
                I::AsyncRegistrationRequestClose { registration }
                | I::AsyncRegistrationPollClose { registration } => *registration = child,
                _ => unreachable!(),
            }
            let span = operation.span;
            f.blocks[block].instructions.insert(
                position,
                zil_ir::Instruction {
                    result: Some(child),
                    kind: I::ReborrowAddress {
                        address,
                        mutable: true,
                    },
                    span,
                },
            );
            if parent_access {
                f.blocks[block].instructions.insert(position + 2, operation);
            } else {
                f.blocks[block].instructions[position + 1] = operation;
            }
            let result = zil_analyze::analyze(&raw, &limits);
            if parent_access {
                let errors = result.unwrap_err();
                assert!(
                    errors.iter().any(|d| d.message.contains("live child")),
                    "Analyze poll={poll}: {errors:?}"
                );
            } else {
                result.unwrap();
            }
            let mut valid = raw.clone();
            if parent_access {
                match &mut valid.functions[main].blocks[block].instructions[position + 1].kind {
                    I::AsyncRegistrationRequestClose { registration }
                    | I::AsyncRegistrationPollClose { registration } => *registration = child,
                    _ => unreachable!(),
                }
            }
            let mut claim = zil_analyze::analyze(&valid, &limits).unwrap();
            claim.module_fingerprint = raw.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
            for wire in [false, true] {
                let (input, claim) = if wire {
                    zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
                } else {
                    (raw.clone(), claim.clone())
                };
                let result = zil_verify::verify_module(input, claim, &target, &limits);
                if parent_access {
                    let errors = result.unwrap_err();
                    assert!(
                        errors
                            .iter()
                            .any(|d| d.category == ResultCategory::SafeReject),
                        "Verify poll={poll} wire={wire}: {errors:?}"
                    );
                } else {
                    result.unwrap();
                }
            }
        }
    }
    for verify in [false, true] {
        let accepts = |steps| {
            let selected = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &selected)
                    .map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &selected).map(|_| ())
            }
        };
        let (mut low, mut high) = (0, limits.max_analysis_steps);
        while low < high {
            let middle = low + (high - low) / 2;
            if accepts(middle).is_ok() {
                high = middle
            } else {
                low = middle + 1
            }
        }
        accepts(low).unwrap();
        assert_eq!(
            accepts(low - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        accepts(low).unwrap();
        eprintln!("async_registration exact kernel verify={verify} N={low}");
    }
}

#[test]
fn async_registration_legacy_task_start_cannot_hide_pollable_worker_raw_or_wire() {
    use zil_ir::{
        BasicBlock, BlockId, Constant, Instruction, InstructionKind as I, Terminator, Type, ValueId,
    };
    let text =
        include_str!("../../../tests/language-readiness/async-registration/queued-cancel.zil");
    let prefix = text.split("fn drive(").next().unwrap();
    let text = format!(
        "{prefix}\nfn sync(seed:i32)->i32 {{task_yield();return seed}}\nfn hidden()->i32 {{var task=task_start(sync,37);return 0}}\nfn main()->i32 {{var task=task_start(sync,37);return 0}}\n"
    );
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let src = source(&text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let pollable = typed
        .functions
        .iter()
        .find(|f| f.name == "worker")
        .unwrap()
        .id;
    let main = typed
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let hidden = typed
        .functions
        .iter()
        .position(|f| f.name == "hidden")
        .unwrap();
    for variant in 0..3 {
        let mut raw = typed.clone();
        let mut claim = plan.clone();
        let index = if variant == 1 { hidden } else { main };
        if variant == 2 {
            let f = &mut raw.functions[index];
            let block = BlockId(f.blocks.len() as u32);
            let argument = ValueId(f.values.len() as u32);
            f.values.push(Type::I32);
            let output = ValueId(f.values.len() as u32);
            f.values.push(Type::Managed(Box::new(Type::TaskI32)));
            let sync = typed
                .functions
                .iter()
                .find(|f| f.name == "sync")
                .unwrap()
                .id;
            f.blocks.push(BasicBlock {
                id: block,
                instructions: vec![
                    Instruction {
                        result: Some(argument),
                        kind: I::Constant(Constant::I32(37)),
                        span: f.span,
                    },
                    Instruction {
                        result: Some(output),
                        kind: I::TaskStart {
                            worker: sync,
                            argument,
                        },
                        span: f.span,
                    },
                ],
                terminator: Terminator::Trap,
            });
            claim = zil_analyze::analyze(&raw, &limits).unwrap();
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
            let i = raw.functions[index]
                .blocks
                .last_mut()
                .unwrap()
                .instructions
                .last_mut()
                .unwrap();
            let I::TaskStart { worker, .. } = &mut i.kind else {
                unreachable!()
            };
            *worker = pollable;
        } else {
            for i in raw.functions[index]
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
            {
                if let I::TaskStart { worker, .. } = &mut i.kind {
                    *worker = pollable;
                }
            }
        }
        claim.module_fingerprint = raw.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        for decoded in [false, true] {
            let (module, candidate) = if decoded {
                zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
            } else {
                (raw.clone(), claim.clone())
            };
            for (kernel, errors) in [
                (
                    "Analyze",
                    zil_analyze::analyze(&module, &limits).unwrap_err(),
                ),
                (
                    "Verify",
                    zil_verify::verify_module(module, candidate, &target, &limits).unwrap_err(),
                ),
            ] {
                assert_eq!(
                    errors[0].category,
                    ResultCategory::SafeReject,
                    "variant={variant} decoded={decoded} {kernel}: {errors:?}"
                );
                assert!(
                    errors[0].message.contains("pollable Task"),
                    "variant={variant} decoded={decoded} {kernel}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn async_registration_actual_task_carrier_cleanup_requires_synchronous_origin() {
    let queued =
        include_str!("../../../tests/language-readiness/async-registration/queued-cancel.zil");
    let prefix = queued.split("fn drive(").next().unwrap();
    for (case, tail) in [
        ("direct", "return 0"),
        ("tuple", "let pair=(give task,0);return 0"),
        (
            "closure",
            "let callback=move ()=>{task_finish_destroy(give task);return true};return 0",
        ),
        (
            "once",
            "let callback=move ()=>{task_finish_destroy(give task);return true};invoke(callback);return 0",
        ),
        (
            "package",
            "let pair=Pair{task:give task,mark:Mark{code:1}};return 0",
        ),
    ] {
        let package = if case == "package" {
            "package Pair{task:Task<i32>;manages mark:Mark}"
        } else {
            ""
        };
        let text = format!(
            "{prefix}{package}fn invoke<F:FnOnce()->bool>(f:F)->bool{{return f()}}fn main()->std.task.TaskCreateError!i32{{var task=task_create(worker,37)?;task_poll(&mut task);{tail}}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        if ["package", "closure", "once"].contains(&case) {
            assert_eq!(
                report.status,
                ResultCategory::Unsupported,
                "{case}: {}",
                report.to_json()
            );
            let expected = if case == "package" {
                "aggregate field requires"
            } else {
                "closure value requires a finite Copy type"
            };
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains(expected)),
                "{}",
                report.to_json()
            );
            continue;
        }
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{case}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("Task automatic Drop")),
            "{case}: {}",
            report.to_json()
        );
        let sync = text.replace("task_create(worker,37)", "task_create(sync,37)")
            + "fn sync(seed:i32)->i32{task_yield();return seed}";
        let report = compile_source(&source(&sync), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "sync {case}: {}",
            report.to_json()
        );
        let limits = Limits::default();
        let target = zil_base::TargetProfile::default();
        let input = source(&sync);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let mut raw = zil_types::check(&core, &target, &limits).unwrap();
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let pollable = raw
            .functions
            .iter()
            .find(|f| f.name == "worker")
            .unwrap()
            .id;
        let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let zil_ir::InstructionKind::TaskCreate { worker, .. } = &mut instruction.kind {
                *worker = pollable;
            }
        }
        plan.module_fingerprint = raw.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        for decoded in [false, true] {
            let (raw, plan) = if decoded {
                zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
            } else {
                (raw.clone(), plan.clone())
            };
            for (kernel, errors) in [
                ("Analyze", zil_analyze::analyze(&raw, &limits).unwrap_err()),
                (
                    "Verify",
                    zil_verify::verify_module(raw, plan, &target, &limits).unwrap_err(),
                ),
            ] {
                assert_eq!(
                    errors[0].category,
                    ResultCategory::SafeReject,
                    "{case} {kernel} wire={decoded}: {errors:?}"
                );
                assert!(
                    errors[0].message.contains("pollable Task automatic Drop"),
                    "{case} {kernel} wire={decoded}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn task_origin_unused_task_owner_cannot_skip_synchronous_cleanup_proof() {
    let text = "fn unused(give task:Task<i32>){}fn main(){}";
    let input = source(text);
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        errors[0]
            .message
            .contains("Task automatic Drop has no complete synchronous worker origin"),
        "{errors:?}"
    );
    // Nearest source control explicitly transfers responsibility to finish;
    // its runtime CLOSED guard is the existing Task contract.
    let control = text.replace(
        "Task<i32>){}",
        "Task<i32>){task_finish_destroy(give task);}",
    );
    let input = source(&control);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let control = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&control, &limits).unwrap();
    zil_verify::verify_module(control, plan, &target, &limits).unwrap();
}

fn captured_helper_source_captured_sources() -> Vec<(&'static str, String)> {
    let shared = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-context-producer/fn-input-capture.zil"
    );
    let once = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-context-producer/fnonce-input-capture.zil"
    );
    let exclusive = shared
        .replace("let f=(p:", "var count=0;var f=move(p:")
        .replace("=>i32(*p.p+*left.p);return f(left)", "=>{count+=1;return i32(*p.p+*left.p)+count-1};let first=f(left);return first+f(left)-43");
    vec![
        ("fn", shared.into()),
        ("fnmut-two-calls", exclusive),
        ("fnonce", once.into()),
    ]
}

#[test]
fn captured_helper_source_captured_source_wire_and_exact_lir_binding() {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (case, text) in captured_helper_source_captured_sources() {
        let input = source(&text);
        let syntax = zil_syntax::parse(&input).into_result().unwrap();
        let aliases = Default::default();
        let core = zil_expand::expand_with_map_limits_target_context(
            &input,
            &syntax,
            &limits,
            &target,
            SourceIdentityContext::ClosedSnapshot {
                root: input.module_id,
                aliases: &aliases,
                declarations: DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        assert_eq!(core.closure_life_contexts.len(), 1, "{case}");
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        assert_eq!(raw.closures.len(), 1);
        assert_eq!(
            raw.closures[0].kind,
            match case {
                "fn" => zil_ir::CallableKind::Fn,
                "fnmut-two-calls" => zil_ir::CallableKind::FnMut,
                _ => zil_ir::CallableKind::FnOnce,
            }
        );
        assert!(
            raw.closures[0]
                .storage_life
                .as_ref()
                .unwrap()
                .fixed_binder_count
                > 0
        );
        if case == "fnonce" {
            assert!(
                raw.closures[0]
                    .captures
                    .iter()
                    .any(|t| matches!(t, zil_ir::Type::Managed(_)))
            );
        }
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let verified = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        let lir = zil_lower::lower(&verified).unwrap();
        zil_backend_llvm::emit(&verified, &lir).unwrap();
        for relation in [false, true] {
            let mut forged = lir.clone();
            if relation {
                forged.closures[0]
                    .storage_life
                    .as_mut()
                    .unwrap()
                    .fixed_binder_count += 1;
            } else {
                forged.closures[0].captures[0] = zil_ir::Type::I64;
            }
            let errors = zil_backend_llvm::emit(&verified, &forged).unwrap_err();
            assert_eq!(
                errors[0].code, "E-CODEGEN-UNBOUND-LIR",
                "{case}: {errors:?}"
            );
        }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn captured_helper_source_captured_source_native_full_results_and_budget() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for (case, text) in captured_helper_source_captured_sources() {
        let input = source(&text);
        // This is the actual complete Source-to-Lower threshold. The tight
        // phase is reported, never relabeled as an isolated Lower threshold.
        let check = |work| {
            compile_source(
                &input,
                &CompileOptions {
                    stop_after: Stage::Lower,
                    limits: Limits {
                        max_analysis_steps: work,
                        ..Limits::default()
                    },
                    ..CompileOptions::default()
                },
            )
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        assert_eq!(check(high).status, ResultCategory::Success);
        while low < high {
            let mid = low + (high - low) / 2;
            let report = check(mid);
            if report.status == ResultCategory::Success {
                high = mid;
            } else {
                assert_eq!(
                    report.status,
                    ResultCategory::ResourceLimit,
                    "{case}: {}",
                    report.to_json()
                );
                low = mid + 1;
            }
        }
        // Complete nominal-row proofs and retained-boundary inventories now
        // run independently in each kernel. These are measured actual source
        // counts, separate from the unchanged local callable solver fees.
        let (old, nominal, retained_scans) = match case {
            "fn" => (24_224, 178, 37),
            "fnmut-two-calls" => (34_804, 178, 39),
            "fnonce" => (36_926, 98, 22),
            _ => unreachable!(),
        };
        assert_eq!(
            low,
            old + nominal + retained_scans * 3,
            "{case} actual Source work changed"
        );
        assert_eq!(check(old).status, ResultCategory::ResourceLimit);
        assert_eq!(check(low).status, ResultCategory::Success);
        let short = check(low - 1);
        assert_eq!(short.status, ResultCategory::ResourceLimit);
        assert_eq!(check(low).status, ResultCategory::Success);
        eprintln!(
            "captured helper lowering {case} whole Source-to-Lower N={low}/N-1/fresh; tight phase={:?}",
            short.phase
        );
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-captured_helper_source-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &input,
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("main".into()),
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{case}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let result = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{case} {backend:?} {optimization:?}: {result:?}"
                );
                assert_eq!(result.stdout, b"42\n");
                assert!(result.stderr.is_empty());
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn owned_resource_identity_owned_fixed_source_resource_identity_is_independent() {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let text = captured_helper_source_captured_sources().pop().unwrap().1;
    let input = source(&text);
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let aliases = Default::default();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &limits,
        &target,
        SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &aliases,
            declarations: DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    assert_eq!(raw.closures[0].kind, zil_ir::CallableKind::FnOnce);
    assert!(matches!(
        raw.closures[0].captures[0],
        zil_ir::Type::Managed(_)
    ));
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    for mode in 0..4 {
        let mut forged = raw.clone();
        match mode {
            0 => {
                forged.resources[0].name = "Other".into();
                let zil_ir::Type::Record { name, .. } = &mut forged.resources[0].payload else {
                    unreachable!()
                };
                *name = "Other".into();
            }
            1 => forged.resources.clear(),
            2 => forged.resources.push(forged.resources[0].clone()),
            _ => {
                let mut local = forged.functions[0].locals[0].clone();
                local.id = zil_ir::LocalId(forged.functions[0].locals.len() as u32);
                local.name = "unused_other".into();
                local.ty = zil_ir::Type::Managed(Box::new(zil_ir::Type::Record {
                    name: "Other".into(),
                    fields: vec![("n".into(), zil_ir::Type::I32)],
                }));
                forged.functions[0].locals.push(local);
            }
        }
        let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
        assert!(
            errors.iter().any(|d| if mode == 1 {
                d.category == ResultCategory::Unsupported
            } else {
                d.code == "E-ANALYZE-IR" && d.message.contains("resource")
            }),
            "Source-derived Analyze neighbor {mode}: {errors:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&forged, &plan, &limits).unwrap();
        for errors in [
            zil_verify::verify_module(forged, plan.clone(), &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
        ] {
            assert!(
                errors.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
                "Source-derived Verify neighbor {mode}: {errors:?}"
            );
        }
    }
    let wrong = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-context-producer/fnonce-wrong-class.zil"
    );
    let report = compile_source(
        &source(wrong),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Analyze);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("actual retained classes"))
    );
}

#[test]
fn captured_owned_fixed_actual_source_reaches_independent_kernels_and_wire() {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    use zil_ir::{CallableKind, InstructionKind as I, Type};
    let original = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-context-producer/fnonce-input-capture.zil"
    );
    let dual = original
        .replace(
            "fn apply<life s>(left:&Tag<'s>)",
            "fn apply<life s,life t>(left:&Tag<'s>,right:&Tag<'t>)",
        )
        .replace("{let mark=Mark", "{let seen=right;let mark=Mark")
        .replace("return apply(&tag)", "return apply(&tag,&tag)");
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (label, text) in [("original", original), ("two-input", dual.as_str())] {
        let unit = source(text);
        let syntax = zil_syntax::parse(&unit).into_result().unwrap();
        let aliases = Default::default();
        let core = zil_expand::expand_with_map_limits_target_context(
            &unit,
            &syntax,
            &limits,
            &target,
            SourceIdentityContext::ClosedSnapshot {
                root: unit.module_id,
                aliases: &aliases,
                declarations: DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let d = typed
            .closures
            .iter()
            .find(|d| d.kind == CallableKind::FnOnce)
            .unwrap();
        assert!(matches!(d.captures[0], Type::Managed(_)));
        assert!(matches!(d.captures[1], Type::Address { .. }));
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        let (decoded, claim) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        zil_analyze::analyze(&decoded, &limits).unwrap();
        zil_verify::verify_module(decoded, claim, &target, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        if label == "two-input" {
            let mut raw = typed.clone();
            let f = raw
                .functions
                .iter_mut()
                .find(|f| f.name == "apply")
                .unwrap();
            let right = f.locals.iter().find(|l| l.name == "right").unwrap().id;
            let value = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match i.kind {
                    I::Load(l) if l == right => i.result,
                    _ => None,
                })
                .unwrap();
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, I::CallClosureOnce { .. }))
                .unwrap();
            let I::CallClosureOnce { arguments, .. } = &mut i.kind else {
                unreachable!()
            };
            assert_eq!(
                f.values[arguments[0].0 as usize],
                f.values[value.0 as usize]
            );
            arguments[0] = value;
            let error = zil_analyze::analyze(&raw, &limits).unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class")),
                "{error:?}"
            );
            let mut claim = plan;
            claim.module_fingerprint = raw.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
            let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert!(
                zil_analyze::analyze(&decoded, &limits)
                    .unwrap_err()
                    .iter()
                    .any(|d| d.message.contains("class"))
            );
            for result in [
                zil_verify::verify_module(raw.clone(), claim, &target, &limits),
                zil_verify::verify_module_bytes(&bytes, &target, &limits),
            ] {
                let error = result.unwrap_err();
                assert!(
                    error.iter().any(|d| d.message.contains("class equality")),
                    "{error:?}"
                );
            }
        }
    }
}

const SHARED_HELPER_LIFE_SIMPLE: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/fn-shared-input.zil"
);
const SHARED_HELPER_LIFE_REVERSE: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/reverse-generics-good.zil"
);
const SHARED_HELPER_LIFE_CALLER: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/actual-caller-good.zil"
);
fn shared_helper_life_core(text: &str) -> zil_ir::CoreModule {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &Limits::default(),
        &zil_base::TargetProfile::host_llvm(),
        SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core
}
fn shared_helper_life_role(core: &zil_ir::CoreModule) -> &zil_ir::CoreCallableParameter {
    core.functions
        .iter()
        .flat_map(|f| &f.callable_parameters)
        .find(|r| r.source_life.is_some())
        .unwrap()
}

#[test]
fn shared_helper_life_original_bound_full_ordinals_and_complete_typed_relations() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let a = shared_helper_life_core(include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/unused-declared-bound-a.zil"
    ));
    let b = shared_helper_life_core(include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/unused-declared-bound-b.zil"
    ));
    let (a, b) = (shared_helper_life_role(&a), shared_helper_life_role(&b));
    assert_eq!(a.parameters, b.parameters);
    assert_eq!(a.result, b.result);
    assert_eq!(a.life, b.life);
    let (a, b) = (
        a.source_life.as_ref().unwrap(),
        b.source_life.as_ref().unwrap(),
    );
    assert_eq!(a.parameters, b.parameters);
    assert_eq!(a.inputs, b.inputs);
    assert_eq!(a.named_fixed[0].parameter, 0);
    assert_eq!(b.named_fixed[0].parameter, 1);
    for (source, expected) in [(0, StorageBinder::Fixed(0)), (1, StorageBinder::Fixed(1))] {
        assert!(a.inputs.iter().any(|i| i.source_parameter == Some(source)
            && i.header_binder == expected
            && matches!(i.position, CoreClosureHeaderPosition::Class(_))));
    }
    let reverse = shared_helper_life_core(SHARED_HELPER_LIFE_REVERSE);
    let role = shared_helper_life_role(&reverse);
    let context = role.source_life.as_ref().unwrap();
    assert_eq!(
        context.parameters,
        vec![
            LifeParameterKind::Lifetime,
            LifeParameterKind::Type,
            LifeParameterKind::Lifetime
        ]
    );
    assert_eq!(
        (context.generic_parameter, role.generic_index, role.index),
        (1, 0, 0)
    );
    assert_eq!(context.named_fixed[0].parameter, 2);
    assert!(context.inputs.iter().any(|i| i.parameter == 2
        && i.source_parameter == Some(2)
        && i.header_binder == StorageBinder::Fixed(1)
        && matches!(i.position, CoreClosureHeaderPosition::Class(_))));
    for text in [
        SHARED_HELPER_LIFE_SIMPLE,
        SHARED_HELPER_LIFE_REVERSE,
        SHARED_HELPER_LIFE_CALLER,
    ] {
        let core = shared_helper_life_core(text);
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let d = &raw.closures[0];
        assert_eq!(d.kind, CallableKind::Fn);
        let helper = raw
            .functions
            .iter()
            .find(|f| f.callable_contract.is_some())
            .unwrap();
        let role = &helper.callable_contract.as_ref().unwrap().roles[0];
        assert_eq!(role.life, d.storage_life);
        let header = helper.storage_life.as_ref().unwrap();
        for class in 0..d.storage_life.as_ref().unwrap().fixed_binder_count {
            assert!(header.classes.iter().any(|r| r.position.base
                == StorageBase::Parameter(role.parameter)
                && r.position.path == [LifeTypeProjection::Pointee]
                && r.position.class == class));
        }
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
    let twice = SHARED_HELPER_LIFE_SIMPLE.replace(
        "return helper(&f,seed)",
        "let first=helper(&f,seed);return first+helper(&f,seed)-42",
    );
    let report = compile_source(
        &source(&twice),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let wrong = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/actual-caller-wrong-class.zil"
    );
    let report = compile_source(
        &source(wrong),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Analyze);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("actual retained classes"))
    );
}

#[test]
fn shared_helper_life_source_derived_actual_call_and_capture_neighbors_reach_both_kernels() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let raw = zil_types::check(
        &shared_helper_life_core(SHARED_HELPER_LIFE_CALLER),
        &target,
        &limits,
    )
    .unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let helper = raw
        .functions
        .iter()
        .find(|f| f.callable_contract.is_some())
        .unwrap()
        .id;
    let caller =
        raw.functions
            .iter()
            .position(|f| {
                f.blocks.iter().flat_map(|b| &b.instructions).any(
                    |i| matches!(i.kind,InstructionKind::Call {function,..} if function==helper),
                )
            })
            .unwrap();
    let right = raw.functions[caller].parameters[1];
    let right_value = raw.functions[caller]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::Load(local) if local == right => i.result,
            _ => None,
        })
        .unwrap();
    for change_capture in [false, true] {
        let mut bad = raw.clone();
        let mut changed = false;
        for instruction in bad.functions[caller]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
        {
            match &mut instruction.kind {
                InstructionKind::Call {
                    function,
                    arguments,
                } if !change_capture && *function == helper => {
                    arguments[1] = right_value;
                    changed = true;
                }
                InstructionKind::MakeClosure { captures, .. } if change_capture => {
                    captures[0] = right_value;
                    changed = true;
                }
                _ => {}
            }
        }
        assert!(changed);
        let analyze = zil_analyze::analyze(&bad, &limits).unwrap_err();
        assert!(
            analyze.iter().any(|d| d.message.contains("class")),
            "actual class proof: {analyze:?}"
        );
        // Rebind only the untrusted current-module fingerprint. This is an
        // adversarial raw candidate, not an Analyze-produced success plan.
        let mut forged_plan = plan.clone();
        forged_plan.module_fingerprint = bad.fingerprint();
        let wire = wire::encode_module_candidate(&bad, &forged_plan, &limits).unwrap();
        for errors in [
            zil_verify::verify_module(bad, forged_plan, &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
        ] {
            assert!(
                errors.iter().any(|d| d.message.contains("class")),
                "actual independent class proof: {errors:?}"
            );
        }
    }
}

#[test]
fn shared_helper_life_malformed_context_clone_budget_and_fresh_warm_requests() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let core = shared_helper_life_core(SHARED_HELPER_LIFE_SIMPLE);
    let helper = core
        .functions
        .iter()
        .position(|f| {
            f.callable_parameters
                .iter()
                .any(|r| r.source_life.is_some())
        })
        .unwrap();
    for mutation in 0..9 {
        let mut bad = core.clone();
        let role = &mut bad.functions[helper].callable_parameters[0];
        match mutation {
            0 => role.source_life = None,
            1 => role.source_life.as_mut().unwrap().generic_parameter = 0,
            2 => role.generic_index = 1,
            3 => role.index = u32::MAX,
            4 => role.source_life.as_mut().unwrap().named_fixed[0].parameter = 99,
            5 => {
                role.source_life.as_mut().unwrap().inputs[0].header_binder =
                    StorageBinder::Fixed(99)
            }
            6 => role.source_life.as_mut().unwrap().bound_ordinal = 99,
            7 => {
                let duplicate = role.source_life.as_ref().unwrap().inputs[0].clone();
                role.source_life.as_mut().unwrap().inputs.push(duplicate);
            }
            _ => role.source_life.as_mut().unwrap().inputs[0].parameter = 99,
        }
        let errors = zil_types::check(&bad, &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit),
            "{mutation}: {errors:?}"
        );
        eprintln!(
            "SHARED_HELPER_LIFE malformed bridge {mutation}: {}",
            errors[0].message
        );
    }
    let mut renamed = core.clone();
    let identity = renamed.functions[helper].callable_parameters[0]
        .source_life
        .as_mut()
        .unwrap();
    identity.identity.package = "raw-provenance-record-only".into();
    identity.declaration.declaration = zil_base::NodeId(999);
    assert_eq!(
        zil_types::check(&renamed, &target, &limits).unwrap(),
        zil_types::check(&core, &target, &limits).unwrap()
    );
    let bounded = Limits {
        max_analysis_steps: 16_384,
        ..limits.clone()
    };
    zil_types::infer_source_closures(&core, &bounded).unwrap();
    for huge_path in [false, true] {
        let mut huge = core.clone();
        let context = huge.functions[helper].callable_parameters[0]
            .source_life
            .as_mut()
            .unwrap();
        if huge_path {
            context.inputs[0].position = CoreClosureHeaderPosition::Borrow(StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![
                    ContractProjection::Place(PlaceProjection::Field(0));
                    limits.max_call_depth + 1
                ],
            });
        } else {
            context.identity.package = "x".repeat(32_768);
        }
        let errors = zil_types::infer_source_closures(&huge, &bounded).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit
                    && d.message.contains("context")),
            "{errors:?}"
        );
    }
    let expected = zil_types::check(&core, &target, &limits).unwrap();
    let (mut low, mut high) = (1, limits.max_analysis_steps);
    while low < high {
        let mid = low + (high - low) / 2;
        let probe = Limits {
            max_analysis_steps: mid,
            ..limits.clone()
        };
        match zil_types::check(&core, &target, &probe) {
            Ok(_) => high = mid,
            Err(errors)
                if errors
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit) =>
            {
                low = mid + 1
            }
            Err(errors) => panic!("{errors:?}"),
        }
    }
    for n in [low, low - 1, low] {
        let probe = Limits {
            max_analysis_steps: n,
            ..limits.clone()
        };
        let result = zil_types::check(&core, &target, &probe);
        if n == low {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit)
            );
        }
    }
    let program = CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    };
    let mut session = zil_types::FunctionSession::default();
    let mut prior = None;
    for _ in 0..2 {
        let prepared = zil_types::infer_program_for_ct(&program, &limits)
            .unwrap()
            .into_prepared();
        let output = session
            .check_prepared_program(&prepared, &target, &limits)
            .unwrap();
        if let Some(prior) = &prior {
            assert_eq!(prior, &output);
        }
        prior = Some(output);
    }
    eprintln!(
        "SHARED_HELPER_LIFE whole Types N={low}/N-1/fresh; repeated actual FunctionSession request checked (qualified profile has no scalar reuse claim)"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn shared_helper_life_actual_source_native_clang_api_o0_o2_full_results() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let mut runs = 0;
    for (case, text) in [
        ("simple", SHARED_HELPER_LIFE_SIMPLE),
        ("reverse", SHARED_HELPER_LIFE_REVERSE),
        (
            "ordered-f-before",
            include_str!(
                "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/ordered-f-before-data.zil"
            ),
        ),
        (
            "ordered-f-after",
            include_str!(
                "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/ordered-f-after-data.zil"
            ),
        ),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-shared_helper_life-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("main".into()),
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{case}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let result = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{case}/{backend:?}/{optimization:?}: {result:?}"
                );
                assert_eq!(result.stdout, b"42\n");
                assert!(result.stderr.is_empty());
                std::fs::remove_dir_all(dir).unwrap();
                runs += 1;
            }
        }
    }
    eprintln!(
        "SHARED_HELPER_LIFE real Source native runs={runs}; stdout42 newline, empty stderr, exit0, selected backend and no fallback checked"
    );
}

#[test]
fn shared_helper_life_scope_frontiers_keep_old_unused_bodies_and_body_ct_transport() {
    let prefix = SHARED_HELPER_LIFE_SIMPLE.split("fn apply").next().unwrap();
    for replacement in [
        "F:ClosedEnvironment+Fn(&Tag<'s>)->i32",
        "F:Fn(&Tag<'s>)->i32+ClosedEnvironment",
        "F:Fn(&Tag<'s>)->i32,G:Fn(&Tag<'s>)->i32",
    ] {
        let mut text = prefix.replace("F:Fn(&Tag<'s>)->i32", replacement);
        if replacement.contains(",G:") {
            text = text.replace("f:&F,x:", "f:&F,g:&G,x:");
        }
        text.push_str("fn main()->i32{return 0}");
        let core = shared_helper_life_core(&text);
        let roles: Vec<_> = core
            .functions
            .iter()
            .flat_map(|f| &f.callable_parameters)
            .collect();
        if replacement.contains(",G:") {
            // Multiple real roles now keep their original source-bound
            // contexts. Runtime positions and full source ordinals differ.
            assert_eq!(roles.len(), 2);
            for (position, role) in roles.iter().enumerate() {
                assert_eq!(role.index, position as u32);
                assert_eq!(role.generic_index, position as u32);
                let original = role.source_life.as_ref().unwrap();
                assert_eq!(original.generic_parameter, position as u32 + 1);
                assert_eq!(original.bound_ordinal, 0);
                assert_eq!(original.parameters.len(), 3);
                assert_eq!(original.named_fixed.len(), 1);
                assert!(!original.inputs.is_empty());
                assert!(role.life.is_some());
            }
        } else {
            assert_eq!(roles.len(), 1);
            assert!(roles[0].source_life.is_none());
        }
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{replacement}: {}",
            report.to_json()
        );
    }
    for body in ["return 42", "return #(42)"] {
        for (text, good) in [
            (SHARED_HELPER_LIFE_CALLER, true),
            (
                include_str!(
                    "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/actual-caller-wrong-class.zil"
                ),
                false,
            ),
        ] {
            let text = text.replace("return f(x)", body);
            let report = compile_source(
                &source(&text),
                &CompileOptions {
                    stop_after: zil_driver::Stage::Verify,
                    ..CompileOptions::default()
                },
            );
            if good {
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{body}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
            } else {
                assert_eq!(
                    report.status,
                    ResultCategory::SafeReject,
                    "{body}: {}",
                    report.to_json()
                );
                assert_eq!(report.phase, zil_base::Phase::Analyze);
                assert!(
                    report
                        .diagnostics
                        .iter()
                        .any(|d| d.message.contains("actual retained classes"))
                );
            }
        }
    }
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/body-ct-call.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/body-ct-unused.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/ordered-f-before-data.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/ordered-f-after-data.zil"
        ),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(
            shared_helper_life_role(&shared_helper_life_core(text))
                .source_life
                .is_some()
        );
    }
}

#[test]
fn shared_helper_life_private_project_wrapper_cold_warm_change_revert_always_verifies() {
    use zil_driver::{CompilationSession, Stage};
    let dir = std::env::temp_dir().join(format!(
        "zil-shared_helper_life-project-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let root = dir.join("main.zil");
    let library = dir.join("library.zil");
    std::fs::write(
        &root,
        "import library\nfn main()->i32{return library.answer()}",
    )
    .unwrap();
    let good = SHARED_HELPER_LIFE_CALLER.replace("fn main()->i32", "pub fn answer()->i32");
    let wrong = good.replace("return helper(&f,left)", "return helper(&f,right)");
    let options = CompileOptions {
        stop_after: Stage::Verify,
        semantic_cache: Some(dir.join("semantic")),
        ..CompileOptions::default()
    };
    std::fs::write(&library, &good).unwrap();
    let mut session = CompilationSession::default();
    let cold = session.compile_path(&root, &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    assert_eq!(cold.verification_runs, 1);
    let warm = session.compile_path(&root, &options);
    assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
    assert_eq!(warm.verification_runs, 1);
    let disk = CompilationSession::default().compile_path(&root, &options);
    assert_eq!(disk.status, ResultCategory::Success, "{}", disk.to_json());
    assert_eq!(disk.verification_runs, 1);
    eprintln!(
        "SHARED_HELPER_LIFE private dependency cache cold={:?}, warm={:?}/memory_hit={}, new_session={:?}",
        cold.disk_cache, warm.disk_cache, warm.cache_hit, disk.disk_cache
    );
    std::fs::write(&library, &wrong).unwrap();
    let changed = session.compile_path(&root, &options);
    assert_eq!(
        changed.status,
        ResultCategory::SafeReject,
        "{}",
        changed.to_json()
    );
    assert_eq!(changed.phase, zil_base::Phase::Analyze);
    assert!(
        changed
            .diagnostics
            .iter()
            .any(|d| d.message.contains("actual retained classes"))
    );
    assert_ne!(changed.input_fingerprint, cold.input_fingerprint);
    std::fs::write(&library, &good).unwrap();
    for report in [
        session.compile_path(&root, &options),
        CompilationSession::default().compile_path(&root, &options),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert_eq!(report.input_fingerprint, cold.input_fingerprint);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shared_helper_life_unanchored_phantom_data_cannot_enter_the_physical_helper_profile() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let wrong = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/phantom-data-wrong-class.zil"
    );
    let mut core = shared_helper_life_core(wrong);
    assert!(
        core.functions
            .iter()
            .flat_map(|f| &f.callable_parameters)
            .all(|role| role.source_life.is_none())
    );
    let report = compile_source(
        &source(wrong),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let unused = include_str!(
        "../../../tests/language-readiness/terminal/342/fixed-helper-bridge/phantom-unused.zil"
    );
    assert!(
        shared_helper_life_core(unused)
            .functions
            .iter()
            .flat_map(|f| &f.callable_parameters)
            .all(|role| role.source_life.is_none())
    );
    let report = compile_source(
        &source(unused),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    // Adversarial Core attaches a well-shaped real context to a phantom
    // signature. Types must independently enforce its physical profile.
    let real = shared_helper_life_core(SHARED_HELPER_LIFE_SIMPLE);
    let source_role = shared_helper_life_role(&real);
    let helper = core
        .functions
        .iter_mut()
        .find(|f| !f.callable_parameters.is_empty())
        .unwrap();
    helper.callable_parameters[0].source_life = source_role.source_life.clone();
    helper.callable_parameters[0].life = source_role.life.clone();
    let errors = zil_types::check(&core, &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("source helper first profile")),
        "{errors:?}"
    );
}

const MUTABLE_HELPER_LIFE_STATEFUL: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/stateful-two-calls.zil"
);
const MUTABLE_HELPER_LIFE_REVERSE: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/reverse-stateful-two-calls.zil"
);
const MUTABLE_HELPER_LIFE_ADAPTED: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/fn-adapted-control.zil"
);
const MUTABLE_HELPER_LIFE_CALLER: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/actual-caller-good.zil"
);

#[test]
fn mutable_helper_life_actual_stateful_definition_receiver_and_source_bound_are_preserved() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (text, actual_kind) in [
        (MUTABLE_HELPER_LIFE_STATEFUL, CallableKind::FnMut),
        (MUTABLE_HELPER_LIFE_REVERSE, CallableKind::FnMut),
        (MUTABLE_HELPER_LIFE_ADAPTED, CallableKind::Fn),
    ] {
        let core = shared_helper_life_core(text);
        let bound = shared_helper_life_role(&core);
        assert_eq!(bound.kind, CallableKind::FnMut);
        let context = bound.source_life.as_ref().unwrap();
        if text == MUTABLE_HELPER_LIFE_REVERSE {
            assert_eq!(
                (context.generic_parameter, bound.generic_index, bound.index),
                (1, 0, 1)
            );
            assert_eq!(context.named_fixed[0].parameter, 2);
        }
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let definition = &raw.closures[0];
        assert_eq!(definition.kind, actual_kind);
        let entry = &raw.functions[definition.function.0 as usize];
        if actual_kind == CallableKind::FnMut {
            assert!(
                entry
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::WriteCapture { index: 0, .. }))
            );
            assert_eq!(definition.captures[0], Type::I32);
        }
        let helper = raw
            .functions
            .iter()
            .find(|f| f.callable_contract.is_some())
            .unwrap();
        let role = &helper.callable_contract.as_ref().unwrap().roles[0];
        assert_eq!(role.kind, CallableKind::FnMut);
        assert_eq!(role.life, definition.storage_life);
        let local = &helper.locals[helper.parameters[role.parameter as usize].0 as usize];
        assert!(
            matches!(&local.ty, Type::Address { mutable:true, inner } if matches!(inner.as_ref(),Type::Closure(_)))
        );
        assert!(
            helper
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(
                    i.kind,
                    InstructionKind::CallClosureBorrowed {
                        kind: CallableKind::FnMut,
                        ..
                    }
                ))
        );
        let header = helper.storage_life.as_ref().unwrap();
        for class in 0..definition.storage_life.as_ref().unwrap().fixed_binder_count {
            assert!(header.classes.iter().any(|row| row.position.base
                == StorageBase::Parameter(role.parameter)
                && row.position.path == [LifeTypeProjection::Pointee]
                && row.position.class == class));
        }
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
    for (text, phase, message) in [
        (
            include_str!(
                "../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/shared-receiver-reject.zil"
            ),
            zil_base::Phase::Types,
            "passing mode",
        ),
        (
            include_str!(
                "../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/overlapping-receiver-borrows.zil"
            ),
            zil_base::Phase::Analyze,
            "overlaps",
        ),
        (
            include_str!(
                "../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/wrong-actual-class.zil"
            ),
            zil_base::Phase::Analyze,
            "actual retained classes",
        ),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains(message)),
            "{}",
            report.to_json()
        );
    }
    let wrong = include_str!(
        "../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/wrong-actual-class.zil"
    );
    for body in ["return 42", "return #(42)"] {
        for (text, expected) in [
            (MUTABLE_HELPER_LIFE_CALLER, ResultCategory::Success),
            (wrong, ResultCategory::SafeReject),
        ] {
            let text = text.replace("return f(x)", body);
            let report = compile_source(
                &source(&text),
                &CompileOptions {
                    stop_after: zil_driver::Stage::Verify,
                    ..CompileOptions::default()
                },
            );
            assert_eq!(report.status, expected, "{}", report.to_json());
            if expected == ResultCategory::SafeReject {
                assert!(
                    report
                        .diagnostics
                        .iter()
                        .any(|d| d.message.contains("actual retained classes")),
                    "{}",
                    report.to_json()
                );
            }
        }
    }
}

#[test]
fn mutable_helper_life_source_derived_wrong_class_capture_and_mutable_call_reach_both_kernels() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let raw = zil_types::check(
        &shared_helper_life_core(MUTABLE_HELPER_LIFE_CALLER),
        &target,
        &limits,
    )
    .unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let helper = raw
        .functions
        .iter()
        .find(|f| f.callable_contract.is_some())
        .unwrap()
        .id;
    let caller = raw
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind,InstructionKind::Call{function,..}if function==helper))
        })
        .unwrap();
    let right = raw.functions[caller].parameters[1];
    let right_value = raw.functions[caller]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::Load(local) if local == right => i.result,
            _ => None,
        })
        .unwrap();
    for mutation in 0..3 {
        let mut bad = raw.clone();
        let mut changed = false;
        for f in &mut bad.functions {
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                match &mut i.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    } if mutation == 0 && *function == helper => {
                        arguments[1] = right_value;
                        changed = true;
                    }
                    InstructionKind::MakeClosure { captures, .. }
                        if mutation == 1 && f.id == raw.functions[caller].id =>
                    {
                        captures[1] = right_value;
                        changed = true;
                    }
                    InstructionKind::CallClosureBorrowed { kind, .. }
                        if mutation == 2 && f.id == helper =>
                    {
                        *kind = CallableKind::Fn;
                        changed = true;
                    }
                    _ => {}
                }
            }
        }
        assert!(changed);
        let errors = zil_analyze::analyze(&bad, &limits).unwrap_err();
        assert!(
            errors.iter().any(|d| if mutation < 2 {
                d.message.contains("class")
            } else {
                d.message.contains("kind")
                    || d.message.contains("callable")
                    || d.message.contains("mode")
            }),
            "{mutation}: {errors:?}"
        );
        // Adversarial current-module candidate: only its fingerprint is rebound,
        // so independent Verify must reject semantics rather than stale identity.
        let mut forged = plan.clone();
        forged.module_fingerprint = bad.fingerprint();
        let wire = wire::encode_module_candidate(&bad, &forged, &limits).unwrap();
        for errors in [
            zil_verify::verify_module(bad, forged, &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
        ] {
            assert!(
                !errors.iter().any(|d| d.message.contains("fingerprint")),
                "{errors:?}"
            );
            assert!(
                errors.iter().any(|d| if mutation < 2 {
                    d.message.contains("class")
                } else {
                    // Verify rejects this forged instruction at its finite
                    // stored-profile inventory, before the relation proof.
                    d.category == ResultCategory::Unsupported
                        && d.message.contains("outside the checked stored profile")
                }),
                "{mutation}: {errors:?}"
            );
        }
    }
    // Start with two actual same-type environments at disjoint physical locals.
    // Redirect only the first receiver borrow onto the second actual local;
    // SSA/CFG initialization facts remain unchanged, so Verify reaches loans.
    let disjoint = include_str!("../../../tests/language-readiness/terminal/342/fnmut-helper-bridge/overlapping-receiver-borrows.zil")
        .replace("let held=&mut f;", "var g=f;let held=&mut g;");
    let mut bad = zil_types::check(&shared_helper_life_core(&disjoint), &target, &limits).unwrap();
    let mut forged = zil_analyze::analyze(&bad, &limits).unwrap();
    zil_verify::verify_module(bad.clone(), forged.clone(), &target, &limits).unwrap();
    let caller = bad.functions.iter_mut().find(|f| f.blocks.iter().flat_map(|b| &b.instructions)
        .filter(|i| matches!(i.kind, InstructionKind::BorrowLocalMut {local} if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))))
        .count() == 2).unwrap();
    let receivers = caller
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match i.kind {
            InstructionKind::BorrowLocalMut { local }
                if matches!(caller.locals[local.0 as usize].ty, Type::Closure(_)) =>
            {
                Some(local)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_ne!(receivers[0], receivers[1]);
    let instruction = caller
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::BorrowLocalMut {local} if local==receivers[0]))
        .unwrap();
    instruction.kind = InstructionKind::BorrowLocalMut {
        local: receivers[1],
    };
    assert!(
        zil_analyze::analyze(&bad, &limits)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("overlaps"))
    );
    forged.module_fingerprint = bad.fingerprint();
    let wire = wire::encode_module_candidate(&bad, &forged, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(bad, forged, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
    ] {
        assert!(
            errors.iter().any(|d| d
                .message
                .contains("actual target intersects a live incompatible loan")),
            "{errors:?}"
        );
    }
}

#[test]
fn mutable_helper_life_context_budget_and_actual_function_session_requests_are_bounded() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let core = shared_helper_life_core(MUTABLE_HELPER_LIFE_STATEFUL);
    let expected = zil_types::check(&core, &target, &limits).unwrap();
    let (mut low, mut high) = (1, limits.max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        match zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: middle,
                ..limits.clone()
            },
        ) {
            Ok(_) => high = middle,
            Err(errors)
                if errors
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit) =>
            {
                low = middle + 1
            }
            Err(errors) => panic!("{errors:?}"),
        }
    }
    for n in [low, low - 1, low] {
        let result = zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: n,
                ..limits.clone()
            },
        );
        if n == low {
            assert_eq!(result.unwrap(), expected)
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit)
            )
        }
    }
    let program = CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            core: core.clone(),
            imports: vec![],
            exports: vec![],
        }],
    };
    let mut session = zil_types::FunctionSession::default();
    let mut prior = None;
    for _ in 0..2 {
        let prepared = zil_types::infer_program_for_ct(&program, &limits)
            .unwrap()
            .into_prepared();
        let output = session
            .check_prepared_program(&prepared, &target, &limits)
            .unwrap();
        if let Some(prior) = &prior {
            assert_eq!(prior, &output)
        }
        prior = Some(output);
    }
    // Before this slice the shared Fn / exclusive receiver unused declaration
    // was legal on None. Keep it off the paid new template formation lane.
    let old_none = "struct Tag<life s>{p:&'s i64}fn helper<life s,F:Fn(&Tag<'s>)->i32>(f:&mut F,x:&Tag<'s>)->i32{return 42}fn main()->i32{return 42}";
    assert!(
        shared_helper_life_core(old_none)
            .functions
            .iter()
            .flat_map(|f| &f.callable_parameters)
            .all(|r| r.source_life.is_none())
    );
    let report = compile_source(
        &source(old_none),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let helper = core
        .functions
        .iter()
        .position(|f| {
            f.callable_parameters
                .iter()
                .any(|r| r.source_life.is_some())
        })
        .unwrap();
    for mutation in 0..3 {
        let mut bad = core.clone();
        match mutation {
            0 => bad.functions[helper].callable_parameters[0].kind = CallableKind::FnOnce,
            1 => {
                let function = &mut bad.functions[helper];
                let role = &function.callable_parameters[0];
                function.parameters[role.index as usize].ty = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::Named(role.generic_name.clone())),
                };
            }
            _ => {
                let context = bad.functions[helper].callable_parameters[0]
                    .source_life
                    .as_mut()
                    .unwrap();
                context
                    .inputs
                    .iter_mut()
                    .find(|i| matches!(i.position, CoreClosureHeaderPosition::Class(_)))
                    .unwrap()
                    .header_binder = StorageBinder::Fixed(99);
            }
        }
        let errors = zil_types::check(&bad, &target, &limits).unwrap_err();
        assert!(
            !errors
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit),
            "{mutation}: {errors:?}"
        );
    }
    eprintln!(
        "MUTABLE_HELPER_LIFE Types N={low}/N-1/fresh and repeated actual FunctionSession requests; qualified profile has no scalar reuse claim"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn mutable_helper_life_stateful_and_adapted_source_native_clang_api_o0_o2_full_results() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let mut runs = 0;
    for (case, text, expected) in [
        (
            "stateful",
            MUTABLE_HELPER_LIFE_STATEFUL,
            b"4243\n".as_slice(),
        ),
        ("reverse", MUTABLE_HELPER_LIFE_REVERSE, b"4243\n".as_slice()),
        ("adapted", MUTABLE_HELPER_LIFE_ADAPTED, b"42\n".as_slice()),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-mutable_helper_life-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("main".into()),
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
                assert_eq!(report.verification_runs, 1);
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let result = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(result.status.success(), "{result:?}");
                assert_eq!(result.stdout, expected);
                assert!(result.stderr.is_empty());
                std::fs::remove_dir_all(dir).unwrap();
                runs += 1;
            }
        }
    }
    eprintln!(
        "MUTABLE_HELPER_LIFE {runs} real Source native runs; stateful complete42 and43 individually checked before stdout4243, exit0/no fallback"
    );
}

const OWNING_HELPER_LIFE_OWNED: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/helper-owned-byvalue.zil"
);
const OWNING_HELPER_LIFE_REVERSE: &str = include_str!(
    "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/reverse-owned-byvalue.zil"
);

#[test]
fn owning_helper_life_verify_staged_actual_source_and_raw_wire_neighbors() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let caller = include_str!(
        "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/actual-caller-good.zil"
    );
    let spare = include_str!(
        "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/owned-ssa-spare.zil"
    );
    for text in [
        OWNING_HELPER_LIFE_OWNED,
        OWNING_HELPER_LIFE_REVERSE,
        spare,
        caller,
    ] {
        let raw = zil_types::check(&shared_helper_life_core(text), &target, &limits).unwrap();
        let helper = raw
            .functions
            .iter()
            .find(|f| f.callable_contract.is_some())
            .unwrap();
        let stage = helper
            .blocks
            .iter()
            .flat_map(|bb| &bb.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::Store { local, value }
                    if matches!(helper.locals[local.0 as usize].ty, Type::Closure(_)) =>
                {
                    Some((local, value))
                }
                _ => None,
            })
            .unwrap();
        assert!(
            !helper.parameters.contains(&stage.0),
            "real staging was removed"
        );
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        if text != OWNING_HELPER_LIFE_OWNED {
            continue;
        }
        for case in 0..5 {
            let mut changed = raw.clone();
            let mut added = None;
            let f = &mut changed.functions[helper.id.0 as usize];
            let bb = f.blocks.iter_mut().find(|bb| bb.instructions.iter().any(|i| matches!(i.kind, InstructionKind::Store { local, .. } if local == stage.0))).unwrap();
            let store = bb
                .instructions
                .iter()
                .position(
                    |i| matches!(i.kind, InstructionKind::Store { local, .. } if local == stage.0),
                )
                .unwrap();
            match case {
                0 => { let duplicate = bb.instructions[store].clone(); bb.instructions.insert(store + 1, duplicate); }
                1 => bb.instructions.iter_mut().find(|i| i.result == Some(stage.1)).unwrap().kind = InstructionKind::TakeLocal { local: stage.0 },
                2 => bb.instructions.iter_mut().find(|i| matches!(i.kind, InstructionKind::TakeLocal { local } if local == stage.0)).unwrap().kind = InstructionKind::Load(stage.0),
                3 => bb.terminator = Terminator::Jump(bb.id),
                4 => {
                    let mut duplicate = bb.instructions.iter().find(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. })).unwrap().clone();
                    let index = bb.instructions.iter().position(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. })).unwrap();
                    let value = ValueId(f.values.len() as u32);
                    f.values.push(Type::I32);
                    duplicate.result = Some(value);
                    added = Some(value);
                    bb.instructions.insert(index + 1, duplicate);
                }
                _ => unreachable!(),
            }
            let mut claim = plan.clone();
            if let Some(value) = added {
                claim.functions[helper.id.0 as usize].holds.push(ValueHold {
                    value,
                    roots: vec![],
                });
            }
            claim.module_fingerprint = changed.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&changed, &claim, &limits).unwrap();
            for wire in [false, true] {
                let result = if wire {
                    zil_verify::verify_module_bytes(&bytes, &target, &limits)
                } else {
                    zil_verify::verify_module(changed.clone(), claim.clone(), &target, &limits)
                };
                let errors = result.unwrap_err();
                let expected = [
                    "ambiguous or repeated Stores",
                    "before initialization",
                    "never copying Load",
                    "Return cleanup",
                    "consumes a callback role more than once",
                ][case];
                assert!(
                    errors[0].message.contains(expected),
                    "case {case}/{wire}: {errors:?}"
                );
                assert!(
                    errors.iter().any(|d| matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )),
                    "case {case}/{wire}: {errors:?}"
                );
                eprintln!(
                    "OWNING_HELPER_LIFE actual helper neighbor {case} wire={wire}: {}",
                    errors[0].message
                );
            }
        }
    }
    // Equal runtime Type bytes do not identify the actual shared class. This
    // reuses the earlier genuine right-parameter SSA rather than inventing one.
    let raw = zil_types::check(&shared_helper_life_core(caller), &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let mut changed = raw.clone();
    let f = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "apply" || f.name.ends_with(":apply"))
        .unwrap();
    let right = f.parameters[1];
    let value = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Load(local) if local == right))
        .unwrap()
        .result
        .unwrap();
    let call = f.blocks.iter_mut().flat_map(|b| &mut b.instructions).find(|i| matches!(i.kind, InstructionKind::Call { function, .. } if raw.functions[function.0 as usize].callable_contract.is_some())).unwrap();
    let InstructionKind::Call { arguments, .. } = &mut call.kind else {
        unreachable!()
    };
    assert_eq!(
        f.values[value.0 as usize],
        f.values[arguments[1].0 as usize]
    );
    arguments[1] = value;
    let mut claim = plan.clone();
    claim.module_fingerprint = changed.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&changed, &claim, &limits).unwrap();
    for result in [
        zil_verify::verify_module(changed, claim, &target, &limits),
        zil_verify::verify_module_bytes(&bytes, &target, &limits),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
        eprintln!(
            "OWNING_HELPER_LIFE actual equal-Type wrong class: {}",
            errors[0].message
        );
        assert!(
            errors[0].message.contains("retained class equality"),
            "{errors:?}"
        );
    }
    let mut changed = raw.clone();
    let f = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "apply" || f.name.ends_with(":apply"))
        .unwrap();
    let bb = f.blocks.iter_mut().find(|bb| bb.instructions.iter().any(|i| matches!(i.kind, InstructionKind::Call { function, .. } if raw.functions[function.0 as usize].callable_contract.is_some()))).unwrap();
    let index = bb.instructions.iter().position(|i| matches!(i.kind, InstructionKind::Call { function, .. } if raw.functions[function.0 as usize].callable_contract.is_some())).unwrap();
    let mut duplicate = bb.instructions[index].clone();
    let value = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    duplicate.result = Some(value);
    bb.instructions.insert(index + 1, duplicate);
    let mut claim = plan;
    claim.functions[f.id.0 as usize].holds.push(ValueHold {
        value,
        roots: vec![],
    });
    claim.module_fingerprint = changed.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&changed, &claim, &limits).unwrap();
    for result in [
        zil_verify::verify_module(changed, claim, &target, &limits),
        zil_verify::verify_module_bytes(&bytes, &target, &limits),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
        eprintln!(
            "OWNING_HELPER_LIFE actual caller duplicate owned SSA: {}",
            errors[0].message
        );
        assert!(
            errors[0]
                .message
                .contains("managed SSA value was not available exactly once"),
            "{errors:?}"
        );
    }
}

#[test]
fn owning_helper_life_verify_staged_actual_source_work_boundary() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let raw = zil_types::check(
        &shared_helper_life_core(OWNING_HELPER_LIFE_OWNED),
        &target,
        &limits,
    )
    .unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    for wire in [false, true] {
        let check = |n| {
            let tight = Limits {
                max_analysis_steps: n,
                ..limits.clone()
            };
            if wire {
                zil_verify::verify_module_bytes(&bytes, &target, &tight)
            } else {
                zil_verify::verify_module(raw.clone(), plan.clone(), &target, &tight)
            }
        };
        let expected = check(limits.max_analysis_steps).unwrap().fingerprint();
        let (mut low, mut high) = (1, limits.max_analysis_steps);
        while low < high {
            let middle = low + (high - low) / 2;
            match check(middle) {
                Ok(_) => high = middle,
                Err(errors)
                    if errors
                        .iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit) =>
                {
                    low = middle + 1
                }
                Err(errors) => panic!("{errors:?}"),
            }
        }
        for n in [low, low - 1, low, low] {
            let result = check(n);
            if n == low {
                assert_eq!(result.unwrap().fingerprint(), expected);
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit)
                );
            }
        }
        eprintln!(
            "OWNING_HELPER_LIFE genuine staged owning Source Verify wire={wire} N={low}; N-1/fresh/repeated/retry"
        );
    }
}

#[test]
fn owning_helper_life_actual_owning_source_types_forms_owned_receiver_and_complete_relations() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for text in [OWNING_HELPER_LIFE_OWNED, OWNING_HELPER_LIFE_REVERSE] {
        let core = shared_helper_life_core(text);
        let bound = shared_helper_life_role(&core);
        assert_eq!(bound.kind, CallableKind::FnOnce);
        if text == OWNING_HELPER_LIFE_REVERSE {
            let context = bound.source_life.as_ref().unwrap();
            assert_eq!(
                (context.generic_parameter, bound.generic_index, bound.index),
                (1, 0, 1)
            );
            assert_eq!(context.named_fixed[0].parameter, 2);
        }
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let definition = &raw.closures[0];
        assert_eq!(definition.kind, CallableKind::FnOnce);
        let Type::Managed(payload) = &definition.captures[0] else {
            panic!("true owning environment required")
        };
        assert_eq!(raw.resources.len(), 1);
        assert_eq!(payload.as_ref(), &raw.resources[0].payload);
        assert!(raw.resources[0].teardown.is_none());
        assert!(
            matches!(&definition.captures[1],Type::Address{mutable:false,inner}if matches!(inner.as_ref(),Type::BorrowedStorage{life,..}if life.binder_count==1&&matches!(life.slots.as_slice(),[slot]if slot.binder==0&&slot.mode==StoredBorrowMode::SharedAddress)))
        );
        let entry = &raw.functions[definition.function.0 as usize];
        assert_eq!(entry.storage_life, definition.storage_life);
        assert!(
            entry
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::TakeCapture { index: 0 }))
        );
        let helper = raw
            .functions
            .iter()
            .find(|f| f.callable_contract.is_some())
            .unwrap();
        let role = &helper.callable_contract.as_ref().unwrap().roles[0];
        assert_eq!(role.kind, CallableKind::FnOnce);
        assert_eq!(role.cleanup, CallableCleanup::Owned);
        assert_eq!(role.life, definition.storage_life);
        assert_eq!(
            helper.locals[helper.parameters[role.parameter as usize].0 as usize].ty,
            Type::Closure(definition.id)
        );
        for class in 0..definition.storage_life.as_ref().unwrap().fixed_binder_count {
            assert!(helper.storage_life.as_ref().unwrap().classes.iter().any(
                |row| row.position.base == StorageBase::Parameter(role.parameter)
                    && row.position.path.is_empty()
                    && row.position.class == class
            ));
        }
        assert!(
            helper
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
        );
        let take=helper.blocks.iter().flat_map(|b|&b.instructions).find(|i|matches!(i.kind,InstructionKind::TakeLocal{local}if local==helper.parameters[role.parameter as usize])).unwrap();
        assert_eq!(
            helper.values[take.result.unwrap().0 as usize],
            Type::Closure(definition.id)
        );
    }
}

#[test]
fn owning_helper_life_owning_source_context_budget_and_physical_gate_are_independent() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let core = shared_helper_life_core(OWNING_HELPER_LIFE_OWNED);
    let expected = zil_types::check(&core, &target, &limits).unwrap();
    let (mut low, mut high) = (1, limits.max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        match zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: middle,
                ..limits.clone()
            },
        ) {
            Ok(_) => high = middle,
            Err(errors)
                if errors
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit) =>
            {
                low = middle + 1
            }
            Err(errors) => panic!("{errors:?}"),
        }
    }
    for n in [low, low - 1, low] {
        let result = zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: n,
                ..limits.clone()
            },
        );
        if n == low {
            assert_eq!(result.unwrap(), expected)
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit)
            )
        }
    }
    zil_types::infer_source_closures(
        &core,
        &Limits {
            max_analysis_steps: 16384,
            ..limits.clone()
        },
    )
    .unwrap();
    let mut huge = core.clone();
    let f = huge
        .functions
        .iter_mut()
        .find(|f| {
            f.callable_parameters
                .iter()
                .any(|r| r.source_life.is_some())
        })
        .unwrap();
    let bound = &mut f.callable_parameters[0];
    bound.generic_name = "F".repeat(32768);
    f.parameters[bound.index as usize].ty = Type::Named(bound.generic_name.clone());
    let errors = zil_types::infer_source_closures(
        &huge,
        &Limits {
            max_analysis_steps: 16384,
            ..limits.clone()
        },
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{errors:?}"
    );
    let text = include_str!(
        "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/phantom-wrong-class.zil"
    );
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(
        report.diagnostics.iter().any(|d| d
            .message
            .contains("signature omits actual closure free Fixed domain")),
        "{}",
        report.to_json()
    );
    let mut phantom = shared_helper_life_core(text);
    assert!(
        phantom
            .functions
            .iter()
            .flat_map(|f| &f.callable_parameters)
            .all(|r| r.source_life.is_none())
    );
    let template = shared_helper_life_role(&core);
    let role = phantom
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.callable_parameters)
        .find(|r| r.kind == CallableKind::FnOnce)
        .unwrap();
    role.source_life = template.source_life.clone();
    role.life = template.life.clone();
    let errors = zil_types::check(&phantom, &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("shared qualified")),
        "{errors:?}"
    );
    eprintln!(
        "OWNING_HELPER_LIFE isolated whole Types N={low}/N-1/fresh; Named owned-F huge preflight and actual Phantom source/raw gates checked, no whole Source acceptance claim"
    );
}

#[test]
fn owning_helper_life_analyze_actual_owning_source_request_budget_is_reproducible() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let raw = zil_types::check(
        &shared_helper_life_core(OWNING_HELPER_LIFE_OWNED),
        &target,
        &limits,
    )
    .unwrap();
    let expected = zil_analyze::analyze(&raw, &limits).unwrap();
    let (mut low, mut high) = (1, limits.max_analysis_steps);
    while low < high {
        let n = low + (high - low) / 2;
        match zil_analyze::analyze(
            &raw,
            &Limits {
                max_analysis_steps: n,
                ..limits.clone()
            },
        ) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = n
            }
            Err(errors)
                if errors
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit) =>
            {
                low = n + 1
            }
            Err(errors) => panic!("{errors:?}"),
        }
    }
    for n in [low, low - 1, low] {
        let actual = zil_analyze::analyze(
            &raw,
            &Limits {
                max_analysis_steps: n,
                ..limits.clone()
            },
        );
        if n == low {
            assert_eq!(actual.unwrap(), expected)
        } else {
            assert!(
                actual
                    .unwrap_err()
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit)
            )
        }
    }
    eprintln!(
        "OWNING_HELPER_LIFE whole Analyze actual owning Source N={low}/N-1/fresh; exact honest CandidatePlan stable"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn owning_helper_life_actual_native_clang_api_o0_o2_preserves_complete_owned_results() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let mut runs = 0;
    for (case, text) in [
        ("owned", OWNING_HELPER_LIFE_OWNED),
        ("reverse", OWNING_HELPER_LIFE_REVERSE),
        (
            "distinct-inputs",
            include_str!(
                "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/actual-caller-good.zil"
            ),
        ),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-owning_helper_life-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("main".into()),
                        output: Some(dir.join("program")),
                        emit_dir: Some(dir.join("ir")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let result = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{case}/{backend:?}/{optimization:?}: {result:?}"
                );
                assert_eq!(result.stdout, b"42\n");
                assert!(result.stderr.is_empty());
                // Retain one compact actual lowering/codegen artifact for the
                // independent owner flag/TakeCapture inspection, not all runs.
                if case == "owned"
                    && backend == NativeBackend::Clang
                    && optimization == Optimization::O0
                {
                    eprintln!(
                        "OWNING_HELPER_LIFE owner artifact retained {}",
                        dir.display()
                    );
                } else {
                    std::fs::remove_dir_all(dir).unwrap();
                }
                runs += 1;
            }
        }
    }
    eprintln!(
        "OWNING_HELPER_LIFE {runs} native processes; full stdout42 newline/exit0/empty stderr/currentVerify/no fallback, real owned transfer not a Fn/Mut adapter; teardown=None has no observable user Drop counter"
    );
}

#[test]
fn owning_helper_life_actual_source_consumption_and_class_neighbors_keep_their_rejection_stage() {
    use zil_base::Phase;
    for (text, phase, message) in [
        (
            include_str!(
                "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/helper-double-invoke-byvalue.zil"
            ),
            Phase::Expand,
            "environment was consumed",
        ),
        (
            include_str!(
                "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/second-consume-byvalue.zil"
            ),
            Phase::Analyze,
            "not initialized",
        ),
        (
            include_str!(
                "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/wrong-class-byvalue.zil"
            ),
            Phase::Analyze,
            "actual retained classes",
        ),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, phase);
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains(message)),
            "{}",
            report.to_json()
        );
    }
    let report = compile_source(
        &source(include_str!(
            "../../../tests/language-readiness/terminal/342/fnonce-helper-bridge/direct-owned-control.zil"
        )),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let drop = OWNING_HELPER_LIFE_OWNED.replace(
        "resource Mark{n:i32}",
        "resource Mark{n:i32}impl Drop for Mark{fn drop(self:Self){}}",
    );
    let report = compile_source(
        &source(&drop),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[path = "../../../tests/language-readiness/terminal/342/fnonce-helper-next/regressions.rs"]
mod helper_tail_return_tail;

#[path = "../../../tests/language-readiness/terminal/342/fnonce-helper-cfg/regressions.rs"]
mod helper_cfg_join_cfg;

#[path = "../../../tests/language-readiness/terminal/342/fnonce-helper-p0-review338/regressions.rs"]
mod helper_cfg_join_review;

#[path = "../../../tests/language-readiness/terminal/342/callable-completion/regressions.rs"]
mod helper_life_completion_completion;

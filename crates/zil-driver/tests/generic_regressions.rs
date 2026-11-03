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

#[cfg(target_os = "linux")]
fn recovery317_project(entry: &str) -> zil_driver::ProjectSources {
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
#[cfg(target_os = "linux")]
fn recovery317_component_source_and_nearest_rejections() {
    let report =
        zil_driver::compile_project(&recovery317_project("main"), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for entry in ["controls/shared_window_end", "controls/callback_alias_end"] {
        let report =
            zil_driver::compile_project(&recovery317_project(entry), &CompileOptions::default());
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
        let input = recovery317_project(&format!("negative/{entry}"));
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
fn recovery317_component_native_success_error_and_null_cleanup() {
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
                &recovery317_project("main"),
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
const THREAD325_RETURN_FACTORY: &str = "fn worker(n:i32)->i32{return n+1}fn make()->impl Fn()->i32{let result=thread_scope(worker,7,()->i32=>1);let x=result.1;return move()->i32=>x}fn main()->i32{let f=make();return f()-8}";

#[test]
#[cfg(target_os = "linux")]
fn thread325_review_direct_factory_scalar_graph_and_once_body() {
    let report = compile_source(
        &source(THREAD325_RETURN_FACTORY),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let src = source(THREAD325_RETURN_FACTORY);
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
fn thread325_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/mainline/concurrency325")
}

#[cfg(target_os = "linux")]
fn thread325_project() -> zil_driver::ProjectSources {
    use std::collections::BTreeMap;
    let mut paths = BTreeMap::new();
    let modules = ["main", "worker", "observer", "component"]
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let index = index as u32;
            let path = thread325_root().join(format!("{name}.zil"));
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
fn thread325_source_and_worker_boundaries() {
    let report = zil_driver::compile_project(&thread325_project(), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let mut private = thread325_project();
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
        std::fs::read_to_string(thread325_root().join("controls/parent_borrow.zil")).unwrap();
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
        let path = thread325_root().join(format!("negative/{name}.zil"));
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
    for (name, phase) in [
        ("parent_closure", zil_base::Phase::Types),
        ("callable_wrapper", zil_base::Phase::Expand),
    ] {
        let path = thread325_root().join(format!("pending/{name}.zil"));
        let text = std::fs::read_to_string(path).unwrap();
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{name}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"),
            "{name}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
    for (name, phase) in [
        ("parent_closure", zil_base::Phase::Types),
        ("callable_wrapper", zil_base::Phase::Expand),
    ] {
        let path = thread325_root().join(format!("pending/{name}.zil"));
        let text = std::fs::read_to_string(path).unwrap();
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{name}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"),
            "{name}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
#[cfg(target_os = "linux")]
fn thread325_raw_wire_worker_identity_and_once_body() {
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
    // A well-formed concrete parent closure is a future legal placement, not
    // a forged worker authority. Both raw kernels must report Unsupported.
    let mut future = typed.clone();
    let id = zil_ir::ClosureId(future.closures.len() as u32);
    future.functions[caller].environment = Some(id);
    future.closures.push(zil_ir::ClosureDefinition {
        id,
        function: future.functions[caller].id,
        captures: vec![],
        parameters: vec![],
        result: zil_ir::Type::I32,
        kind: zil_ir::CallableKind::Fn,
        lending_captures: vec![],
    });
    let errors = zil_analyze::analyze(&future, &limits).unwrap_err();
    assert_eq!(
        errors[0].category,
        ResultCategory::Unsupported,
        "{errors:?}"
    );
    let mut claim = plan.clone();
    claim.module_fingerprint = future.fingerprint();
    let errors = zil_verify::verify_module(future, claim, &target, &limits).unwrap_err();
    assert_eq!(
        errors[0].category,
        ResultCategory::Unsupported,
        "{errors:?}"
    );
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
fn thread325_native_os_join_parent_cleanup_and_failure() {
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
                .arg(thread325_root().join("observer.c"))
                .arg("-o")
                .arg(&observer)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            let wrapper = directory.join("link.sh");
            fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -pthread -Wl,--wrap=pthread_create -Wl,--wrap=pthread_join\nfi\nexec {} \"$@\"\n", quote(&clang), quote(&observer), quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let units = if backend == NativeBackend::LlvmApi && optimization == Optimization::O2 {
                4
            } else {
                1
            };
            let report = zil_driver::compile_project(
                &thread325_project(),
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
            for (mode, status, signal, stdout) in [
                (b'0', Some(0), None, "PECBDJA"),
                (b'1', Some(1), None, "PECBDJA"),
                (b'2', Some(2), None, "PECDA"),
                (b'3', None, Some(6), "PECBDX"),
            ] {
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
            for control in ["parent_borrow", "life_trait", "return_factory"] {
                let text = if control == "return_factory" {
                    THREAD325_RETURN_FACTORY.to_owned()
                } else {
                    fs::read_to_string(thread325_root().join(format!("controls/{control}.zil")))
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
                assert_eq!(actual.status.code(), Some(0), "{actual:?}");
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

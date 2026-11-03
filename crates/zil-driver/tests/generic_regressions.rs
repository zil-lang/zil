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
    for entry in [
        "shared_window",
        "callback_alias",
        "nominal_config",
        "phantom_config",
    ] {
        let report = zil_driver::compile_project(
            &recovery317_project(&format!("negative/{entry}")),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{entry}: {}",
            report.to_json()
        );
        assert!(report.artifacts.get("executable").is_none());
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

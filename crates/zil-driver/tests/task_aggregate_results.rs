//! Actual ordinary aggregate results across a stable task frame.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/task-aggregate-results/enum-result.zil");
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "task-aggregate.zil", text)
}
fn cases() -> Vec<(&'static str, String, &'static str)> {
    let checked = SOURCE.replace("    drive(&mut task);\n    drive(&mut task);", "    match drive(&mut task){.ok(value)=>{if value!=0{return 91;}},.err(_)=>{return 92;}}\n    match drive(&mut task){.ok(value)=>{if value!=65{return 93;}},.err(_)=>{return 94;}}");
    let input = SOURCE
        .replace(
            "fn worker(give token:Token)->Packet",
            "fn worker(packet:Packet)->Packet",
        )
        .replace("    let packet=Packet.Item(give token);\n", "")
        .replace(
            "task_create(worker,give input)",
            "task_create(worker,Packet.Item(give input))",
        );
    let nested = SOURCE.replace("enum Packet { Empty, Item(manages Token) }", "package Inner { manages token:Token; flag:i32 }\npackage Outer { inner:Inner; extra:i32 }\nenum Packet { Empty, Item(Outer) }").replace("Packet.Item(give token)", "Packet.Item(Outer{inner:Inner{token:give token,flag:7},extra:9})").replace("return value.code;", "let extra=value.extra;let inner=value.inner;let flag=inner.flag;return inner.token.code+flag+extra-16;");
    let child = SOURCE.replace("fn worker(give token:Token)", "fn child(give token:Token)").replace("fn drive(", "fn worker(give token:Token)->Packet { return task_await(child,give token); }\nfn drive(");
    let tuple = SOURCE.replace("enum Packet { Empty, Item(manages Token) }", "")
        .replace("->Packet", "->(Managed<Token>,i32)")
        .replace("let packet=Packet.Item(give token);", "let packet=(give token,7);")
        .replace("Task<Packet>", "Task<(Managed<Token>,i32)>")
        .replace("match packet {\n                Packet.Empty()=>{return 1;},\n                Packet.Item(value)=>{unsafe{putchar(82);}return value.code;}\n            }", "let (value,flag)=packet;unsafe{putchar(82);}return value.code+flag-7;");
    let product = SOURCE.replace("enum Packet { Empty, Item(manages Token) }", "package Packet { manages value:Token; flag:i32 }")
        .replace("let packet=Packet.Item(give token);", "let packet=Packet{value:give token,flag:7};")
        .replace("match packet {\n                Packet.Empty()=>{return 1;},\n                Packet.Item(value)=>{unsafe{putchar(82);}return value.code;}\n            }", "unsafe{putchar(82);}let flag=packet.flag;return packet.value.code+flag-7;");
    let array = SOURCE.replace("enum Packet { Empty, Item(manages Token) }", "")
        .replace("->Packet", "->[i32;2]")
        .replace("let packet=Packet.Item(give token);", "let packet=[token.code,7];")
        .replace("Task<Packet>", "Task<[i32;2]>")
        .replace("match packet {\n                Packet.Empty()=>{return 1;},\n                Packet.Item(value)=>{unsafe{putchar(82);}return value.code;}\n            }", "unsafe{putchar(82);}return packet[0]+packet[1]-7;");
    let borrowed = SOURCE
        .replace(
            "fn worker(give token:Token)",
            "fn worker(pointer:&i32,give token:Token)",
        )
        .replace(
            "    task_yield();",
            "    task_yield();if *pointer!=7 {return Packet.Empty;}",
        )
        .replace(
            "fn drive(task:&mut Task<Packet>)",
            "fn drive<life a>(task:&mut Task<Packet,&'a i32>)",
        )
        .replace(
            "var task=task_create(worker,give input)?;",
            "let number=7;var task=task_create(worker,&number,give input)?;",
        );
    let error = include_str!("../../../tests/language-readiness/task-composition/verified-source-consumers/owned-result-task.zil").to_owned();
    vec![
        ("ready", checked, "PRA"),
        (
            "cancel-pc0",
            SOURCE.replace("    drive(&mut task);\n    drive(&mut task);\n", ""),
            "A",
        ),
        (
            "cancel-pending",
            SOURCE.replace(
                "    drive(&mut task);\n    drive(&mut task);",
                "    drive(&mut task);",
            ),
            "PA",
        ),
        (
            "unobserved-ready",
            SOURCE.replace(
                "    drive(&mut task);\n    drive(&mut task);",
                "    drive(&mut task);\n    task_poll(&mut task);",
            ),
            "PA",
        ),
        ("enum-input", input, "PRA"),
        ("nested-product-result", nested, "PRA"),
        ("child-result", child, "PRA"),
        ("tuple-result", tuple, "PRA"),
        ("product-result", product, "PRA"),
        ("copy-array-result", array, "PAR"),
        ("borrowed-input-owned-result", borrowed, "PRA"),
        ("error-ok", error.clone(), "PRA"),
        (
            "error-err",
            error.replace(
                "task_create(worker,give input,false)",
                "task_create(worker,give input,true)",
            ),
            "PAE",
        ),
    ]
}
#[test]
fn aggregate_result_source_probe() {
    let mut failures = Vec::new();
    for (name, text, _) in cases() {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        if report.status != ResultCategory::Success || report.verification_runs != 1 {
            failures.push(format!("{name}: {}", report.to_json()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn aggregate_result_queued_nested_capture_source() {
    let text = include_str!(
        "../../../tests/language-readiness/task-aggregate-results/queued-nested-result.zil"
    );
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Verify,
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

#[test]
fn aggregate_result_native_once_transfer_and_cancel() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text, expected) in cases() {
                let dir = std::env::temp_dir().join(format!(
                    "zil-task-aggregate-{}-{name}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                fs::create_dir_all(&dir).unwrap();
                let clang = std::env::var_os("ZIL_CLANG")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "clang".into());
                let object = dir.join("observer.o");
                let observer = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/language-readiness/task-aggregate-results/frame-observer.c");
                let built = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(observer)
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert!(built.status.success(), "{built:?}");
                let quote =
                    |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
                let wrapper = dir.join("link.sh");
                fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&object),quote(&clang))).unwrap();
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
                let report = compile_source(
                    &source(&text),
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
                    "{name}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let output = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(output.status.code(), Some(0), "{name}: {output:?}");
                assert_eq!(output.stdout, expected.as_bytes(), "{name}: {output:?}");
                let failed = Command::new(&report.artifacts["executable"])
                    .env("ZIL_TASK_AGGREGATE_FAIL", "1")
                    .output()
                    .unwrap();
                assert_eq!(
                    failed.status.code(),
                    Some(1),
                    "allocation failure {name}: {failed:?}"
                );
                assert_eq!(failed.stdout, b"A", "allocation failure {name}: {failed:?}");
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
#[test]
fn aggregate_result_independent_raw_wire_resource_identity() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let input = source(SOURCE);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for wire in [false, true] {
        let verified = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap()
        } else {
            zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap()
        };
        let worker = typed.functions.iter().find(|f| f.name == "worker").unwrap();
        assert_eq!(
            verified.task_ownership(worker.id).unwrap().result_type(),
            Some(&worker.return_type)
        );
    }
    let token = typed
        .resources
        .iter()
        .position(|r| r.name.ends_with("Token"))
        .unwrap();
    for mutation in 0..3 {
        let mut raw = typed.clone();
        match mutation {
            0 => raw.resources[token].name.push_str(".forged"),
            1 => raw.resources.push(raw.resources[token].clone()),
            _ => {
                raw.resources[token].teardown = Some(
                    raw.functions
                        .iter()
                        .find(|f| f.foreign.is_some())
                        .unwrap()
                        .id,
                )
            }
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze mutation {mutation}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let rejected = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err()
            } else {
                zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err()
            };
            assert!(rejected, "Verify mutation {mutation}, wire={wire}");
        }
    }
}

#[test]
fn aggregate_result_queued_nested_capture_late_entry_and_cancel() {
    let normal = include_str!(
        "../../../tests/language-readiness/task-aggregate-results/queued-nested-result.zil"
    );
    let cancelled = normal
        .replace(
            "    drive(&mut task);\n    drive(&mut task);",
            "    drive(&mut task);\n    drive(&mut task);\n    task_request_cancel(&mut task);",
        )
        .replace(
            "    drive(&mut task);\n    match task_poll_close",
            "    match task_poll_close",
        )
        .replace(
            "extern \"C\" fn async_complete()->i32",
            "extern \"C\" fn aggregate_async_cancel_complete()->i32",
        )
        .replace(
            "return async_complete()",
            "return aggregate_async_cancel_complete()",
        );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text) in [("normal", normal), ("cancel", cancelled.as_str())] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-task-queued-aggregate-{}-{name}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                fs::create_dir_all(&dir).unwrap();
                let clang = std::env::var_os("ZIL_CLANG")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "clang".into());
                let object = dir.join("observer.o");
                let observer = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../../tests/language-readiness/task-aggregate-results/queued-observer.c",
                );
                let built = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(observer)
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert!(built.status.success(), "{built:?}");
                let quote =
                    |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
                let wrapper = dir.join("link.sh");
                fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&object),quote(&clang))).unwrap();
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
                    "{name}: {}",
                    report.to_json()
                );
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let mut command = Command::new(&report.artifacts["executable"]);
                if name == "normal" {
                    command
                        .env("ZIL_ASYNC_NORMAL", "1")
                        .env("ZIL_ASYNC_RESULT", "1");
                }
                let output = command.output().unwrap();
                assert_eq!(output.status.code(), Some(0), "{name}: {output:?}");
                assert!(
                    String::from_utf8(output.stdout)
                        .unwrap()
                        .contains("no-future before")
                );
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn aggregate_result_retained_input_loan_survives_pending_and_ready() {
    let text = cases()
        .into_iter()
        .find(|(name, _, _)| *name == "borrowed-input-owned-result")
        .unwrap()
        .1
        .replace("let number=7;", "var number=7;");
    for (name, old, new) in [
        (
            "before-poll",
            "    drive(&mut task);\n    drive(&mut task);",
            "    number=9;\n    drive(&mut task);\n    drive(&mut task);",
        ),
        (
            "pending",
            "    drive(&mut task);\n    drive(&mut task);",
            "    drive(&mut task);\n    number=9;\n    drive(&mut task);",
        ),
        (
            "ready",
            "    task_request_cancel(&mut task);",
            "    number=9;\n    task_request_cancel(&mut task);",
        ),
    ] {
        let input = text.replace(old, new);
        let report = compile_source(&source(&input), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
    }
    let released = text.replace(
        "    task_finish_destroy(give task);",
        "    task_finish_destroy(give task);number=9;",
    );
    let report = compile_source(&source(&released), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

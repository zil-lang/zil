//! Actual ordinary aggregate results across a stable task frame.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::Command,
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/task-aggregate-results/enum-result.zil");
const INLINE_REGISTRATION: &str =
    include_str!("../../../tests/language-readiness/task-aggregate-results/inline-registration-tuple.zil");
const PARTIAL_REGISTRATION: &str =
    include_str!("../../../tests/language-readiness/task-aggregate-results/inline-registration-tuple-selective.zil");
const INLINE_CHILD: &str =
    include_str!("../../../tests/language-readiness/task-aggregate-results/inline-task-enum.zil");
const PARTIAL_CHILD: &str =
    include_str!("../../../tests/language-readiness/task-aggregate-results/inline-task-enum-selective.zil");
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
fn queued_registration_drains_before_explicit_destruction() {
    let queued = include_str!(
        "../../../tests/language-readiness/task-aggregate-results/queued-nested-result.zil"
    );
    let (setup, _) = queued.split_once("    unsafe {if async_pump()!=0").unwrap();
    let drained = format!("{setup}
        unsafe {{let observed=async_pump();}}
        drive(&mut task);
        while true {{
            match task_poll_close(&mut task) {{
                std.task.Poll.Pending()=>{{}},
                std.task.Poll.Ready(_)=>{{task_finish_destroy(give task);unsafe{{return async_complete();}}}}
            }}
        }}
    }}");
    let report = compile_source(&source(&drained), &CompileOptions {
        stop_after: Stage::Verify, ..CompileOptions::default()
    });
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn queued_registration_cleanup_requires_the_actual_suspend_frame() {
    let queued = include_str!(
        "../../../tests/language-readiness/task-aggregate-results/queued-nested-result.zil"
    );
    let direct = queued.replace("fn worker(seed:i32)->Packet {",
        "fn worker(seed:i32)->Packet contract{effects(ffi);} {");
    for (text, code, message) in [
        (direct.replace("task_yield();", ""), "E-SEMANTIC-PROTOCOL", "live AsyncRegistration owner"),
        (direct, "E-SEMANTIC-ORIGINAL", "Direct ABI upper bound"),
        (queued.replace("task_yield();", "async_registration_finish_destroy_checked(give owner);task_yield();"),
            "E-SEMANTIC-PROTOCOL", "not dominated by Closed"),
    ] {
        let report = compile_source(&source(&text), &CompileOptions {
            stop_after: Stage::Verify, ..CompileOptions::default()
        });
        assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
        assert!(report.diagnostics.iter().any(|d| d.code == code && d.message.contains(message)),
            "{}", report.to_json());
    }
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
fn queued_candidate(limits: &Limits, target: &zil_base::TargetProfile) -> zil_ir::TypedModule {
    candidate(
        include_str!(
            "../../../tests/language-readiness/task-aggregate-results/queued-nested-result.zil"
        ),
        limits,
        target,
    )
}

fn candidate(text: &str, limits: &Limits, target: &zil_base::TargetProfile) -> zil_ir::TypedModule {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let aliases = Default::default();
    let scope = SourceIdentityContext::ClosedSnapshot {
        root: input.module_id,
        aliases: &aliases,
        declarations: DeclarationPlacement::Original,
    };
    let expanded =
        zil_expand::expand_with_map_semantics(&input, &ast, limits, target, scope).unwrap();
    zil_types::check(&expanded.core, target, limits).unwrap()
}

#[test]
fn queued_registered_package_rebuilds_from_the_actual_callback_and_provider() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let mut typed = queued_candidate(&limits, &target);
    typed.semantic = None;
    let proved = zil_analyze::analyze(&typed, &limits).unwrap();
    for mutation in 0..3 {
        let mut module = typed.clone();
        if mutation == 1 {
            let registration = module
                .resources
                .iter_mut()
                .find(|row| matches!(row.payload, zil_ir::Type::AsyncRegistration { .. }))
                .unwrap();
            registration.name.push_str(".forged");
        }
        if mutation == 2 {
            let start = module
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.instructions)
                .find(|i| {
                    matches!(
                        i.kind,
                        zil_ir::InstructionKind::AsyncRegistrationStart { .. }
                    )
                })
                .unwrap();
            let zil_ir::InstructionKind::AsyncRegistrationStart {
                request_close,
                poll_drain,
                ..
            } = &mut start.kind
            else {
                unreachable!()
            };
            std::mem::swap(request_close, poll_drain);
        }
        let accepted = mutation == 0;
        assert_eq!(zil_analyze::analyze(&module, &limits).is_ok(), accepted);
        let mut claim = proved.clone();
        claim.module_fingerprint = module.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&module, &claim, &limits).unwrap();
        assert_eq!(
            zil_verify::verify_module(module, claim, &target, &limits).is_ok(),
            accepted
        );
        assert_eq!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_ok(),
            accepted
        );
    }
}

#[test]
fn queued_result_transfers_only_the_actual_poll_arm() {
    use zil_ir::{InstructionKind, Terminator};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let mut module = queued_candidate(&limits, &target);
    let mut claim = zil_analyze::analyze(&module, &limits).unwrap();
    let drive = module
        .functions
        .iter_mut()
        .find(|function| function.name == "drive")
        .unwrap();
    let branch = drive
        .blocks
        .iter_mut()
        .find(|block| {
            block
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.kind, InstructionKind::TaskPoll { .. }))
        })
        .unwrap();
    let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = &mut branch.terminator
    else {
        panic!("expected the actual TaskPoll result test");
    };
    std::mem::swap(then_block, else_block);
    let rejected = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(
        rejected.iter().any(|d| d.code == "E-ANALYZE-ERROR-TAG"),
        "{rejected:?}"
    );
    claim.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &claim, &limits).unwrap();
    for rejected in [
        zil_verify::verify_module(module, claim, &target, &limits),
        zil_verify::verify_module_bytes(&bytes, &target, &limits),
    ] {
        let errors = rejected
            .err()
            .expect("a swapped owning result tag must be rejected");
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-ERROR-TAG"),
            "{errors:?}"
        );
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

fn linked_observer(dir: &Path, name: &str) -> PathBuf {
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let object = dir.join("observer.o");
    let observer = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/task-aggregate-results")
        .join(name);
    let built = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(observer)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(built.status.success(), "{built:?}");
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    let wrapper = dir.join("link.sh");
    fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&object),quote(&clang))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    wrapper
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
                let wrapper = linked_observer(&dir, "queued-observer.c");
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

#[test]
fn aggregate_owning_enum_payload_keeps_the_declared_value_role() {
    let explicit = SOURCE.replace("Item(manages Token)", "Item(Managed<Token>)");
    let report = compile_source(
        &source(&explicit),
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
    for rejected in [
        SOURCE.replace("Item(manages Token)", "Item(Token)"),
        SOURCE.replace("Packet.Item(give token)", "Packet.Item(token)"),
    ] {
        let report = compile_source(
            &source(&rejected),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "owning payload role was erased: {}",
            report.to_json()
        );
    }
}

#[test]
fn aggregate_child_await_preserves_its_static_worker_and_suspend_abi() {
    use zil_ir::InstructionKind;

    let (_, text, _) = cases()
        .into_iter()
        .find(|(name, _, _)| *name == "child-result")
        .unwrap();
    let source = text.replace("fn drive(", "fn sibling(give token:Token)->Packet { task_yield(); return Packet.Item(give token); }\nfn drive(");
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let typed = candidate(&source, &limits, &target);
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let sibling = typed
        .functions
        .iter()
        .find(|f| f.name.ends_with("sibling"))
        .unwrap()
        .id;
    for change in 0..3 {
        let mut module = typed.clone();
        let call = module
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|block| &mut block.instructions)
            .find(|op| matches!(op.kind, InstructionKind::SuspendCall { .. }))
            .unwrap();
        let InstructionKind::SuspendCall {
            function,
            arguments,
        } = &mut call.kind
        else {
            unreachable!()
        };
        match change {
            0 => {}
            1 => *function = sibling,
            _ => {
                call.kind = InstructionKind::Call {
                    function: *function,
                    arguments: arguments.clone(),
                }
            }
        }
        let mut proposed = plan.clone();
        proposed.module_fingerprint = module.fingerprint();
        assert_eq!(zil_analyze::analyze(&module, &limits).is_ok(), change == 0);
        let wire = zil_ir::wire::encode_module_candidate(&module, &proposed, &limits).unwrap();
        assert_eq!(
            zil_verify::verify_module(module, proposed, &target, &limits).is_ok(),
            change == 0
        );
        assert_eq!(
            zil_verify::verify_module_bytes(&wire, &target, &limits).is_ok(),
            change == 0
        );
    }
}

#[test]
fn retained_input_checks_real_owning_poll_arm_and_destinations() {
    use zil_ir::InstructionKind;

    let (_, text, _) = cases()
        .into_iter()
        .find(|(name, _, _)| *name == "borrowed-input-owned-result")
        .unwrap();
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let typed = candidate(&text, &limits, &target);
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for change in 0..3 {
        let mut module = typed.clone();
        let unpack = module.functions.iter_mut().find(|function| function.name.ends_with("drive")).unwrap()
            .blocks.iter_mut().flat_map(|block| &mut block.instructions)
            .find(|op| matches!(&op.kind, InstructionKind::UnpackVariant { fields, .. } if !fields.is_empty())).unwrap();
        let InstructionKind::UnpackVariant {
            local,
            fields,
            variant,
        } = &mut unpack.kind
        else {
            unreachable!()
        };
        match change {
            0 => {}
            1 => *variant = 0,
            _ => fields[0] = *local,
        }
        let mut proposed = plan.clone();
        proposed.module_fingerprint = module.fingerprint();
        assert_eq!(zil_analyze::analyze(&module, &limits).is_ok(), change == 0);
        let wire = zil_ir::wire::encode_module_candidate(&module, &proposed, &limits).unwrap();
        assert_eq!(
            zil_verify::verify_module(module, proposed, &target, &limits).is_ok(),
            change == 0
        );
        assert_eq!(
            zil_verify::verify_module_bytes(&wire, &target, &limits).is_ok(),
            change == 0
        );
    }
}

#[test]
fn inline_protocol_aggregates_follow_the_actual_continuation_and_cleanup_edge() {
    let queued = include_str!(
        "../../../tests/language-readiness/task-aggregate-results/queued-nested-result.zil"
    );
    let normal = format!(
        "{}fn main({}",
        INLINE_REGISTRATION.split_once("fn main(").unwrap().0,
        queued.split_once("fn main(").unwrap().1,
    );
    let folded_tuple =
        INLINE_REGISTRATION.replace("(give marker,give owner);", "((give marker,give owner),);");
    let folded_enum = INLINE_CHILD.replace("manages Task<i32>", "manages (Task<i32>)");
    let interrupted_tuple = INLINE_REGISTRATION
        .replace("tuple_close_complete", "tuple_interrupted_complete")
        .replace(
            "    drive(&mut task);\n    task_request_cancel",
            "    drive(&mut task);\n    drive(&mut task);\n    task_request_cancel",
        )
        .replace(
            "    match task_poll_close(&mut task) {\n        std.task.Poll.Pending()=>{}\n        std.task.Poll.Ready(_)=>{task_finish_destroy(give task);return 6;}\n    }\n",
            "",
        );
    let selective = |text: &str| {
        text.replace(
            "    let pair=(give marker,give owner);",
            "    let pair=(give marker,give owner);\n    let extracted=give pair.0;",
        )
    };
    let partial_normal = selective(&normal);
    let partial_interrupted = selective(&interrupted_tuple);
    let partial_moved_registration = PARTIAL_REGISTRATION.replace("give pair.0", "give pair.1");
    let poll_to_completion = |text: &str| text.replace(
        "    match task_poll(&mut task) {\n        .err(_)=>{return 90;}\n        .ok(poll)=>{match poll {\n            Poll.Pending()=>{}\n            Poll.Ready(_)=>{}\n        }}\n    }",
        "    loop {\n        match task_poll(&mut task) {\n            .err(_)=>{return 90;}\n            .ok(poll)=>{match poll {\n                Poll.Pending()=>{continue;}\n                Poll.Ready(_)=>{break;}\n            }}\n        }\n    }",
    );
    let enum_return = poll_to_completion(INLINE_CHILD);
    let enum_partial_return = poll_to_completion(PARTIAL_CHILD);
    let cases = [
        (
            "tuple-cancel",
            INLINE_REGISTRATION,
            "inline-registration-tuple-observer.c",
            "Anested tuple: no-future before release\n",
        ),
        (
            "tuple-folded",
            &folded_tuple,
            "inline-registration-tuple-observer.c",
            "Anested tuple: no-future before release\n",
        ),
        (
            "tuple-return",
            &normal,
            "inline-registration-tuple-observer.c",
            "Aqueued late entry rejected; no-future before two frees\n",
        ),
        (
            "tuple-cancel-after-pending-drop",
            &interrupted_tuple,
            "inline-registration-tuple-observer.c",
            "Anested tuple: cancelled after actual first drop Pending\n",
        ),
        (
            "tuple-partial-cancel",
            PARTIAL_REGISTRATION,
            "inline-registration-tuple-observer.c",
            "Anested tuple: no-future before release\n",
        ),
        (
            "tuple-partial-return",
            &partial_normal,
            "inline-registration-tuple-observer.c",
            "Aqueued late entry rejected; no-future before two frees\n",
        ),
        (
            "tuple-partial-failure",
            include_str!("../../../tests/language-readiness/task-aggregate-results/inline-registration-tuple-failure.zil"),
            "inline-registration-tuple-observer.c",
            "Apartial tuple: failure observed after no-future and both frees\n",
        ),
        (
            "tuple-partial-cancel-after-pending-drop",
            &partial_interrupted,
            "inline-registration-tuple-observer.c",
            "Anested tuple: cancelled after actual first drop Pending\n",
        ),
        (
            "tuple-partial-moved-registration",
            &partial_moved_registration,
            "inline-registration-tuple-observer.c",
            "Anested tuple: no-future before release\n",
        ),
        (
            "enum-cancel",
            INLINE_CHILD,
            "inline-task-enum-observer.c",
            "AB selected child closed between its neighboring fields\n",
        ),
        (
            "enum-folded",
            &folded_enum,
            "inline-task-enum-observer.c",
            "AB selected child closed between its neighboring fields\n",
        ),
        (
            "enum-return",
            &enum_return,
            "inline-task-enum-observer.c",
            "AB selected child closed between its neighboring fields\n",
        ),
        (
            "enum-partial-cancel",
            PARTIAL_CHILD,
            "inline-task-enum-observer.c",
            "AB selected child closed between its neighboring fields\n",
        ),
        (
            "enum-partial-return",
            &enum_partial_return,
            "inline-task-enum-observer.c",
            "AB selected child closed between its neighboring fields\n",
        ),
        (
            "enum-partial-failure",
            include_str!("../../../tests/language-readiness/task-aggregate-results/inline-task-enum-selective-failure.zil"),
            "inline-task-enum-observer.c",
            "AB selected child closed between its neighboring fields\n",
        ),
    ];
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text, observer, stdout) in cases {
                let dir = std::env::temp_dir().join(format!(
                    "zil-inline-task-{}-{name}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        clang: Some(linked_observer(&dir, observer)),
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
                let executable = &report.artifacts["executable"];
                let mut command = Command::new(executable);
                if matches!(name, "tuple-return" | "tuple-partial-return" | "tuple-partial-failure") {
                    command.env("ZIL_ASYNC_NORMAL", "1");
                }
                if matches!(name, "tuple-return" | "tuple-partial-return") {
                    command
                        .env("ZIL_ASYNC_RESULT", "1");
                }
                let actual = command.output().unwrap();
                assert_eq!(actual.status.code(), Some(0), "{name}: {actual:?}");
                assert_eq!(actual.stdout, stdout.as_bytes(), "{name}: {actual:?}");
                assert!(actual.stderr.is_empty(), "{name}: {actual:?}");
                if matches!(name, "tuple-cancel" | "tuple-partial-cancel") {
                    let invalid = Command::new(executable)
                        .env("ZIL_ASYNC_TRACE", "1")
                        .env("ZIL_ASYNC_INVALID_DRAIN", "1")
                        .output()
                        .unwrap();
                    assert_eq!(invalid.status.signal(), Some(4), "{invalid:?}");
                    assert_eq!(
                        invalid.stdout,
                        b"installed live=2 drops=0\nAinvalid-provider live=2 drops=0\n"
                    );
                    assert!(invalid.stderr.is_empty(), "{invalid:?}");
                }
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn mixed_enum_reference_requires_its_exact_original_field() {
    use zil_ir::{InstructionKind, PlaceProjection, VariantProjection};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let text = PARTIAL_CHILD
        .replace("manages Task<i32>,manages Mark) }", "manages Task<i32>,manages Mark,manages Mark) }")
        .replace("let packet=Packet.Live(give before,give task,give after);", "let neighbor=Mark{code:67};\n            let packet=Packet.Live(give before,give task,give after,give neighbor);")
        .replace("Packet.Live(extracted,_,ref observer)", "Packet.Live(extracted,_,ref observer,_)");
    let mut module = candidate(&text, &limits, &target);
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    let worker = module.functions.iter_mut().find(|function| function.name == "worker").unwrap();
    let field = worker.blocks.iter_mut().flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match &mut instruction.kind {
            InstructionKind::BorrowPlaceLocal { path, mutable: false, .. } => match path.as_mut_slice() {
                [PlaceProjection::Variant(field)] if *field == (VariantProjection { variant: 1, index: 2 }) => Some(field),
                _ => None,
            },
            _ => None,
        }).unwrap();
    field.index = 3;
    let rejected = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(rejected.iter().any(|d| d.code == "E-ANALYZE-IR"
        && d.message.contains("actual reference pattern changes its original root, active variant, field, or mode")), "{rejected:?}");

    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for checked in [
        zil_verify::verify_module(module, plan, &target, &limits),
        zil_verify::verify_module_bytes(&wire, &target, &limits),
    ] {
        let rejected = checked.err().expect("another active same-typed field cannot replace the original reference");
        assert!(rejected.iter().any(|d| d.code == "E-VERIFY-SEMANTIC"
            && d.message.contains("actual reference pattern changes its original active variant, field, or mode")), "{rejected:?}");
    }
}

#[test]
fn inline_tuple_move_requires_the_exact_original_field() {
    use zil_ir::{InstructionKind, PlaceProjection};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let text = PARTIAL_REGISTRATION.replace(
        "    let pair=(give marker,give owner);",
        "    let neighbor=Marker{code:66};\n    let pair=(give marker,give neighbor,give owner);",
    );
    let mut module = candidate(&text, &limits, &target);
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();

    let worker = module.functions.iter_mut().find(|f| f.name == "worker").unwrap();
    let take = worker.blocks.iter_mut().flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match &mut instruction.kind {
            InstructionKind::TakeTupleField { path, .. } => Some(path),
            _ => None,
        }).unwrap();
    assert_eq!(take, &[PlaceProjection::Field(0)]);
    take[0] = PlaceProjection::Field(1);
    let errors = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(errors.iter().any(|d| d.code == "E-ANALYZE-IR"
        && d.message.contains("takes another original place, field or full type")), "{errors:?}");

    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for checked in [
        zil_verify::verify_module(module, plan, &target, &limits),
        zil_verify::verify_module_bytes(&bytes, &target, &limits),
    ] {
        let errors = checked.err().expect("same-type sibling cannot replace the selected owner");
        assert!(errors.iter().any(|d| d.code == "E-VERIFY-SEMANTIC"
            && d.message.contains("actual field index differs from the original ordered nominal/tuple field")
            && d.notes.iter().any(|note| note.contains("original field \"0\"") && note.contains("ordinal 1"))), "{errors:?}");
    }
}

#[test]
fn inline_protocol_aggregates_reject_moved_sources_and_forged_raw_cleanup() {
    use zil_ir::InstructionKind;
    let cases = [
        INLINE_REGISTRATION.replace(
            "let pair=(give marker,give owner);",
            "let pair=(give marker,give owner);async_registration_valid(owner);",
        ),
        INLINE_CHILD.replace(
            "let packet=Packet.Live(give before,give task,give after);",
            "let packet=Packet.Live(give before,give task,give after);task_valid(task);",
        ),
        INLINE_CHILD.replace("manages Task<i32>", "manages Desc<Task<i32>>"),
        INLINE_REGISTRATION.replace(
            "    task_yield();",
            "    task_yield();let (observed,registration)=pair;var active=give registration;async_registration_finish_destroy_checked(give active);",
        ),
    ];
    for text in cases {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (text, swap_sources) in [
        (INLINE_REGISTRATION, false),
        (INLINE_CHILD, false),
        (INLINE_CHILD, true),
    ] {
        let module = candidate(text, &limits, &target);
        let plan = zil_analyze::analyze(&module, &limits).unwrap();
        let clean_wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
        zil_verify::verify_module_bytes(&clean_wire, &target, &limits).unwrap();

        let mut altered = module;
        let worker = altered
            .functions
            .iter_mut()
            .find(|row| row.name == "worker")
            .unwrap();
        let instruction = worker.blocks.iter_mut().flat_map(|block| &mut block.instructions)
            .find(|instruction| if swap_sources {
                matches!(&instruction.kind, InstructionKind::MakeVariant { fields, .. } if fields.len() == 3)
            } else {
                matches!(instruction.kind, InstructionKind::DropTuple { .. } | InstructionKind::DropEnum { .. })
            }).unwrap();
        match &mut instruction.kind {
            InstructionKind::MakeVariant { fields, .. } => {
                assert_eq!(
                    worker.values[fields[0].0 as usize],
                    worker.values[fields[2].0 as usize]
                );
                fields.swap(0, 2);
            }
            InstructionKind::DropTuple { local } | InstructionKind::DropEnum { local } => {
                instruction.kind = InstructionKind::StorageDead(*local);
            }
            _ => unreachable!(),
        }
        assert!(zil_analyze::analyze(&altered, &limits).is_err());
        let mut plan = plan;
        plan.module_fingerprint = altered.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&altered, &plan, &limits).unwrap();
        assert!(zil_verify::verify_module(altered, plan, &target, &limits).is_err());
        assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
    }
}

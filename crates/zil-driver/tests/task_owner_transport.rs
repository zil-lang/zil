use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/task-owner-transport/returned-task-fnonce.zil");
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "returned-task-fnonce.zil", text)
}

fn observer_linker(dir: &Path, clang: &Path, object: &Path) -> PathBuf {
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    let wrapper = dir.join("link.sh");
    fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n", quote(clang), quote(object), quote(clang))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    wrapper
}

#[test]
fn actual_close_exits_allow_only_the_current_callable_task_occupants() {
    let blocking = include_str!(
        "../../../tests/language-readiness/task-owner-transport/fnmut-async-close.zil"
    );
    let polling = include_str!(
        "../../../tests/language-readiness/task-owner-transport/fnmut-async-poll-close.zil"
    );
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let conditional = polling
        .replace(
            "fn keep(give original:Task<i32>)",
            "fn keep(give original:Task<i32>,wait:bool)",
        )
        .replace("keep(give original)", "keep(give original,false)")
        .replace(
            "  task_request_cancel(&mut task);\n  return task_poll_close(&mut task);",
            "  if wait{return Poll.Pending();}\n  task_request_cancel(&mut task);\n  return task_poll_close(&mut task);",
        );
    assert!(conditional.contains("if wait{return Poll.Pending();}"));
    for (name, text) in [
        ("blocking", blocking),
        ("polling", polling),
        ("conditional-pending", conditional.as_str()),
    ] {
        let report = compile_source(&source(text), &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let keep = blocking.find("fn keep(").unwrap();
    let close_loop = "  loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}\n";
    assert!(blocking[keep..].contains(close_loop));
    let request_only = format!(
        "{}{}",
        &blocking[..keep],
        blocking[keep..].replacen(close_loop, "", 1)
    );
    for (name, text) in [
        ("cancel-is-not-closed", request_only),
        (
            "arbitrary-ready",
            polling.replace(
                "return task_poll_close(&mut task);",
                "return Poll.Ready(());",
            ),
        ),
        (
            "pending-is-not-closed",
            polling
                .replace(
                    "return task_poll_close(&mut task);",
                    "return Poll.Pending();",
                )
                .replace(
                    "loop{match callback(){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}",
                    "callback();",
                ),
        ),
    ] {
        let report = compile_source(&source(&text), &options);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-TASK-ABI" || d.code == "E-VERIFY-TASK"),
            "{name}: {}",
            report.to_json()
        );
    }
}

#[test]
fn canonical_ready_cannot_be_replaced_by_an_equal_shaped_ready_raw_and_wire() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/fnmut-async-poll-close.zil"
    )
    .replace(
        "return task_poll_close(&mut task);",
        "let marker=();return task_poll_close(&mut task);",
    );
    let mut raw = typed_original(&text);
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let entry = raw
        .functions
        .iter_mut()
        .find(|f| {
            f.environment.is_some()
                && matches!(f.return_type, zil_ir::Type::Enum { .. })
                && f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, zil_ir::InstructionKind::TaskPollClose { .. }))
        })
        .unwrap();
    let unit = entry
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(
                i.kind,
                zil_ir::InstructionKind::Constant(zil_ir::Constant::Unit)
            )
            .then_some(i.result)
            .flatten()
        })
        .unwrap();
    let mut changed = 0;
    for i in entry.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if matches!(i.kind, zil_ir::InstructionKind::TaskPollClose { .. }) {
            i.kind = zil_ir::InstructionKind::MakeVariant {
                variant: 1,
                fields: vec![unit],
            };
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    let rejected = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        rejected.iter().any(|d| d.code == "E-TASK-ABI"),
        "{rejected:?}"
    );
    plan.module_fingerprint = raw.fingerprint();
    let rejected =
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        rejected.iter().any(|d| d.code == "E-VERIFY-TASK"),
        "{rejected:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    let rejected = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        rejected.iter().any(|d| d.code == "E-VERIFY-TASK"),
        "{rejected:?}"
    );
}

fn closed_pair_setup() -> &'static str {
    r#" let first=make_async_child()?;
 match make_async_child(){
  .err(reason)=>{close(give first);return reason;},
  .ok(second)=>{
"#
}

#[test]
fn closed_phase_keeps_both_same_type_capture_leaves_and_replacement_epochs() {
    let blocking = include_str!(
        "../../../tests/language-readiness/task-owner-transport/fnmut-async-close.zil"
    );
    let polling = include_str!(
        "../../../tests/language-readiness/task-owner-transport/fnmut-async-poll-close.zil"
    );
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let prefix = &blocking[..blocking.find("fn keep(").unwrap()];
    let keep = r#"fn keep(give first:Task<i32>,give second:Task<i32>)->impl FnMut()->i32 {
 var one=give first;var two=give second;
 return move()->i32=>{
  task_request_cancel(&mut one);task_request_cancel(&mut two);
  loop{match task_poll_close(&mut one){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
  loop{match task_poll_close(&mut two){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
  return 0;
 };
}

"#;
    let pair = format!(
        "{prefix}{keep}fn main()->TaskCreateError!i32{{\n{} var callback=keep(give first,give second);callback();return 0;\n}}}}}}",
        closed_pair_setup()
    );
    let report = compile_source(&source(&pair), &options);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let only_one = pair.replace("  loop{match task_poll_close(&mut two){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}\n", "");
    let report = compile_source(&source(&only_one), &options);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TASK-ABI" || d.code == "E-VERIFY-TASK"),
        "{}",
        report.to_json()
    );

    let prefix = &polling[..polling.find("fn main(").unwrap()];
    let before = r#" var callback=keep(give first);let replacement=keep(give second);
 var saved:std.task.Poll<()> = Poll.Pending();
 loop{saved=callback();match saved{Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 callback=replacement;
 match saved{Poll.Pending()=>{loop{}},Poll.Ready(_)=>{}}
"#;
    let close_new =
        " loop{match callback(){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}\n";
    let positive = format!(
        "{prefix}fn main()->TaskCreateError!i32{{\n{}{before}{close_new}return 0;\n}}}}}}",
        closed_pair_setup()
    );
    let report = compile_source(&source(&positive), &options);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let stale = positive.replace(close_new, "");
    let report = compile_source(&source(&stale), &options);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TASK-ABI" || d.code == "E-VERIFY-TASK"),
        "{}",
        report.to_json()
    );
}
#[test]
fn closed_callable_drop_is_drained_before_destruction_native() {
    for (name, text) in [
        (
            "blocking",
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/fnmut-async-close.zil"
            ),
        ),
        (
            "polling",
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/fnmut-async-poll-close.zil"
            ),
        ),
    ] {
        let text = format!(
            "extern \"C\" fn ready_observed();\nresource Witness{{value:i32}}\nimpl Drop for Witness{{fn drop(self:Self){{unsafe{{ready_observed();}}}}}}\n{}",
            text.replace(
                "var callback=keep(give original);",
                "var callback=keep(give original);let witness=Witness{value:1};"
            )
        );
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../../out/runs/task-closed-native/{name}-{backend:?}-{optimization:?}"
                ));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("source.zil"), &text).unwrap();
                let clang = std::env::var_os("ZIL_CLANG")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "clang".into());
                let observer = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../../tests/language-readiness/task-owner-transport/closed-frame-observer.c",
                );
                let object = dir.join("observer.o");
                let built = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(observer)
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert!(built.status.success(), "{built:?}");
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        clang: Some(observer_linker(&dir, &clang, &object)),
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                fs::write(dir.join("report.json"), report.to_json()).unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}: {}",
                    report.to_json()
                );
                let generated = report.native_codegen.as_ref().unwrap();
                assert_eq!(generated.backend, backend);
                assert!(generated.fallback.is_none());
                for failure in 0..=2 {
                    let output = Command::new(&report.artifacts["executable"])
                        .env("ZIL_CLOSED_FAIL_AT", failure.to_string())
                        .output()
                        .unwrap();
                    fs::write(
                        dir.join(format!("failure-{failure}.txt")),
                        format!("{output:?}"),
                    )
                    .unwrap();
                    assert_eq!(
                        output.status.code(),
                        Some(i32::from(failure == 2)),
                        "{name} failure {failure}: {output:?}"
                    );
                    assert!(output.stdout.is_empty(), "{output:?}");
                }
            }
        }
    }
}

fn typed_original(text: &str) -> zil_ir::TypedModule {
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    zil_types::check(&core, &target, &limits).unwrap()
}

#[test]
fn asynchronous_child_input_does_not_acquire_synchronous_parent_cleanup() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/child-frame-cleanup.zil"
    );
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let safe = compile_source(&source(text), &options);
    assert_eq!(safe.status, ResultCategory::Success, "{}", safe.to_json());
    let asynchronous = text.replace(
        "let child=make_safe_child()",
        "let child=make_async_child()",
    );
    let explicit = asynchronous.replace(
        ".ok(original)=>{return 0;}",
        ".ok(original)=>{return close(give original);}",
    );
    let drained = compile_source(&source(&explicit), &options);
    assert_eq!(
        drained.status,
        ResultCategory::Success,
        "{}",
        drained.to_json()
    );
    let rejected = compile_source(&source(&asynchronous), &options);
    assert_ne!(
        rejected.status,
        ResultCategory::Success,
        "{}",
        rejected.to_json()
    );
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Task") && d.message.contains("Drop")),
        "{}",
        rejected.to_json()
    );
}

#[test]
fn actual_child_factory_change_cannot_reuse_a_synchronous_cleanup_plan_raw_and_wire() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/child-frame-cleanup.zil"
    );
    let mut raw = typed_original(text);
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let safe = raw
        .functions
        .iter()
        .find(|f| f.name == "make_safe_child")
        .unwrap()
        .clone();
    let asynchronous = raw
        .functions
        .iter()
        .find(|f| f.name == "make_async_child")
        .unwrap()
        .clone();
    assert_eq!(safe.return_type, asynchronous.return_type);
    assert_eq!(safe.parameters.len(), asynchronous.parameters.len());
    let mut changed = 0;
    for i in raw
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let zil_ir::InstructionKind::Call { function, .. } = &mut i.kind {
            if *function == safe.id {
                *function = asynchronous.id;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    plan.module_fingerprint = raw.fingerprint();
    assert!(zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
}

#[test]
fn mutable_task_capture_keeps_parent_loans_until_actual_owner_cleanup() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/mutable-task-fnmut.zil"
    );
    for replacement in [
        "var callback=keep(give original);number=0;let answer=callback();",
        "var callback=keep(give original);number=0;let answer=0;",
    ] {
        let hostile = text.replace("let answer=drive_callback(give original);", replacement);
        let report = compile_source(
            &source(&hostile),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(
            matches!(
                report.phase,
                zil_base::Phase::Analyze | zil_base::Phase::Verify
            ),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn mutable_task_capture_requires_exact_mutable_slot_and_cleanup_raw_and_wire() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/mutable-task-fnmut.zil"
    );
    let typed = typed_original(text);
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let closure = typed
        .closures
        .iter()
        .find(|d| d.kind == zil_ir::CallableKind::FnMut)
        .unwrap();
    for mutation in 0..5 {
        let mut raw = typed.clone();
        match mutation {
            0 => {
                raw.closures[closure.id.0 as usize].kind = zil_ir::CallableKind::Fn;
                for f in &mut raw.functions {
                    if let Some(c) = &mut f.callable_return_contract {
                        if c.closure == closure.id {
                            c.kind = zil_ir::CallableKind::Fn;
                        }
                    }
                }
            }
            1 | 2 => {
                let f = &mut raw.functions[closure.function.0 as usize];
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, zil_ir::InstructionKind::BorrowCapture { .. }))
                    .unwrap();
                if mutation == 1 {
                    if let zil_ir::InstructionKind::BorrowCapture { index, .. } = &mut i.kind {
                        *index = 99;
                    }
                } else {
                    let result = i.result.unwrap();
                    if let zil_ir::Type::Address { mutable, .. } = &mut f.values[result.0 as usize]
                    {
                        *mutable = false;
                    } else {
                        panic!("actual Task head type");
                    }
                }
            }
            3 => {
                let life = raw.closures[closure.id.0 as usize]
                    .storage_life
                    .as_mut()
                    .unwrap();
                life.bindings.clear();
                raw.functions[closure.function.0 as usize].storage_life = Some(life.clone());
            }
            _ => {
                let c = raw
                    .functions
                    .iter_mut()
                    .find_map(|f| {
                        f.callable_return_contract
                            .as_mut()
                            .filter(|c| c.closure == closure.id)
                    })
                    .unwrap();
                assert!(!c.cleanup.is_empty());
                c.cleanup.clear();
            }
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze mutation {mutation}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        assert!(
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err(),
            "Verify mutation {mutation}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire mutation {mutation}"
        );
    }
}

#[test]
fn static_task_callback_requires_every_actual_constructor_even_outside_main_raw_and_wire() {
    let text = format!(
        "{}\nfn second_ctor(text:&[u8]) {{let callback=make();callback();}}\n",
        include_str!(
            "../../../tests/language-readiness/task-owner-transport/static-callable-input.zil"
        )
    );
    let mut raw = typed_original(&text);
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let maker = raw.functions.iter().find(|f| f.name == "make").unwrap().id;
    let zil_ir::Type::Closure(closure) = raw.functions[maker.0 as usize].return_type else {
        panic!("actual factory closure");
    };
    let f = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "second_ctor")
        .unwrap();
    let parameter = f.parameters[0];
    let value = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(f.locals[parameter.0 as usize].ty.clone());
    let function = f.id;
    let mut changed = None;
    for (block, b) in f.blocks.iter_mut().enumerate() {
        if let Some(index) = b.instructions.iter().position(|i| matches!(i.kind, zil_ir::InstructionKind::Call { function, .. } if function == maker)) {
            let at = b.instructions[index].span;
            b.instructions.insert(index, zil_ir::Instruction { result: Some(value), kind: zil_ir::InstructionKind::Load(parameter), span: at });
            changed = Some((block, index + 1)); break;
        }
    }
    let (block, instruction) = changed.unwrap();
    // Keep the hostile module's value arena and block shape identical to a
    // genuinely accepted control, so Wire cannot reject on a stale plan size.
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    raw.functions[function.0 as usize].blocks[block].instructions[instruction].kind =
        zil_ir::InstructionKind::MakeClosure {
            closure,
            captures: vec![value],
        };
    let proposal = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        proposal.iter().any(|d| d.message.contains("constant")),
        "{proposal:?}"
    );
    plan.module_fingerprint = raw.fingerprint();
    let rejected =
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        rejected.iter().any(|d| d.message.contains("constant")),
        "{rejected:?}"
    );
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
}

#[test]
fn mutable_callable_keeps_the_same_task_through_pending_ready_and_cleanup() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/mutable-task-fnmut.zil"
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
fn static_callable_task_input_keeps_the_complete_constant_capture_contract() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/static-callable-input.zil"
    );
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    if report.status != ResultCategory::Success {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/runs");
        fs::write(
            dir.join("static-task-typed.txt"),
            format!("{:#?}", typed_original(text)),
        )
        .unwrap();
    }
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let immutable = text.replace("F:Fn()", "F:FnMut()");
    let rejected = compile_source(
        &source(&immutable),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_ne!(
        rejected.status,
        ResultCategory::Success,
        "{}",
        rejected.to_json()
    );
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|d| d.message.contains("mutable binding")),
        "{}",
        rejected.to_json()
    );
    let mutable = immutable.replace(
        " task_yield();return i32(callback());",
        " var active=callback;task_yield();return i32(active());",
    );
    let accepted = compile_source(
        &source(&mutable),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        accepted.status,
        ResultCategory::Success,
        "{}",
        accepted.to_json()
    );
    let duplicate = mutable.replace(
        "var active=callback;",
        "var active=callback;var duplicate=callback;",
    );
    let rejected = compile_source(
        &source(&duplicate),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_ne!(
        rejected.status,
        ResultCategory::Success,
        "{}",
        rejected.to_json()
    );
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|d| d.message.contains("after move")),
        "{}",
        rejected.to_json()
    );
    for text in [
        mutable_static_source(text),
        owning_mutable_static_source(text),
        owning_static_source(text),
    ] {
        let report = compile_source(
            &source(&text),
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
    }
}

#[test]
fn moved_mutable_task_callback_keeps_actual_role_when_types_are_identical_raw_and_wire() {
    let original = include_str!(
        "../../../tests/language-readiness/task-owner-transport/static-callable-input.zil"
    );
    let text = mutable_static_source(original)
        .replace(
            "let text=\"abc\";var count=0;",
            "let text=\"abc\";let quiet=Quiet{value:1u8};var count=0;",
        )
        .replace(
            "return text[0]+u8(count)-1u8;",
            "return text[0]+u8(count)+quiet.value-2u8;",
        )
        .replace("(callback:F)", "(other:F,callback:F)")
        .replace(
            "task_create(worker,make())",
            "task_create(worker,make(),make())",
        );
    let text = format!(
        "resource Quiet{{value:u8}}\nimpl Drop for Quiet{{fn drop(self:Self){{}}}}\n{text}"
    );
    let mut raw = typed_original(&text);
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let worker_id = raw
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            zil_ir::InstructionKind::TaskCreate { worker, .. } => Some(worker),
            _ => None,
        })
        .unwrap();
    let worker = &mut raw.functions[worker_id.0 as usize];
    let active = worker
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let zil_ir::InstructionKind::CallClosureMut { callee, .. } = i.kind {
                Some(callee)
            } else {
                None
            }
        })
        .unwrap();
    let worker_id = worker.id;
    let mut immutable = raw.clone();
    immutable.functions[worker_id.0 as usize].locals[active.0 as usize].mutable = false;
    assert!(zil_analyze::analyze(&immutable, &limits).is_err());
    let mut immutable_plan = plan.clone();
    immutable_plan.module_fingerprint = immutable.fingerprint();
    assert!(
        zil_verify::verify_module(immutable.clone(), immutable_plan.clone(), &target, &limits)
            .is_err()
    );
    let wire = zil_ir::wire::encode_module_candidate(&immutable, &immutable_plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
    let worker = &mut raw.functions[worker_id.0 as usize];
    assert_eq!(worker.parameters.len(), 2);
    let first = worker.parameters[0];
    let second = worker.parameters[1];
    assert_eq!(
        worker.locals[first.0 as usize].ty,
        worker.locals[second.0 as usize].ty
    );
    let mut moved = 0;
    let mut parameter_cleanups = [0usize; 2];
    let mut parameter_ends = [0usize; 2];
    // Exchange the complete actual uses, preserving affine consumption and
    // both identical types. Only the claimed role identity is now false.
    for instruction in worker.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        let local = match &mut instruction.kind {
            zil_ir::InstructionKind::TakeLocal { local } => {
                moved += usize::from(*local == first || *local == second);
                local
            }
            zil_ir::InstructionKind::DropClosure { local } => {
                if *local == first {
                    parameter_cleanups[0] += 1;
                } else if *local == second {
                    parameter_cleanups[1] += 1;
                }
                local
            }
            zil_ir::InstructionKind::StorageDead(local) => {
                if *local == first {
                    parameter_ends[0] += 1;
                } else if *local == second {
                    parameter_ends[1] += 1;
                }
                local
            }
            _ => continue,
        };
        if *local == first {
            *local = second;
        } else if *local == second {
            *local = first;
        }
    }
    assert_eq!(moved, 1);
    // Both parameter slots retain a guarded scope cleanup instruction. The
    // moved slot's live flag is already clear; only the other slot still owns
    // a callable. Exchange both sites, including the inactive one.
    assert_eq!(parameter_cleanups, [1, 1]);
    assert_eq!(parameter_ends, [1, 1]);
    let rejected = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        rejected.iter().any(|d| d.message.contains("public helper")
            && (d.message.contains("flow differs") || d.message.contains("trace claim differs"))),
        "{rejected:?}"
    );
    plan.module_fingerprint = raw.fingerprint();
    let rejected =
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        rejected.iter().any(|d| d.message.contains("traces differ")),
        "{rejected:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    let rejected = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        rejected.iter().any(|d| d.message.contains("traces differ")),
        "{rejected:?}"
    );
}
fn cancel_drive(text: &str) -> String {
    let begin = text.find("fn drive<life a>").unwrap();
    let end = text.find("fn keep<life a>").unwrap();
    format!(
        "{}{}{}",
        &text[..begin],
        r#"fn drive<life a>(give original:Task<i32,&'a mut i32>)->i32 {
 var task=give original;task_request_cancel(&mut task);
 loop {match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 task_finish_destroy(give task);return 0;
}
"#,
        &text[end..]
    )
}
fn cases() -> Vec<(&'static str, String)> {
    let token=SOURCE.replace("fn worker(input:&mut i32)","extern \"C\" fn putchar(code:i32)->i32;\nresource Token{code:i32}\nimpl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}\nfn worker(input:&mut i32,give token:Token)")
        .replace("return *input;","return *input+token.code-65;")
        .replace("let original=task_create(worker,&mut number)?;","let input=Token{code:65};var original=task_create(worker,&mut number,give input)?;");
    let pending = token.replace(
        " let callback=",
        " task_poll(&mut original);\n let callback=",
    );
    let cancel0 = cancel_drive(&token)
        .replace("if number!=42", "if number!=37")
        .replace("return answer-42", "return answer");
    let pending_cancel = cancel_drive(&pending).replace("return answer-42", "return answer");
    let ready_cancel = pending_cancel.replace(
        " task_poll(&mut original);",
        " task_poll(&mut original);task_poll(&mut original);",
    );
    vec![
        ("ready", token),
        ("prepolled-pending", pending),
        ("cancel-before-poll", cancel0),
        ("cancel-after-pending", pending_cancel),
        ("ready-result-unobserved-by-owner", ready_cancel),
    ]
}
fn closed_owner() -> String {
    include_str!("../../../tests/language-readiness/task-owner-transport/closed-task-fnonce.zil")
        .to_owned()
}
fn returned_frame_cases() -> Vec<(&'static str, String)> {
    cases()
        .into_iter()
        .map(|(name, text)| {
            let name = match name {
                "ready" => "returned-frame-ready",
                "prepolled-pending" => "returned-frame-prepolled-pending",
                "cancel-before-poll" => "returned-frame-cancel-before-poll",
                "cancel-after-pending" => "returned-frame-cancel-after-pending",
                "ready-result-unobserved-by-owner" => "returned-frame-ready-unobserved",
                _ => unreachable!(),
            };
            let text = text
                .replace(
                    "->impl FnOnce()->i32",
                    "->impl FnOnce()->Managed<Task<i32,&'a mut i32>>",
                )
                .replace(
                    "return move()=>drive(give original);",
                    "return move()=>give original;",
                )
                .replace(
                    "let answer=callback();",
                    "let restored=callback();let answer=drive(give restored);",
                );
            (name, text)
        })
        .collect()
}

fn mutable_frame_cases() -> Vec<(&'static str, String)> {
    let text = include_str!("../../../tests/language-readiness/task-owner-transport/mutable-task-fnmut.zil")
        .replace("fn worker(input:&mut i32)", "extern \"C\" fn putchar(code:i32)->i32;\nresource Token{code:i32}\nimpl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}\nfn worker(input:&mut i32,give token:Token)")
        .replace("return *input;", "return *input+token.code-65;")
        .replace("let original=task_create(worker,&mut number)?;", "let input=Token{code:65};var original=task_create(worker,&mut number,give input)?;");
    let driver = "var callback=keep(give original);if callback()!=-1{return 91;}return callback();";
    let pending = text.replace(
        "let answer=drive_callback",
        "task_poll(&mut original);let answer=drive_callback",
    );
    let ready = pending.replace(driver, "var callback=keep(give original);let answer=callback();if answer!=42{return 93;}if callback()!=-2{return 94;}return answer;");
    let unobserved = text
        .replace(
            driver,
            "var callback=keep(give original);callback();callback();return 0;",
        )
        .replace("return answer-42;", "return answer;");
    let uncalled = text
        .replace(driver, "var callback=keep(give original);return 0;")
        .replace("if number!=42", "if number!=37")
        .replace("return answer-42;", "return answer;");
    let begin = text.find(" return move()->i32=>{").unwrap();
    let end = text.find("fn drive_callback").unwrap();
    let canceled = format!(
        "{}{}{}",
        &text[..begin],
        r#"
 return move()->i32=>{
  task_request_cancel(&mut task);
  match task_poll_close(&mut task){Poll.Pending()=>{return -1;},Poll.Ready(_)=>{return 0;}}
 };
}


"#,
        &text[end..]
    )
    .replace(
        driver,
        "var callback=keep(give original);if callback()!=0{return 95;}return callback();",
    )
    .replace("return answer-42;", "return answer;");
    let canceled_pending = canceled.replace(
        "let answer=drive_callback",
        "task_poll(&mut original);let answer=drive_callback",
    );
    vec![
        ("mutable-frame-ready", text),
        ("mutable-frame-prepolled", ready),
        ("mutable-frame-ready-unobserved", unobserved),
        ("mutable-frame-never-called", uncalled),
        (
            "mutable-frame-cancel-before-poll",
            canceled.replace("if number!=42", "if number!=37"),
        ),
        ("mutable-frame-cancel-after-pending", canceled_pending),
    ]
}
fn mutable_static_source(text: &str) -> String {
    text.replace("F:Fn()", "F:FnMut()")
        .replace("fn make()->impl Fn()->u8{let text=\"abc\";return move()=>text[0];}",
            "fn make()->impl FnMut()->u8{let text=\"abc\";var count=0;return move()=>{count+=1;return text[0]+u8(count)-1u8;};}")
        .replace(" task_yield();return i32(callback())",
            " var active=callback;task_yield();let first=active();task_yield();let second=active();return i32(first)+i32(second)-98")
}
fn owning_static_source(text: &str) -> String {
    text.replace("F:Fn()", "F:FnOnce()")
        .replace("fn make()->impl Fn()->u8{let text=\"abc\";return move()=>text[0];}",
            "extern \"C\" fn quiet_taken();\nextern \"C\" fn quiet_retired();\nresource Quiet{value:u8}\nimpl Drop for Quiet{fn drop(self:Self){unsafe{quiet_retired();}}}\nfn make()->impl FnOnce()->u8{let text=\"abc\";let quiet=Quiet{value:1u8};return move()=>{let taken=give quiet;unsafe{quiet_taken();}return text[0]+taken.value-1u8;};}")
}

fn owning_mutable_static_source(text: &str) -> String {
    let text = mutable_static_source(text)
        .replace(
            "let text=\"abc\";var count=0;",
            "let text=\"abc\";let quiet=Quiet{value:1u8};var count=0;",
        )
        .replace(
            "count+=1;return text[0]+u8(count)-1u8;",
            "count+=1;unsafe{quiet_taken();}return text[0]+u8(count)+quiet.value-2u8;",
        );
    format!(
        "extern \"C\" fn quiet_taken();\nextern \"C\" fn quiet_retired();\nresource Quiet{{value:u8}}\nimpl Drop for Quiet{{fn drop(self:Self){{unsafe{{quiet_retired();}}}}}}\n{text}"
    )
}

fn static_frame_cases() -> Vec<(&'static str, String)> {
    let text = include_str!("../../../tests/language-readiness/task-owner-transport/static-callable-input.zil")
        .replace("fn make()", "extern \"C\" fn putchar(code:i32)->i32;\nresource Token{code:i32}\nimpl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}\nfn make()")
        .replace("(callback:F)", "(callback:F,give token:Token)")
        .replace("return i32(callback());", "return i32(callback())+token.code-65;")
        .replace("var task=task_create(worker,make())?;", "let token=Token{code:65};var task=task_create(worker,make(),give token)?;");
    let start = text.find(" loop{match task_poll(&mut task)").unwrap();
    let end = text.find(" task_request_cancel(&mut task)").unwrap();
    let cancel =
        format!("{}{}", &text[..start], &text[end..]).replace("return answer-97", "return answer");
    vec![
        ("static-frame-fn", text.clone()),
        ("static-frame-fnmut", mutable_static_source(&text)),
        (
            "static-frame-owning-fnmut",
            owning_mutable_static_source(&text),
        ),
        (
            "static-frame-owning-fnmut-cancel-before-poll",
            owning_mutable_static_source(&cancel),
        ),
        (
            "static-frame-owning-fnmut-cancel-after-pending",
            owning_mutable_static_source(&cancel.replace(
                " task_request_cancel(&mut task)",
                " task_poll(&mut task);task_request_cancel(&mut task)",
            )),
        ),
        (
            "static-frame-owning-fnmut-cancel-after-second-pending",
            owning_mutable_static_source(&cancel.replace(
                " task_request_cancel(&mut task)",
                " task_poll(&mut task);task_poll(&mut task);task_request_cancel(&mut task)",
            )),
        ),
        ("static-frame-fnonce", text.replace("F:Fn()", "F:FnOnce()")),
        ("static-frame-owning-fnonce", owning_static_source(&text)),
        (
            "static-frame-owning-fnonce-cancel-before-poll",
            owning_static_source(&cancel),
        ),
        (
            "static-frame-owning-fnonce-cancel-after-pending",
            owning_static_source(&cancel.replace(
                " task_request_cancel(&mut task)",
                " task_poll(&mut task);task_request_cancel(&mut task)",
            )),
        ),
        ("static-frame-cancel-before-poll", cancel.clone()),
        (
            "static-frame-cancel-after-pending",
            cancel.replace(
                " task_request_cancel(&mut task)",
                " task_poll(&mut task);task_request_cancel(&mut task)",
            ),
        ),
    ]
}

#[test]
fn consuming_callable_returns_the_actual_task_frame_and_retained_context() {
    for (name, text) in [
        (
            "one",
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/returned-task-result-fnonce.zil"
            ),
        ),
        (
            "second",
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/returned-second-task-fnonce.zil"
            ),
        ),
    ] {
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
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn returned_frame_keeps_actual_capture_identity_when_lifetime_classes_are_equal_raw_and_wire() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/returned-second-task-fnonce.zil"
    )
    .replace("keep<life a,life b>", "keep<life a>")
    .replace("&'b mut i32", "&'a mut i32");
    let input = source(&text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let d = &typed.closures[0];
    assert_eq!(d.captures[0], d.captures[1]);
    assert_eq!(d.storage_life.as_ref().unwrap().fixed_binder_count, 1);
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let creator = typed
        .functions
        .iter()
        .position(|f| f.callable_return_contract.is_some())
        .unwrap();
    let mut forged = typed.clone();
    let rows = forged.functions[creator]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .capture_sources
        .as_mut()
        .unwrap();
    assert_eq!(rows.len(), 2);
    rows[1].sources = rows[0].sources.clone();
    assert!(zil_analyze::analyze(&forged, &limits).is_err());
    let mut claim = plan.clone();
    claim.module_fingerprint = forged.fingerprint();
    assert!(zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());

    let entry = &typed.functions[d.function.0 as usize];
    let returned = entry
        .blocks
        .iter()
        .find_map(|block| match block.terminator {
            zil_ir::Terminator::Return(Some(value)) => Some(value),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        plan.functions[d.function.0 as usize].holds[returned.0 as usize].roots,
        [zil_ir::HoldRoot::Capture(1)]
    );
    let mut claim = plan;
    claim.functions[d.function.0 as usize].holds[returned.0 as usize].roots =
        vec![zil_ir::HoldRoot::Capture(0)];
    assert!(zil_verify::verify_module(typed.clone(), claim.clone(), &target, &limits).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
}
#[test]
fn returned_fnonce_moves_the_actual_live_task_and_retains_its_parent() {
    for (name, text) in [
        ("basic", SOURCE.to_owned()),
        ("closed-owner", closed_owner()),
    ]
    .into_iter()
    .chain(cases())
    .chain(returned_frame_cases())
    {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
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
    }
}
#[test]
fn retained_task_parent_and_once_responsibility_survive_the_factory_call() {
    for (name, text) in [
        (
            "write-before-call",
            SOURCE.replace("let answer=callback();", "number=0;let answer=callback();"),
        ),
        (
            "twice",
            SOURCE.replace(
                "let answer=callback();",
                "let first=callback();let answer=callback();",
            ),
        ),
    ] {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
    }
    let original = include_str!(
        "../../../tests/language-readiness/task-owner-transport/returned-task-result-fnonce.zil"
    );
    for text in [
        original.replace(
            "let answer=drive(give restored);",
            "number=0;let answer=drive(give restored);",
        ),
        original.replace(
            "let restored=callback();",
            "let restored=callback();let duplicate=callback();",
        ),
    ] {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
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
}
#[test]
fn retained_task_fnonce_keeps_original_pc_inputs_and_one_cleanup_native() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text, expected, frames) in cases().into_iter().chain(returned_frame_cases()).chain(mutable_frame_cases()).chain(static_frame_cases())
                .map(|(name, text)| (name, text, &b"A"[..],1))
                .chain(std::iter::once(("closed-owner", closed_owner(), &b""[..],1)))
                .chain(std::iter::once(("returned-second-frame",include_str!("../../../tests/language-readiness/task-owner-transport/returned-second-task-fnonce.zil").to_owned(),&b""[..],2)))
            {
                let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../../out/runs/task-owner-native/{name}-{backend:?}-{optimization:?}"
                ));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("source.zil"), &text).unwrap();
                let clang = std::env::var_os("ZIL_CLANG")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "clang".into());
                let object = dir.join("observer.o");
                let observer = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/language-readiness/task-aggregate-results/frame-observer.c");
                let built = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(format!("-DZIL_TASK_EXPECTED_FRAMES={frames}"))
                    .arg(format!("-DZIL_TASK_EXPECTED_QUIET={}", usize::from(name.starts_with("static-frame-owning-"))))
                    .arg(format!("-DZIL_TASK_EXPECTED_QUIET_CALLS={}", match name {
                        "static-frame-owning-fnmut" => 2,
                        "static-frame-owning-fnmut-cancel-after-second-pending" | "static-frame-owning-fnonce" => 1,
                        _ => 0,
                    }))
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
                fs::write(dir.join("report.json"), report.to_json()).unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}: {}",
                    report.to_json()
                );
                let generated = report.native_codegen.as_ref().unwrap();
                assert_eq!(generated.backend, backend);
                assert!(generated.fallback.is_none());
                let output = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(output.status.code(), Some(0), "{name}: {output:?}");
                assert_eq!(output.stdout, expected, "{name}: {output:?}");
                let failed = Command::new(&report.artifacts["executable"])
                    .env("ZIL_TASK_AGGREGATE_FAIL", "1")
                    .output()
                    .unwrap();
                assert_eq!(
                    failed.status.code(),
                    Some(1),
                    "allocation {name}: {failed:?}"
                );
                assert_eq!(failed.stdout, expected, "allocation {name}: {failed:?}");
                if frames>1 {
                    let failed=Command::new(&report.artifacts["executable"])
                        .env("ZIL_TASK_AGGREGATE_FAIL_AT","2").output().unwrap();
                    assert_eq!(failed.status.code(),Some(1),"second allocation {name}: {failed:?}");
                    assert_eq!(failed.stdout,expected,"second allocation {name}: {failed:?}");
                }
            }
        }
    }
}

#[test]
fn asynchronous_child_cleanup_yields_and_returns_original_inputs_on_allocation_failure_native() {
    let text = include_str!("../../../tests/language-readiness/task-owner-transport/child-frame-cleanup.zil")
        .replace("let child=make_safe_child()", "let child=make_async_child()")
        .replace(".ok(original)=>{return 0;}", ".ok(original)=>{return close(give original);}")
        .replace("let callback=move(value:i32)->i32=>value;", "let callback=move(value:i32)->i32=>{unsafe{putchar(88);}return value;};")
        .replace("var remaining=give owner;task_yield();", "unsafe{child_enter();}var remaining=give owner;task_yield();unsafe{child_after_yield();}")
        .replace("fn parent(give child:Task<i32>)->i32 {task_yield();", "fn parent(give child:Task<i32>)->i32 {unsafe{parent_enter();}task_yield();")
        .replace("loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}", "loop{match task_poll_close(&mut task){Poll.Pending()=>{unsafe{driver_pending();}continue;},Poll.Ready(_)=>{unsafe{driver_ready();}break;}}}");
    let declarations = "extern \"C\" fn putchar(code:i32)->i32;\nextern \"C\" fn driver_pending();\nextern \"C\" fn driver_ready();\nextern \"C\" fn child_enter();\nextern \"C\" fn child_after_yield();\nextern \"C\" fn parent_enter();\nextern \"C\" fn child_drop_pending();\n";
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for prepolled in [false, true] {
                for middle_drop in [false, true] {
                    let text = if prepolled {
                        text.replace(
                            "let child=make_async_child()?;",
                            "var child=make_async_child()?;task_poll(&mut child);",
                        )
                    } else {
                        text.clone()
                    };
                    let text = if middle_drop {
                        text.replace("return close(give child);", "return 0;")
                        .replace(".ok(original)=>{return close(give original);}", ".ok(original)=>{var held=give original;task_poll(&mut held);task_poll(&mut held);unsafe{child_drop_pending();}return close(give held);}")
                    } else {
                        text
                    };
                    let text = format!("{declarations}{text}");
                    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../out/runs/task-child-native/{backend:?}-{optimization:?}-{prepolled}-{middle_drop}"));
                    fs::create_dir_all(&dir).unwrap();
                    fs::write(dir.join("source.zil"), &text).unwrap();
                    let clang = std::env::var_os("ZIL_CLANG")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| "clang".into());
                    let observer = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/language-readiness/task-owner-transport/child-frame-observer.c");
                    let object = dir.join("observer.o");
                    let built = Command::new(&clang)
                        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                        .arg(format!("-DZIL_CHILD_PREPOLLED={}", usize::from(prepolled)))
                        .arg(format!("-DZIL_CHILD_MID_DROP={}", usize::from(middle_drop)))
                        .arg(observer)
                        .arg("-o")
                        .arg(&object)
                        .output()
                        .unwrap();
                    assert!(built.status.success(), "{built:?}");
                    let quote = |path: &Path| {
                        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
                    };
                    let wrapper = dir.join("link.sh");
                    fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n", quote(&clang), quote(&object), quote(&clang))).unwrap();
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
                    fs::write(dir.join("report.json"), report.to_json()).unwrap();
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    let generated = report.native_codegen.as_ref().unwrap();
                    assert_eq!(generated.backend, backend);
                    assert!(generated.fallback.is_none());
                    for failure in 0..=3 {
                        let output = Command::new(&report.artifacts["executable"])
                            .env("ZIL_CHILD_FAIL_AT", failure.to_string())
                            .output()
                            .unwrap();
                        assert_eq!(
                            output.status.code(),
                            Some(if failure == 2 { 1 } else { 0 }),
                            "failure {failure}, prepolled={prepolled}: {output:?}"
                        );
                        assert!(
                            output.stdout.is_empty(),
                            "late callback ran after Closing: {output:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn equal_task_types_and_classes_keep_actual_source_identity_raw_and_wire() {
    let text=SOURCE.replace("fn keep<life a>(give original:Task<i32,&'a mut i32>)", "fn keep<life a>(give original:Task<i32,&'a mut i32>,give second:Task<i32,&'a mut i32>)")
        .replace("return move()=>drive(give original);", "return move()=>drive(give original)+drive(give second);")
        .replace("var number=37;", "var number=37;var other=41;")
        .replace("let callback=keep(give original);", "let otherTask=task_create(worker,&mut other)?;let callback=keep(give original,give otherTask);")
        .replace("return answer-42;", "if other!=46{return 92;}return answer-88;");
    let input = source(&text);
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    assert_eq!(typed.closures[0].captures[0], typed.closures[0].captures[1]);
    assert_eq!(
        typed.closures[0]
            .storage_life
            .as_ref()
            .unwrap()
            .fixed_binder_count,
        1
    );
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let creator = typed
        .functions
        .iter()
        .position(|f| f.callable_return_contract.is_some())
        .unwrap();
    for mutation in 0..4 {
        let mut raw = typed.clone();
        if mutation == 3 {
            let captures = raw.functions[creator]
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find_map(|i| {
                    if let zil_ir::InstructionKind::MakeClosure { captures, .. } = &mut i.kind {
                        Some(captures)
                    } else {
                        None
                    }
                })
                .unwrap();
            captures.swap(0, 1);
        } else {
            let rows = raw.functions[creator]
                .callable_return_contract
                .as_mut()
                .unwrap()
                .capture_sources
                .as_mut()
                .unwrap();
            match mutation {
                0 => rows[0].sources = rows[1].sources.clone(),
                1 => {
                    rows.pop();
                }
                _ => {
                    rows[0].position.path = vec![
                        zil_ir::ContractProjection::Capture(1),
                        zil_ir::ContractProjection::RetainedPort(0),
                    ]
                }
            }
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze mutation {mutation}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        assert!(
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err(),
            "Verify mutation {mutation}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire mutation {mutation}"
        );
    }
}

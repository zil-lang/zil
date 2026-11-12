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
            for (name, text, expected, frames) in cases().into_iter().chain(returned_frame_cases())
                .into_iter()
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

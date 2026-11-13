use super::*;

#[test]
fn nested_callable_pending_cleanup_keeps_the_root_and_every_live_leaf_native() {
    for (name, original) in [
        ("direct-capture", include_str!("../../../../tests/language-readiness/task-owner-transport/nested-callable-child-cleanup.zil")),
        ("nested-once-capture", include_str!("../../../../tests/language-readiness/task-owner-transport/nested-once-callable-child-cleanup.zil")),
    ] {
    let declarations="extern \"C\" fn putchar(code:i32)->i32;\nextern \"C\" fn driver_pending();\nextern \"C\" fn driver_ready();\nextern \"C\" fn child_enter();\nextern \"C\" fn child_after_yield();\nextern \"C\" fn parent_enter();\nextern \"C\" fn child_drop_pending();\n";
    let instrumented=original
        .replace("let callback=move(value:i32)->i32=>value;", "let callback=move(value:i32)->i32=>{unsafe{putchar(88);}return value;};")
        .replace("var remaining=give owner;task_yield();", "unsafe{child_enter();}var remaining=give owner;task_yield();unsafe{child_after_yield();}")
        .replace(" task_yield();return 0;\n}", " unsafe{parent_enter();}task_yield();return 0;\n}");
    // The constructor-failure F closes its capture in its actual body. Only
    // the outer Task driver reports Pending/Ready to the C observer.
    let close=instrumented.find("fn close(").unwrap();
    let keep=instrumented.find("extern \"C\" fn quiet_retired").unwrap();
    let outer=instrumented[close..keep].replace(
        "loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}",
        "loop{match task_poll_close(&mut task){Poll.Pending()=>{unsafe{driver_pending();}continue;},Poll.Ready(_)=>{unsafe{driver_ready();}break;}}}");
    let text=format!("{}{outer}{}",&instrumented[..close],&instrumented[keep..]);
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend==NativeBackend::LlvmApi && !cfg!(feature="llvm-api") {continue;}
        for optimization in [Optimization::O0,Optimization::O2] {
            for prepolled in [false,true] {
                for middle in [false,true] {
                    let text=if prepolled {text.replace("let child=make_async_child()?;","var child=make_async_child()?;task_poll(&mut child);")} else {text.clone()};
                    let text=if middle {text.replace(".ok(original)=>{return close(give original);}",".ok(original)=>{var held=give original;task_poll(&mut held);task_poll(&mut held);unsafe{child_drop_pending();}return close(give held);}")} else {text};
                    let text=format!("{declarations}{text}");
                    let dir=Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../out/runs/task-nested-native/{name}-{backend:?}-{optimization:?}-{prepolled}-{middle}"));
                    fs::create_dir_all(&dir).unwrap();fs::write(dir.join("source.zil"),&text).unwrap();
                    let clang=std::env::var_os("ZIL_CLANG").map(PathBuf::from).unwrap_or_else(||"clang".into());
                    let observer=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/language-readiness/task-owner-transport/nested-frame-observer.c");
                    let object=dir.join("observer.o");
                    let built=Command::new(&clang).args(["-std=c11","-Wall","-Wextra","-Werror","-c"])
                        .arg(format!("-DZIL_CHILD_PREPOLLED={}",usize::from(prepolled)))
                        .arg(format!("-DZIL_CHILD_MID_DROP={}",usize::from(middle)))
                        .arg(observer).arg("-o").arg(&object).output().unwrap();
                    assert!(built.status.success(),"{built:?}");
                    let report=compile_source(&source(&text),&CompileOptions{
                        stop_after:Stage::Build,backend,optimization,
                        clang:Some(observer_linker(&dir,&clang,&object)),output:Some(dir.join("program")),
                        ..CompileOptions::default()
                    });
                    fs::write(dir.join("report.json"),report.to_json()).unwrap();
                    assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
                    let generated=report.native_codegen.as_ref().unwrap();
                    assert_eq!(generated.backend,backend);assert!(generated.fallback.is_none());
                    for failure in 0..=3 {
                        let output=Command::new(&report.artifacts["executable"]).env("ZIL_CHILD_FAIL_AT",failure.to_string()).output().unwrap();
                        fs::write(dir.join(format!("failure-{failure}.txt")),format!("{output:?}")).unwrap();
                        assert_eq!(output.status.code(),Some(i32::from(failure==2||failure==3)),"failure {failure}: {output:?}");
                        assert!(output.stdout.is_empty(),"{output:?}");
                    }
                }
            }
        }
    }
}
}

#[test]
fn consuming_callable_returns_the_original_async_child_before_environment_cleanup_native() {
    let original = include_str!("../../../../tests/language-readiness/task-owner-transport/returned-async-task-fnonce.zil");
    let declarations = "extern \"C\" fn putchar(code:i32)->i32;\nextern \"C\" fn driver_pending();\nextern \"C\" fn driver_ready();\nextern \"C\" fn child_enter();\nextern \"C\" fn child_after_yield();\nextern \"C\" fn frame_handoff();\n";
    let instrumented = original
        .replace("let callback=move(value:i32)->i32=>value;", "let callback=move(value:i32)->i32=>{unsafe{putchar(88);}return value;};")
        .replace("var remaining=give owner;task_yield();", "unsafe{child_enter();}var remaining=give owner;task_yield();unsafe{child_after_yield();}")
        .replace("loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}",
            "loop{match task_poll_close(&mut task){Poll.Pending()=>{unsafe{driver_pending();}continue;},Poll.Ready(_)=>{unsafe{driver_ready();}break;}}}")
        .replace("let callback=keep(give original);let restored=callback();", "unsafe{frame_handoff();}let callback=keep(give original);let restored=callback();unsafe{frame_handoff();}");
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") { continue; }
        for optimization in [Optimization::O0, Optimization::O2] {
            for prepolled in [false, true] {
                let text = if prepolled {
                    instrumented.replace("let original=make_async_child()?;", "var original=make_async_child()?;task_poll(&mut original);")
                } else { instrumented.clone() };
                let text = format!("{declarations}{text}");
                let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../../out/runs/task-returned-async-native/{backend:?}-{optimization:?}-{prepolled}"));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("source.zil"), &text).unwrap();
                let clang = std::env::var_os("ZIL_CLANG").map(PathBuf::from).unwrap_or_else(|| "clang".into());
                let object = dir.join("observer.o");
                let observer = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../../tests/language-readiness/task-owner-transport/returned-async-frame-observer.c");
                let built = Command::new(&clang).args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(format!("-DZIL_CHILD_PREPOLLED={}", usize::from(prepolled)))
                    .arg(observer).arg("-o").arg(&object).output().unwrap();
                assert!(built.status.success(), "{built:?}");
                let report = compile_source(&source(&text), &CompileOptions {
                    stop_after: Stage::Build, backend, optimization,
                    clang: Some(observer_linker(&dir, &clang, &object)), output: Some(dir.join("program")),
                    ..CompileOptions::default()
                });
                fs::write(dir.join("report.json"), report.to_json()).unwrap();
                assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
                let generated = report.native_codegen.as_ref().unwrap();
                assert_eq!(generated.backend, backend);
                assert!(generated.fallback.is_none());
                for failure in 0..=2 {
                    let output = Command::new(&report.artifacts["executable"])
                        .env("ZIL_CHILD_FAIL_AT", failure.to_string()).output().unwrap();
                    fs::write(dir.join(format!("failure-{failure}.txt")), format!("{output:?}")).unwrap();
                    assert_eq!(output.status.code(), Some(i32::from(failure == 2)), "failure {failure}: {output:?}");
                    assert!(output.stdout.is_empty(), "{output:?}");
                }
            }
        }
    }
}

#[test]
fn active_async_child_returned_by_once_keeps_its_real_frame_raw_and_wire() {
    let text = include_str!("../../../../tests/language-readiness/task-owner-transport/returned-async-task-fnonce.zil");
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let typed = typed_original(text);
    let claim = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), claim.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &claim, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let definition = typed.closures.iter().find(|definition| {
        definition.kind == zil_ir::CallableKind::FnOnce
            && matches!(definition.result, zil_ir::Type::Managed(ref inner) if matches!(inner.as_ref(), zil_ir::Type::Task{..}))
    }).unwrap();
    let entry = &typed.functions[definition.function.0 as usize];
    assert!(entry.blocks.iter().flat_map(|block| &block.instructions)
        .any(|instruction| matches!(instruction.kind, zil_ir::InstructionKind::TakeCapture{..})));
    assert!(!entry.blocks.iter().flat_map(|block| &block.instructions)
        .any(|instruction| matches!(instruction.kind,
            zil_ir::InstructionKind::TaskCreate{..} | zil_ir::InstructionKind::TaskCreateOwned{..}
            | zil_ir::InstructionKind::TaskPoll{..} | zil_ir::InstructionKind::TaskPollClose{..})));
}

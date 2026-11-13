use super::*;

fn inputs() -> [(u32, &'static str, &'static str); 3] {
    [
        (0, "scalar", include_str!("../../../../tests/language-readiness/task-owner-transport/once-pending-callable-cleanup.zil")),
        (1, "packet", include_str!("../../../../tests/language-readiness/task-owner-transport/once-pending-owned-result-cleanup.zil")),
        (2, "task", include_str!("../../../../tests/language-readiness/task-owner-transport/once-pending-returned-task-cleanup.zil")),
    ]
}

#[test]
fn pending_once_cleanup_keeps_its_environment_and_every_returned_owner_raw_and_wire() {
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    for (mode, name, text) in inputs() {
        let typed = typed_original(text);
        let claim = zil_analyze::analyze(&typed, &limits).unwrap();
        let checked = zil_verify::verify_module(typed.clone(), claim.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&typed, &claim, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        let mut observed = false;
        for function in &typed.functions {
            let Some(frame) = checked.task_ownership(function.id) else { continue; };
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    let zil_ir::InstructionKind::CallClosureOnce { callee, .. } = instruction.kind else { continue; };
                    let actions = frame.actions_at(block.id, index).unwrap();
                    let owner = frame.owner_for_value(callee).unwrap();
                    assert_eq!(actions.invocation_owner(), Some(owner), "{name}");
                    assert!(!actions.before().iter().any(|action| matches!(action,
                        zil_verify::TaskOwnerAction::Unlink(slot) if *slot == owner)), "{name}");
                    assert!(actions.after().iter().any(|action| matches!(action,
                        zil_verify::TaskOwnerAction::Unlink(slot) if *slot == owner)), "{name}");
                    let boundary = frame.boundaries().iter().find(|boundary|
                        boundary.block() == block.id && boundary.instruction() == index).unwrap();
                    assert!(boundary.may_live().contains(&owner), "{name}");
                    if mode != 0 {
                        let result = frame.owner_for_value(instruction.result.unwrap()).unwrap();
                        assert!(actions.after_invoke().iter().any(|action| matches!(action,
                            zil_verify::TaskOwnerAction::Link(slot) if *slot == result)), "{name}");
                        assert!(boundary.may_live().contains(&result), "{name}");
                    }
                    observed = true;
                }
            }
        }
        assert!(observed, "{name}");
        // A Direct body cannot deliver a result and quietly discard Pending
        // while reclaiming the still-live environment behind that result.
        let direct = text.replace(" task_yield();let answer=callback();", " let answer=callback();");
        assert_ne!(direct, text);
        let rejected = compile_source(&source(&direct), &CompileOptions {
            stop_after: Stage::Verify, ..CompileOptions::default()
        });
        assert_ne!(rejected.status, ResultCategory::Success, "{}", rejected.to_json());
        assert!(rejected.diagnostics.iter().any(|d| d.code == "E-TASK-ABI" || d.code == "E-VERIFY-TASK"),
            "{}", rejected.to_json());
    }
}

#[test]
fn pending_once_cleanup_preserves_actual_spill_flags_returned_owners_and_retry_native() {
    let declarations = "extern \"C\" fn putchar(code:i32)->i32;\nextern \"C\" fn driver_pending();\nextern \"C\" fn driver_ready();\nextern \"C\" fn child_enter();\nextern \"C\" fn child_after_yield();\nextern \"C\" fn parent_enter();\nextern \"C\" fn postbody_pending();\nextern \"C\" fn safe_enter();\nextern \"C\" fn safe_after_yield();\n";
    for (mode, name, original) in inputs() {
        let instrumented = original
            .replace("let callback=move(value:i32)->i32=>value;", "let callback=move(value:i32)->i32=>{unsafe{putchar(88);}return value;};")
            .replace("var remaining=give owner;task_yield();", "unsafe{child_enter();}var remaining=give owner;task_yield();unsafe{child_after_yield();}")
            .replace("fn safe_worker()->i32 {task_yield();return 0;}", "fn safe_worker()->i32 {unsafe{safe_enter();}task_yield();unsafe{safe_after_yield();}return 0;}")
            .replace(" task_yield();let answer=callback();", " unsafe{parent_enter();}task_yield();let answer=callback();");
        let run = instrumented.find("fn run(").unwrap();
        let main = instrumented.find("fn main(").unwrap();
        let driven = instrumented[run..main]
            .replace("Poll.Pending()=>{continue;}", "Poll.Pending()=>{unsafe{driver_pending();}continue;}")
            .replace("Poll.Ready(value)=>{answer=value;break;}", "Poll.Ready(value)=>{unsafe{driver_ready();}answer=value;break;}");
        let instrumented = format!("{}{driven}{}", &instrumented[..run], &instrumented[main..]);
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") { continue; }
            for optimization in [Optimization::O0, Optimization::O2] {
                for prepolled in [false, true] {
                    for cancel in [false, true] {
                        let text = if prepolled {
                            instrumented.replace("let original=make_async_child()?;", "var original=make_async_child()?;task_poll(&mut original);")
                                .replace("var selected=give input;", "var selected=give input;task_poll(&mut selected);")
                        } else { instrumented.clone() };
                        let text = if cancel {
                            text.replace("return run(give task);", "task_poll(&mut task);unsafe{postbody_pending();}return close(give task);")
                        } else { text };
                        let text = format!("{declarations}{text}");
                        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                            "../../out/runs/task-pending-once-native/{name}-{backend:?}-{optimization:?}-{prepolled}-{cancel}"));
                        fs::create_dir_all(&dir).unwrap();fs::write(dir.join("source.zil"), &text).unwrap();
                        let clang = std::env::var_os("ZIL_CLANG").map(PathBuf::from).unwrap_or_else(|| "clang".into());
                        let object = dir.join("observer.o");
                        let observer = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                            "../../tests/language-readiness/task-owner-transport/once-pending-frame-observer.c");
                        let built = Command::new(&clang).args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                            .arg(format!("-DZIL_ONCE_OWNED={mode}"))
                            .arg(format!("-DZIL_ONCE_CANCEL={}", usize::from(cancel)))
                            .arg(format!("-DZIL_CHILD_PREPOLLED={}", usize::from(prepolled)))
                            .arg(observer).arg("-o").arg(&object).output().unwrap();
                        assert!(built.status.success(), "{built:?}");
                        let report = compile_source(&source(&text), &CompileOptions {
                            stop_after: Stage::Build, backend, optimization,
                            emit_dir: Some(dir.clone()), clang: Some(observer_linker(&dir, &clang, &object)),
                            output: Some(dir.join("program")), ..CompileOptions::default()
                        });
                        fs::write(dir.join("report.json"), report.to_json()).unwrap();
                        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
                        let generated = report.native_codegen.as_ref().unwrap();
                        assert_eq!(generated.backend, backend);assert!(generated.fallback.is_none());
                        let llvm = fs::read_to_string(dir.join("phases/llvm.ll")).unwrap();
                        assert!(llvm.contains("zil.task.owner.retarget"), "{name}");
                        assert!(!llvm.contains("%closure.once."),
                            "a poll-stack environment escaped its activation: {llvm}");
                        for failure in 0..=if mode == 2 { 4 } else { 3 } {
                            let output = Command::new(&report.artifacts["executable"])
                                .env("ZIL_CHILD_FAIL_AT", failure.to_string()).output().unwrap();
                            fs::write(dir.join(format!("failure-{failure}.txt")), format!("{output:?}")).unwrap();
                            assert_eq!(output.status.code(), Some(i32::from(failure == 2 || mode == 2 && failure == 3)),
                                "failure {failure}: {output:?}");
                            assert!(output.stdout.is_empty(), "{output:?}");
                        }
                    }
                }
            }
        }
    }
}

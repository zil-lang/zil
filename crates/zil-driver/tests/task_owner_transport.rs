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
#[test]
fn returned_fnonce_moves_the_actual_live_task_and_retains_its_parent() {
    for (name, text) in std::iter::once(("basic", SOURCE.to_owned())).chain(cases()) {
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
}
#[test]
fn retained_task_fnonce_keeps_original_pc_inputs_and_one_cleanup_native() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text) in cases() {
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
                assert_eq!(output.stdout, b"A", "{name}: {output:?}");
                let failed = Command::new(&report.artifacts["executable"])
                    .env("ZIL_TASK_AGGREGATE_FAIL", "1")
                    .output()
                    .unwrap();
                assert_eq!(
                    failed.status.code(),
                    Some(1),
                    "allocation {name}: {failed:?}"
                );
                assert_eq!(failed.stdout, b"A", "allocation {name}: {failed:?}");
            }
        }
    }
}

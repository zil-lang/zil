use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};
const SOURCE: &str = r#"
fn worker(seed:i32,other:i32)->i32 {task_yield();return seed+other;}
fn main()->i32 {
    match task_create_owned(worker,37,5) {
        .err(failure)=>{let inputs=failure.inputs;return inputs.0+inputs.1-40;},
        .ok(input)=>{var task=give input;var answer=0;loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}task_request_cancel(&mut task);loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give task);return answer-42;}
    }
}
"#;
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "task-create-owned.zil", text)
}
#[test]
fn owning_task_creation_source_keeps_complete_ordered_failure_inputs() {
    let report = compile_source(
        &source(SOURCE),
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

const OWNERS: &str = r#"
extern "C" fn owned_input_drop(code:i32);
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{owned_input_drop(.code);}}}
fn worker(give first:Token,give second:Token,seed:i32)->i32 {task_yield();return first.code+second.code+seed;}
fn main()->i32 {
    let first=Token{code:37};let second=Token{code:43};
    match task_create_owned(worker,give first,give second,4) {
        .err(failure)=>{let (first,second,seed)=failure.inputs;if first.code!=37{return 91;}if second.code!=43{return 92;}if seed!=4{return 93;}return 2;},
        .ok(input)=>{var task=give input;var answer=0;loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}task_request_cancel(&mut task);loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give task);return answer-84;}
    }
}
"#;
#[test]
fn owning_task_creation_resource_source_returns_original_fields() {
    let report = compile_source(
        &source(OWNERS),
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
fn owning_task_creation_native_failure_returns_both_same_type_owners_once() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-create-owned-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let clang = std::env::var_os("ZIL_CLANG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "clang".into());
            let object = dir.join("observer.o");
            let observer = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "../../tests/language-readiness/task-thread-transfer/create-owned-observer.c",
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
                &source(OWNERS),
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
            for fail in [false, true] {
                let mut command = Command::new(&report.artifacts["executable"]);
                if fail {
                    command.env("ZIL_TASK_ALLOC_FAIL", "1");
                }
                let output = command.output().unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(if fail { 2 } else { 0 }),
                    "{backend:?}/{optimization:?}/{fail}: {output:?}"
                );
                assert_eq!(output.stdout, b"both original owners cleaned once\n");
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

const REGISTRATION: &str = r#"
extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
fn worker<F:FnMut(i32)->i32 + ClosedEnvironment>(give owner:AsyncRegistration<F>,seed:i32)->i32 contract{requires(state(owner,Open,Closing,Closed));provides(retired(owner));} {
    var remaining=give owner;task_yield();async_registration_request_close(&mut remaining);
    loop{match async_registration_poll_close(&mut remaining){Poll.Pending()=>{task_yield();continue;},Poll.Ready(_)=>{break;}}}
    async_registration_finish_destroy_checked(give remaining);return seed+5;
}
fn main()->i32 {
    var count=37;let callback=move(value:i32)->i32=>{count+=1;return count+value;};
    unsafe {
    let owner=c_register_async(reg,request,drain,callback);
    match task_create_owned(worker,give owner,37) {
        .err(failure)=>{let (input,seed)=failure.inputs;var remaining=give input;async_registration_request_close(&mut remaining);loop{match async_registration_poll_close(&mut remaining){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}async_registration_finish_destroy_checked(give remaining);return seed-35;},
        .ok(input)=>{var task=give input;var answer=0;loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}task_request_cancel(&mut task);loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give task);return answer-42;}
    }
    }
}
"#;
#[test]
fn live_same_thread_registration_input_keeps_an_explicit_failure_owner() {
    let report = compile_source(
        &source(REGISTRATION),
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
fn live_registration_native_allocation_failure_preserves_protocol_and_owner() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-create-owned-registration-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let clang = std::env::var_os("ZIL_CLANG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "clang".into());
            let object = dir.join("observer.o");
            let observer=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/language-readiness/task-thread-transfer/create-owned-registration-observer.c");
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
                &source(REGISTRATION),
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
            for fail in [false, true] {
                let mut command = Command::new(&report.artifacts["executable"]);
                if fail {
                    command.env("ZIL_TASK_ALLOC_FAIL", "1");
                }
                let output = command.output().unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(if fail { 2 } else { 0 }),
                    "{backend:?}/{optimization:?}/{fail}: {output:?}"
                );
                assert_eq!(
                    output.stdout,
                    b"original active input drained after allocation outcome; once free\n"
                );
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn owned_constructor_raw_and_wire_reject_duplicate_debt_and_false_input_phase() {
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    for (text, phase) in [(OWNERS, false), (REGISTRATION, true)] {
        let input = source(text);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        for wire in [false, true] {
            let mut forged = typed.clone();
            let worker_id = forged
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .flat_map(|b| &b.instructions)
                .find_map(|i| match i.kind {
                    zil_ir::InstructionKind::TaskCreateOwned { worker, .. } => Some(worker),
                    _ => None,
                })
                .unwrap();
            let worker = &mut forged.functions[worker_id.0 as usize];
            if phase {
                worker.protocol_contracts[0].entry = vec![zil_ir::ProtocolState::Closed];
            } else {
                let main = forged
                    .functions
                    .iter_mut()
                    .find(|f| {
                        f.blocks.iter().any(|b| {
                            b.instructions.iter().any(|i| {
                                matches!(i.kind, zil_ir::InstructionKind::TaskCreateOwned { .. })
                            })
                        })
                    })
                    .unwrap();
                let arguments = main
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| match &mut i.kind {
                        zil_ir::InstructionKind::TaskCreateOwned { arguments, .. } => {
                            Some(arguments)
                        }
                        _ => None,
                    })
                    .unwrap();
                arguments[1] = arguments[0];
            }
            assert!(
                zil_analyze::analyze(&forged, &limits).is_err(),
                "Analyze phase={phase}"
            );
            let mut claim = plan.clone();
            claim.module_fingerprint = forged.fingerprint();
            let result = if wire {
                let bytes =
                    zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(forged, claim, &target, &limits)
            };
            assert!(result.is_err(), "Verify phase={phase}/wire={wire}");
        }
    }
}

const BORROWED: &str = r#"
fn worker(input:&mut i32)->i32 {*input+=1;task_yield();return *input;}
fn main()->i32 {
    var number=41;
    match task_create_owned(worker,&mut number) {
        .err(failure)=>{let (input,)=failure.inputs;*input+=1;let answer=*input;if number!=answer{return 91;}number=9;return number-9;},
        .ok(input)=>{var task=give input;var answer=0;loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}task_request_cancel(&mut task);loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give task);number=9;return answer-number-33;}
    }
}
"#;
const TWO_BORROWED: &str = r#"
fn worker(first:&mut i32,second:&mut i32)->i32 {*first+=1;task_yield();*second+=2;return *first+*second;}
fn main()->i32 {
    var number=37;var other=43;
    match task_create_owned(worker,&mut number,&mut other) {
        .err(failure)=>{let (first,second)=failure.inputs;*first+=1;*second+=2;let answer=*first+*second;if number!=38{return 91;}if other!=45{return 92;}return answer-83;},
        .ok(input)=>{var task=give input;var answer=0;loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}task_request_cancel(&mut task);loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give task);if number!=38{return 93;}if other!=45{return 94;}return answer-83;}
    }
}
"#;
#[test]
fn two_same_type_borrowed_inputs_preserve_distinct_original_roots() {
    for (text, success) in [
        (TWO_BORROWED.to_owned(), true),
        (
            TWO_BORROWED.replace(
                "worker,&mut number,&mut other",
                "worker,&mut number,&mut number",
            ),
            false,
        ),
    ] {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status == ResultCategory::Success,
            success,
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn borrowed_input_failure_keeps_the_original_parent_loan_until_consumed() {
    let text = BORROWED;
    for (text, expected) in [
        (text.to_owned(), true),
        (
            text.replace(
                "let (input,)=failure.inputs;",
                "number=99;let (input,)=failure.inputs;",
            ),
            false,
        ),
    ] {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status == ResultCategory::Success,
            expected,
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
fn owned_constructor_does_not_erase_a_live_owner_on_later_argument_early_return() {
    let prepare = "fn prepare(flag:bool)->TaskCreateError!i32{if flag{return TaskCreateError.Allocation();}return 37;}\n";
    let text = REGISTRATION
        .replace("fn main()->i32", "fn main()->TaskCreateError!i32")
        .replace(
            "task_create_owned(worker,give owner,37)",
            "task_create_owned(worker,give owner,prepare(true)?)",
        );
    let report = compile_source(
        &source(&format!("{prepare}{text}")),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
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
            .any(|d| d.message.contains("registration")
                || d.message.contains("asynchronous")
                || d.message.contains("responsibility")),
        "{}",
        report.to_json()
    );
}

#[test]
fn borrowed_failure_native_keeps_the_original_input_until_its_last_use() {
    failure_native_matrix(
        "borrowed",
        &[BORROWED, TWO_BORROWED],
        "create-owned-borrowed-observer.c",
        b"original borrowed input retained through constructor outcome\n",
        &[],
    );
}
fn failure_native_matrix(
    tag: &str,
    texts: &[&str],
    observer_file: &str,
    expected_stdout: &[u8],
    case_environment: &[(&str, &str)],
) {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, text) in texts.iter().copied().enumerate() {
                let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../out/runs")
                    .join(format!(
                        "zil-create-owned-{tag}-{}-{backend:?}-{optimization:?}-{case}",
                        std::process::id()
                    ));
                fs::create_dir_all(&dir).unwrap();
                let clang = std::env::var_os("ZIL_CLANG")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "clang".into());
                let object = dir.join("observer.o");
                let observer = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/language-readiness/task-thread-transfer")
                    .join(observer_file);
                fs::write(dir.join("input.zil"), text).unwrap();
                fs::copy(&observer, dir.join("observer.c")).unwrap();
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
                if report.status != ResultCategory::Success {
                    let input = source(text);
                    let ast = zil_syntax::parse(&input).into_result().unwrap();
                    let core = zil_expand::expand(&input, &ast).unwrap();
                    let typed = zil_types::check(
                        &core,
                        &zil_base::TargetProfile::default(),
                        &zil_base::Limits::default(),
                    )
                    .unwrap();
                    fs::write(dir.join("typed.txt"), format!("{typed:#?}")).unwrap();
                }
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                for fail in [false, true] {
                    let mut command = Command::new(&report.artifacts["executable"]);
                    command.envs(case_environment.iter().copied());
                    if fail {
                        command.env("ZIL_TASK_ALLOC_FAIL", "1");
                    }
                    let output = command.output().unwrap();
                    assert_eq!(
                        output.status.code(),
                        Some(0),
                        "{backend:?}/{optimization:?}/{fail}: {output:?}"
                    );
                    assert_eq!(output.stdout, expected_stdout);
                }
                fs::write(
                    dir.join("result.txt"),
                    "two backends / optimization / allocation outcomes observed",
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn borrowed_failure_raw_and_wire_reject_missing_inputs_and_wrong_parent_projection() {
    use zil_ir::{InstructionKind as I, LifeNominalActual, PlaceProjection as P, Type};
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let input = source(BORROWED);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let analyzed = zil_analyze::analyze(&typed, &limits);
    if analyzed.is_err() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../out/runs/task-failure-borrowed-first-counterexample");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.zil"), BORROWED).unwrap();
        std::fs::write(dir.join("typed.txt"), format!("{typed:#?}")).unwrap();
        std::fs::write(dir.join("diagnostics.txt"), format!("{analyzed:#?}")).unwrap();
    }
    let plan = analyzed.unwrap();
    for mutation in 0..4 {
        let mut forged = typed.clone();
        match mutation {
            0 => {
                let path = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| match &mut i.kind {
                        I::LoadStoredBorrow {
                            path,
                            mode: zil_ir::StoredBorrowLoad::Take,
                            ..
                        } => Some(path),
                        _ => None,
                    })
                    .unwrap();
                *path = vec![P::Field(0)];
            }
            1 => {
                let fields = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| match &mut i.kind {
                        I::UnpackTuple { fields, .. } => Some(fields),
                        _ => None,
                    })
                    .unwrap();
                fields.clear();
            }
            2 => {
                let ports = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| match &mut i.kind {
                        I::TaskCreateOwned { port_bindings, .. } => Some(port_bindings),
                        _ => None,
                    })
                    .unwrap();
                ports.clear();
            }
            3 => {
                let context = forged
                    .life_instances
                    .iter_mut()
                    .find_map(|instance| {
                        if instance.declaration != zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
                        {
                            return None;
                        }
                        match instance.arguments.get_mut(1) {
                            Some(LifeNominalActual::Type(Type::Task { retained_life, .. })) => {
                                Some(retained_life)
                            }
                            _ => None,
                        }
                    })
                    .unwrap();
                context.ports.clear();
            }
            _ => unreachable!(),
        }
        assert!(
            zil_analyze::analyze(&forged, &limits).is_err(),
            "Analyze mutation={mutation}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        assert!(
            zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits).is_err(),
            "Verify mutation={mutation}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire mutation={mutation}"
        );
    }
}

const TWO_REGISTRATIONS: &str = r#"
extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
extern "C" fn mark_worker_entry();
extern "C" fn mark_worker_ready();
extern "C" fn cancel_after_finish()->i32;
fn entered(){unsafe{mark_worker_entry();}}
fn reached_ready(){unsafe{mark_worker_ready();}}
fn should_stop()->bool{unsafe{return cancel_after_finish()!=0;}}
fn make_callback(seed:i32)->impl FnMut(i32)->i32 {var count=seed;return move(value:i32)->i32=>{count+=1;return count+value;};}
fn request_pair<F:FnMut(i32)->i32 + ClosedEnvironment>(left:&mut AsyncRegistration<F>,right:&mut AsyncRegistration<F>) contract{requires(state(left,Open,Closing,Closed),state(right,Open,Closing,Closed));provides(state(left,Closing,Closed),state(right,Closing,Closed));}{async_registration_request_close(left);async_registration_request_close(right);}
fn poll_pair<F:FnMut(i32)->i32 + ClosedEnvironment>(left:&mut AsyncRegistration<F>,right:&mut AsyncRegistration<F>)->Poll<()> contract{requires(state(left,Closing,Closed),state(right,Closing,Closed));provides(on_pending(state(left,Closing)),on_ready(state(left,Closed)),on_pending(state(right,Closing,Closed)),on_ready(state(right,Closing,Closed)));}{return async_registration_poll_close(left);}
fn finish_pair<F:FnMut(i32)->i32 + ClosedEnvironment>(give left:AsyncRegistration<F>,give right:AsyncRegistration<F>) contract{requires(state(left,Closed),state(right,Closed));provides(retired(left),retired(right));}{async_registration_finish_destroy_checked(give left);async_registration_finish_destroy_checked(give right);}
fn worker<F:FnMut(i32)->i32 + ClosedEnvironment>(give left:AsyncRegistration<F>,give right:AsyncRegistration<F>,seed:i32)->i32 contract{requires(state(left,Open,Closing,Closed),state(right,Open,Closing,Closed));provides(retired(left),retired(right));} {
    entered();var first=give left;var second=give right;task_yield();
    request_pair(&mut first,&mut second);loop{match poll_pair(&mut first,&mut second){Poll.Pending()=>{task_yield();continue;},Poll.Ready(_)=>{break;}}}
    task_yield();loop{match async_registration_poll_close(&mut second){Poll.Pending()=>{task_yield();continue;},Poll.Ready(_)=>{break;}}}finish_pair(give first,give second);task_yield();reached_ready();return seed+5;
}
fn main()->i32 {unsafe {
    let left=c_register_async(reg,request,drain,make_callback(37));let right=c_register_async(reg,request,drain,make_callback(43));
    match task_create_owned(worker,give left,give right,37) {
        .err(failure)=>{let (left,right,seed)=failure.inputs;var first=give left;var second=give right;
            request_pair(&mut first,&mut second);loop{match poll_pair(&mut first,&mut second){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
            loop{match async_registration_poll_close(&mut second){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}finish_pair(give first,give second);return seed-35;},
        .ok(input)=>{var task=give input;var answer=0;var rounds=0;loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{rounds+=1;if rounds==5 && should_stop(){answer=42;break;}continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}task_request_cancel(&mut task);loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give task);return answer-42;}
    }
}}
"#;
#[test]
fn two_same_type_active_inputs_keep_each_protocol_role_on_both_constructor_outcomes() {
    let report = compile_source(
        &source(TWO_REGISTRATIONS),
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
#[test]
fn second_protocol_role_cannot_be_omitted_forged_or_aliased_in_raw_or_wire() {
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let input = source(TWO_REGISTRATIONS);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let positive_bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&positive_bytes, &target, &limits).unwrap();
    let worker = typed
        .functions
        .iter()
        .find(|f| {
            f.protocol_contracts.len() == 2
                && f.protocol_contracts[0].entry.len() == 3
                && f.call_kind == zil_ir::CallKind::Suspend
        })
        .unwrap();
    assert_eq!(
        worker.locals[worker.parameters[0].0 as usize].ty,
        worker.locals[worker.parameters[1].0 as usize].ty
    );
    for mutation in 0..7 {
        let mut forged = typed.clone();
        let worker = &mut forged.functions[worker.id.0 as usize];
        match mutation {
            0 => {
                worker.protocol_contracts.pop();
            }
            1 => {
                worker.protocol_contracts[1].entry = vec![zil_ir::ProtocolState::Closed];
            }
            2 => {
                worker.protocol_contracts[1].exits.clear();
            }
            3 => {
                worker.protocol_contracts[1].parameter = worker.protocol_contracts[0].parameter;
            }
            4 => {
                for row in &mut worker.protocol_contracts[1].exits {
                    row.output = vec![zil_ir::ProtocolState::Closed];
                }
            }
            5 => {
                let helper = forged
                    .functions
                    .iter_mut()
                    .find(|f| {
                        f.protocol_contracts.len() == 2
                            && f.return_type == zil_ir::Type::Unit
                            && matches!(
                                f.locals[f.parameters[0].0 as usize].ty,
                                zil_ir::Type::Address { .. }
                            )
                    })
                    .unwrap();
                let target = helper.id;
                let arguments = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| match &mut i.kind {
                        zil_ir::InstructionKind::Call {
                            function,
                            arguments,
                        } if *function == target => Some(arguments),
                        _ => None,
                    })
                    .unwrap();
                arguments[1] = arguments[0];
            }
            6 => {
                let helper=forged.functions.iter_mut().find(|f|f.protocol_contracts.len()==2 && matches!(&f.return_type,zil_ir::Type::Enum{name,..} if name.starts_with("@std.task:v1:Poll:"))).unwrap();
                let ready = helper.protocol_contracts[1]
                    .exits
                    .iter_mut()
                    .find(|r| r.exit == zil_ir::ProtocolExit::PollReady)
                    .unwrap();
                ready.output = vec![zil_ir::ProtocolState::Closed];
            }
            _ => unreachable!(),
        }
        assert!(
            zil_analyze::analyze(&forged, &limits).is_err(),
            "Analyze mutation={mutation}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        assert!(
            zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits).is_err(),
            "Verify mutation={mutation}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire mutation={mutation}"
        );
    }
}

#[test]
fn two_live_protocol_roles_native_preserve_the_actual_same_type_owner_heads() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../out/runs")
                .join(format!(
                    "zil-create-owned-two-registrations-{}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
            fs::create_dir_all(&dir).unwrap();
            let clang = std::env::var_os("ZIL_CLANG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "clang".into());
            let object = dir.join("observer.o");
            let observer=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/language-readiness/task-thread-transfer/create-owned-two-registrations-observer.c");
            fs::write(dir.join("input.zil"), TWO_REGISTRATIONS).unwrap();
            fs::copy(&observer, dir.join("observer.c")).unwrap();
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
                &source(TWO_REGISTRATIONS),
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
            for (fail, cancel) in [(false, false), (true, false), (false, true)] {
                let mut command = Command::new(&report.artifacts["executable"]);
                if fail {
                    command.env("ZIL_TASK_ALLOC_FAIL", "1");
                }
                if cancel {
                    command.env("ZIL_TASK_CANCEL_AFTER_FINISH", "1");
                }
                let output = command.output().unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(if fail { 2 } else { 0 }),
                    "{backend:?}/{optimization:?}/{fail}: {output:?}"
                );
                assert_eq!(
                    output.stdout,
                    b"two original same-type active inputs retired once through the actual helper heads\n"
                );
            }
            fs::write(
                dir.join("result.txt"),
                "actual two helper heads quarantined after one free each",
            )
            .unwrap();
        }
    }
}

#[test]
fn a_returned_poll_for_one_role_never_closes_the_other_same_type_role() {
    let forged = TWO_REGISTRATIONS.replace(
        "on_ready(state(right,Closing,Closed))",
        "on_ready(state(right,Closed))",
    );
    let report = compile_source(
        &source(&forged),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn an_undeclared_return_phase_includes_the_actual_closing_owner() {
    let factory = "fn closing<F:FnMut(i32)->i32 + ClosedEnvironment>(callback:F)->AsyncRegistration<F> contract{result(create);}{unsafe{var owner=c_register_async(reg,request,drain,callback);async_registration_request_close(&mut owner);return give owner;}}\n";
    let text = REGISTRATION
        .replacen("fn worker<", &format!("{factory}fn worker<"), 1)
        .replace(
            "let owner=c_register_async(reg,request,drain,callback);",
            "let owner=closing(callback);",
        );
    for (text, expected) in [
        (text.clone(), true),
        (
            text.replace(
                "state(owner,Open,Closing,Closed)",
                "state(owner,Open,Closed)",
            ),
            false,
        ),
    ] {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status == ResultCategory::Success,
            expected,
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

const MIXED_BORROWED_OWNER: &str = r#"
extern "C" fn mixed_input_drop(code:i32);
extern "C" fn mixed_worker_entry();
extern "C" fn mixed_worker_ready();
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{mixed_input_drop(.code);}}}
fn observe_entry(){unsafe{mixed_worker_entry();}}
fn observe_ready(){unsafe{mixed_worker_ready();}}
fn worker(give token:Token,input:&mut i32)->i32 {observe_entry();*input+=token.code;task_yield();observe_ready();return *input;}
fn main()->i32 {
 var number=37;let token=Token{code:5};
 match task_create_owned(worker,give token,&mut number) {
 .err(failure)=>{let (token,input)=failure.inputs;*input+=token.code;let answer=*input;if number!=answer{return 91;}return answer-42;},
 .ok(input)=>{var task=give input;var answer=0;loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}task_request_cancel(&mut task);loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give task);return answer-number;}
 }
}
"#;
#[test]
fn mixed_owner_and_borrowed_failure_preserves_both_original_responsibilities() {
    let report = compile_source(
        &source(MIXED_BORROWED_OWNER),
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

#[test]
fn mixed_owner_and_borrowed_failure_native_moves_the_original_owner_once() {
    failure_native_matrix(
        "mixed",
        &[MIXED_BORROWED_OWNER],
        "create-owned-mixed-observer.c",
        b"mixed original owner and borrow survived exactly once\n",
        &[],
    );
    let (before, _) = MIXED_BORROWED_OWNER.split_once(".ok(input)=>").unwrap();
    let cancel = format!(
        "{before}.ok(input)=>{{var task=give input;match task_poll(&mut task){{.ok(p)=>{{match p{{Poll.Pending()=>{{}},Poll.Ready(_)=>{{return 97;}}}}}},.err(_)=>{{return 98;}}}}task_request_cancel(&mut task);loop{{match task_poll_close(&mut task){{Poll.Pending()=>{{continue;}},Poll.Ready(_)=>{{break;}}}}}}task_finish_destroy(give task);return number-42;}}\n}}\n}}\n"
    );
    failure_native_matrix(
        "mixed-cancel",
        &[&cancel],
        "create-owned-mixed-observer.c",
        b"mixed original owner and borrow survived exactly once\n",
        &[("ZIL_EXPECT_CANCEL", "1")],
    );
}

const TWO_MIXED_BORROWED_OWNERS: &str = r#"
extern "C" fn mixed_input_drop(code:i32);
extern "C" fn mixed_worker_entry();
extern "C" fn mixed_worker_ready();
fn observe_entry(){unsafe{mixed_worker_entry();}}
fn observe_ready(){unsafe{mixed_worker_ready();}}
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{mixed_input_drop(.code);}}}
fn worker(give first:Token,give second:Token,first_input:&mut i32,second_input:&mut i32)->i32 {observe_entry();*first_input+=first.code;*second_input+=second.code;task_yield();observe_ready();return *first_input+*second_input;}
fn main()->i32 {
 var number=37;var other=43;let first=Token{code:5};let second=Token{code:7};
 match task_create_owned(worker,give first,give second,&mut number,&mut other) {
 .err(failure)=>{let (first,second,first_input,second_input)=failure.inputs;*first_input+=first.code;*second_input+=second.code;let answer=*first_input+*second_input;if number!=42{return 91;}if other!=50{return 92;}number=9;other=11;return answer-92;},
 .ok(input)=>{var task=give input;var answer=0;loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}task_request_cancel(&mut task);loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give task);if number!=42{return 93;}if other!=50{return 94;}number=9;other=11;return answer-92;}
 }
}
"#;
#[test]
fn mixed_failure_raw_and_wire_keep_each_original_owner_and_borrow_slot() {
    use zil_ir::{InstructionKind as I, Type};
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let input = source(TWO_MIXED_BORROWED_OWNERS);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for mutation in 0..7 {
        let mut forged = typed.clone();
        match mutation {
            0 => {
                let i = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TakePackageField { index: 1, .. }))
                    .unwrap();
                let I::TakePackageField { index, .. } = &mut i.kind else {
                    unreachable!()
                };
                *index = 0;
            }
            1 | 2 => {
                let fields = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| match &mut i.kind {
                        I::UnpackTuple { fields, .. } => Some(fields),
                        _ => None,
                    })
                    .unwrap();
                if mutation == 1 {
                    fields.pop();
                } else {
                    fields[1] = fields[0];
                }
            }
            3 | 4 => {
                let i = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreateOwned { .. }))
                    .unwrap();
                let I::TaskCreateOwned {
                    arguments,
                    port_bindings,
                    ..
                } = &mut i.kind
                else {
                    unreachable!()
                };
                if mutation == 3 {
                    arguments[1] = arguments[0];
                } else {
                    port_bindings[1] = port_bindings[0].clone();
                }
            }
            5 => {
                for b in forged.functions.iter_mut().flat_map(|f| &mut f.blocks) {
                    b.instructions
                        .retain(|i| !matches!(i.kind, I::UnpackTuple { .. }));
                }
            }
            6 => {
                let instance = forged
                    .life_instances
                    .iter_mut()
                    .find(|i| i.declaration == zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE)
                    .unwrap();
                assert!(
                    matches!(&instance.ty,Type::BorrowedStorage{storage,..} if matches!(storage.as_ref(),Type::Package{..}))
                );
                instance.declaration = zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD;
            }
            _ => unreachable!(),
        }
        assert!(
            zil_analyze::analyze(&forged, &limits).is_err(),
            "Analyze mixed mutation={mutation}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        assert!(
            zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits).is_err(),
            "Verify mixed mutation={mutation}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire mixed mutation={mutation}"
        );
    }
    for text in [
        TWO_MIXED_BORROWED_OWNERS.replace(
            "*first_input+=first.code;*second_input+=second.code;let answer",
            "number=0;*first_input+=first.code;*second_input+=second.code;let answer",
        ),
        TWO_MIXED_BORROWED_OWNERS.replace(
            "*first_input+=first.code;*second_input+=second.code;let answer",
            "other=0;*first_input+=first.code;*second_input+=second.code;let answer",
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
    }
}
#[test]
fn mixed_two_same_type_owners_and_borrows_native_keep_order_and_cleanup_once() {
    failure_native_matrix(
        "mixed-two",
        &[TWO_MIXED_BORROWED_OWNERS],
        "create-owned-mixed-observer.c",
        b"mixed original owner and borrow survived exactly once\n",
        &[("ZIL_MIXED_TWO", "1")],
    );
}

fn borrowed_registration_source() -> String {
    REGISTRATION
        .replace(
            "give owner:AsyncRegistration<F>,seed:i32",
            "give owner:AsyncRegistration<F>,input:&mut i32",
        )
        .replace("return seed+5;", "*input+=5;return *input;")
        .replace("var count=37;", "var number=37;var count=37;")
        .replace(
            "task_create_owned(worker,give owner,37)",
            "task_create_owned(worker,give owner,&mut number)",
        )
        .replace(
            "let (input,seed)=failure.inputs;var remaining=give input;",
            "let (owner,input)=failure.inputs;var remaining=give owner;",
        )
        .replace(
            "return seed-35;",
            "let answer=*input;if number!=answer{return 91;}number=9;return answer-37;",
        )
}
#[test]
fn live_registration_and_borrowed_failure_keep_both_phase_and_parent_roots() {
    let text = borrowed_registration_source();
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
#[test]
fn live_registration_and_borrowed_failure_native_closes_original_phase_once() {
    let text = borrowed_registration_source();
    failure_native_matrix(
        "borrowed-registration",
        &[&text],
        "create-owned-registration-borrowed-observer.c",
        b"original active input drained after allocation outcome; once free\n",
        &[],
    );
}

fn registration_retry_source(text: &str) -> String {
    fn body<'a>(text: &'a str, needle: &str) -> &'a str {
        let start = text.find(needle).unwrap() + needle.len() - 1;
        let mut depth = 0usize;
        for (offset, byte) in text.as_bytes()[start..].iter().enumerate() {
            if *byte == b'{' {
                depth += 1;
            } else if *byte == b'}' {
                depth -= 1;
                if depth == 0 {
                    return &text[start..=start + offset];
                }
            }
        }
        panic!("complete branch body");
    }
    let err = body(text, ".err(failure)=>{");
    let ok = body(text, ".ok(input)=>{");
    text.replacen(&format!(".err(failure)=>{err}"),&format!(".err(failure)=>{{let(owner,input)=failure.inputs;match task_create_owned(worker,give owner,input){{.err(failure)=>{err},.ok(input)=>{ok}}}}}"),1)
}
#[test]
fn live_registration_and_borrowed_failure_native_cancel_and_retry_keep_the_original_frame_inputs() {
    let text = borrowed_registration_source();
    let (before, _) = text.split_once(".ok(input)=>").unwrap();
    let cancel = format!(
        "{before}.ok(input)=>{{var task=give input;var mismatch=0;match task_poll(&mut task){{.ok(p)=>{{match p{{Poll.Pending()=>{{}},Poll.Ready(_)=>{{mismatch=97;}}}}}},.err(_)=>{{mismatch=98;}}}}task_request_cancel(&mut task);loop{{match task_poll_close(&mut task){{Poll.Pending()=>{{continue;}},Poll.Ready(_)=>{{break;}}}}}}task_finish_destroy(give task);return mismatch+number-37;}}\n}}\n}}\n}}\n"
    );
    failure_native_matrix(
        "borrowed-registration-cancel",
        &[&cancel],
        "create-owned-registration-borrowed-observer.c",
        b"original active input drained after allocation outcome; once free\n",
        &[],
    );
    let retry = registration_retry_source(&text);
    failure_native_matrix(
        "borrowed-registration-retry",
        &[&retry],
        "create-owned-registration-borrowed-observer.c",
        b"original active input drained after allocation outcome; once free\n",
        &[("ZIL_RETRY", "1")],
    );
}
#[test]
fn actual_registration_scope_and_phase_cannot_be_erased_by_failure_metadata() {
    use zil_ir::{InstructionKind as I, StorageLifeContract};
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let text = borrowed_registration_source();
    let input = source(&text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    for mutation in 0..4 {
        let mut forged = typed.clone();
        match mutation {
            0 => {
                let d = forged.closures.first_mut().unwrap();
                let c = StorageLifeContract {
                    static_bindings: Vec::new(),
                    fixed_binder_count: 1,
                    invocation_binder_count: 0,
                    bindings: vec![],
                    classes: vec![],
                };
                d.storage_life = Some(c.clone());
                forged.functions[d.function.0 as usize].storage_life = Some(c);
            }
            1 => {
                let d = forged.closures.first_mut().unwrap();
                let c = StorageLifeContract {
                    static_bindings: Vec::new(),
                    fixed_binder_count: 0,
                    invocation_binder_count: 0,
                    bindings: vec![],
                    classes: vec![],
                };
                d.storage_life = Some(c);
            }
            2 => {
                let i = forged
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreateOwned { .. }))
                    .unwrap();
                let I::TaskCreateOwned { port_bindings, .. } = &mut i.kind else {
                    unreachable!()
                };
                port_bindings.clear();
            }
            3 => {
                let f = forged
                    .functions
                    .iter_mut()
                    .find(|f| f.name == "main")
                    .unwrap();
                let mut changed = false;
                for block in &mut f.blocks {
                    if let Some(index) = block
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, I::AsyncRegistrationPollClose { .. }))
                    {
                        let unit = zil_ir::ValueId(f.values.len() as u32);
                        f.values.push(zil_ir::Type::Unit);
                        let span = block.instructions[index].span;
                        block.instructions[index].kind = I::MakeVariant {
                            variant: 1,
                            fields: vec![unit],
                        };
                        block.instructions.insert(
                            index,
                            zil_ir::Instruction {
                                result: Some(unit),
                                kind: I::Constant(zil_ir::Constant::Unit),
                                span,
                            },
                        );
                        changed = true;
                        break;
                    }
                }
                assert!(changed);
            }
            _ => unreachable!(),
        }
        assert!(
            zil_analyze::analyze(&forged, &limits).is_err(),
            "A actual scope/phase mutation={mutation}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        assert!(
            zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits).is_err(),
            "V actual scope/phase mutation={mutation}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire actual scope/phase mutation={mutation}"
        );
    }
    for bad in [
        text.replace("let answer=*input;", "number=0;let answer=*input;"),
        text.replacen(
            "var remaining=give owner;async_registration_request_close",
            "return 0;var remaining=give owner;async_registration_request_close",
            1,
        ),
    ] {
        let report = compile_source(
            &source(&bad),
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
    }
}

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
fn worker<F:FnMut(i32)->i32 + ClosedEnvironment>(give owner:AsyncRegistration<F>,seed:i32)->i32 contract{requires(state(owner,Open,Closing,Closed));} {
    var remaining=give owner;task_yield();async_registration_request_close(&mut remaining);
    loop{match async_registration_poll_close(&mut remaining){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
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

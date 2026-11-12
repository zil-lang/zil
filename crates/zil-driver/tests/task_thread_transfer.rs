use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/task-thread-transfer/scalar.zil");
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "task-transfer.zil", text)
}
#[test]
fn actual_transfer_task_source_reaches_both_kernels() {
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

#[test]
fn actual_transfer_frame_native_join_and_original_owner_failure() {
    use std::{fs,os::unix::fs::PermissionsExt,path::{Path,PathBuf},process::Command};
    use zil_driver::{NativeBackend,Optimization};
    let text=SOURCE.replace("fn main()", "extern \"C\" fn thread_body_called();\nextern \"C\" fn thread_body_dropped(code:i32);\nresource BodyMark { code:i32 }\nimpl Drop for BodyMark {fn drop(self:Self){unsafe{thread_body_dropped(.code);}}}\nfn main()")
        .replace("match thread_scope(drive,give task,()=>9)","let mark=BodyMark{code:9};match thread_scope(drive,give task,move ()->i32=>{let code=mark.code;unsafe{thread_body_called();}return code;})");
    let resumed=text.replace("task_request_cancel(&mut remaining);", "match task_poll(&mut remaining){.ok(p)=>{match p{Poll.Pending()=>{return 97;},Poll.Ready(v)=>{if v!=42{return 98;}}}},.err(_)=>{return 99;}}task_request_cancel(&mut remaining);");
    for backend in [NativeBackend::Clang,NativeBackend::LlvmApi] {
        if backend==NativeBackend::LlvmApi && !cfg!(feature="llvm-api") {continue;}
        for optimization in [Optimization::O0,Optimization::O2] {
            for (name,text) in [("cancel",&text),("resume",&resumed)] {
                let dir=std::env::temp_dir().join(format!("zil-task-thread-{}-{name}-{backend:?}-{optimization:?}",std::process::id()));fs::create_dir_all(&dir).unwrap();
                let clang=std::env::var_os("ZIL_CLANG").map(PathBuf::from).unwrap_or_else(||"clang".into());
                let object=dir.join("observer.o");
                let observer=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/language-readiness/task-thread-transfer/thread-observer.c");
                let built=Command::new(&clang).args(["-std=c11","-Wall","-Wextra","-Werror","-c"]).arg(observer).arg("-o").arg(&object).output().unwrap();assert!(built.status.success(),"{built:?}");
                let quote=|path:&Path|format!("'{}'",path.to_string_lossy().replace('\'',"'\"'\"'"));
                let wrapper=dir.join("link.sh");fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free -Wl,--wrap=pthread_create -Wl,--wrap=pthread_join -pthread\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&object),quote(&clang))).unwrap();fs::set_permissions(&wrapper,fs::Permissions::from_mode(0o700)).unwrap();
                let report=compile_source(&source(text),&CompileOptions{backend,optimization,clang:Some(wrapper),stop_after:Stage::Build,output:Some(dir.join("program")),..CompileOptions::default()});
                assert_eq!(report.status,ResultCategory::Success,"{name}: {}",report.to_json());
                let codegen=report.native_codegen.as_ref().unwrap();assert_eq!(codegen.backend,backend);assert!(codegen.fallback.is_none());
                let program=&report.artifacts["executable"];
                let success=Command::new(program).output().unwrap();assert_eq!(success.status.code(),Some(0),"{name}: {success:?}");assert_eq!(success.stdout,b"K");
                let failed=Command::new(program).env("ZIL_TASK_THREAD_FAIL","1").output().unwrap();assert_eq!(failed.status.code(),Some(2),"{name}: {failed:?}");assert_eq!(failed.stdout,b"K");
                let join=Command::new(program).env("ZIL_TASK_THREAD_JOIN_FAIL","1").output().unwrap();assert!(!join.status.success(),"join failure returned Completed");assert!(join.stdout.is_empty(),"join failure ran normal finalizer");
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

const THREADED:&str=include_str!("../../../tests/language-readiness/task-thread-transfer/threaded.zil");
#[test]
fn actual_provider_migration_requires_each_explicit_driver_promise() {
    for (name,text,accept) in [
        ("promised",THREADED.to_owned(),true),
        ("callback-only",THREADED.replace(",thread_async_driver_transfer()",""),false),
        ("missing-register",THREADED.replacen(",thread_async_driver_transfer()","",1),false),
        ("same-thread",THREADED.replace("thread_async_callback_","async_callback_"),false),
        ("missing-close",THREADED.replace("thread_async_callback_request_close(),thread_async_driver_transfer()","thread_async_callback_request_close()"),false),
        ("missing-drain",THREADED.replace("thread_async_callback_poll_drain(),thread_async_driver_transfer()","thread_async_callback_poll_drain()"),false),
    ] {
        let report=compile_source(&source(&text),&CompileOptions{stop_after:Stage::Verify,..CompileOptions::default()});
        assert_eq!(report.status==ResultCategory::Success,accept,"{name}: {}",report.to_json());
        assert_ne!(report.status,ResultCategory::InfraError,"{name}: {}",report.to_json());
        if accept {assert_eq!(report.verification_runs,1);}
    }
}
#[test]
fn actual_retained_task_roots_survive_failure_and_release_after_join() {
    let text=SOURCE.replace("compute(seed:i32)","compute(seed:&mut i32)").replace("let saved=seed+1;","*seed+=1;let saved=*seed;")
        .replace("fn drive(give input:Task<i32>)","fn drive<life a>(give input:Task<i32,&'a mut i32>)")
        .replace("var task=task_create(compute,37)?;","var number=37;var task=task_create(compute,&mut number)?;")
        .replace("TaskThreadScopeResult.Completed(answer,body)=>{return answer+body-51;}","TaskThreadScopeResult.Completed(answer,body)=>{number=9;return answer+body-number-42;}");
    let early=text.replace("var remaining=give input;","number=99;var remaining=give input;");
    let overlap=text.replace("()=>9","()=>{number=9;return number;}");
    for (name,text,accept) in [("joined",text,true),("failed-retains",early,false),("parent-overlap",overlap,false)] {
        let report=compile_source(&source(&text),&CompileOptions{stop_after:Stage::Verify,..CompileOptions::default()});
        assert_eq!(report.status==ResultCategory::Success,accept,"{name}: {}",report.to_json());
        assert_ne!(report.status,ResultCategory::InfraError,"{name}: {}",report.to_json());
    }
}

#[test]
fn active_threaded_frame_migration_busy_raw_holder_and_late_wake() {
    use std::{fs,os::unix::fs::PermissionsExt,path::{Path,PathBuf},process::Command};
    use zil_driver::{NativeBackend,Optimization};
    // The host pauses an actually admitted callback with a signal/pipe barrier.
    // The generated CAS stays authoritative; the host never writes gate state.
    let text=THREADED.replace("count+=1;return value+count+i32(bytes_len(bytes))-1", "count+=1;var i=value;var noise=0;loop{if i<=0{break;}noise=(noise+i)%997;i-=1;}return value+count+i32(bytes_len(bytes))-1+noise");
    for backend in [NativeBackend::Clang,NativeBackend::LlvmApi] {
        if backend==NativeBackend::LlvmApi && !cfg!(feature="llvm-api"){continue;}
        for optimization in [Optimization::O0,Optimization::O2] {
            let dir=std::env::temp_dir().join(format!("zil-active-task-thread-{}-{backend:?}-{optimization:?}",std::process::id()));fs::create_dir_all(&dir).unwrap();
            let clang=std::env::var_os("ZIL_CLANG").map(PathBuf::from).unwrap_or_else(||"clang".into());
            let object=dir.join("observer.o");let observer=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/language-readiness/task-thread-transfer/threaded-observer.c");
            let built=Command::new(&clang).args(["-std=c11","-Wall","-Wextra","-Werror","-O2","-c"]).arg(observer).arg("-o").arg(&object).output().unwrap();assert!(built.status.success(),"{built:?}");
            let quote=|path:&Path|format!("'{}'",path.to_string_lossy().replace('\'',"'\"'\"'"));
            let wrapper=dir.join("link.sh");fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=realloc -Wl,--wrap=free -Wl,--wrap=pthread_create -Wl,--wrap=pthread_join -pthread\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&object),quote(&clang))).unwrap();fs::set_permissions(&wrapper,fs::Permissions::from_mode(0o700)).unwrap();
            let report=compile_source(&source(&text),&CompileOptions{backend,optimization,clang:Some(wrapper),stop_after:Stage::Build,output:Some(dir.join("program")),..CompileOptions::default()});assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
            assert_eq!(report.native_codegen.as_ref().unwrap().backend,backend);assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            for fail in [false,true] {let mut command=Command::new(&report.artifacts["executable"]);if fail{command.env("ZIL_TASK_THREAD_FAIL","1");}let output=command.output().unwrap();assert_eq!(output.status.code(),Some(if fail{2}else{0}),"{backend:?}/{optimization:?}/fail={fail}: {output:?}");assert_eq!(output.stdout,b"active frame moved; ClosingBusy and raw-holder drained; once cleanup\n");}
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn hidden_input_child_requires_its_actual_frame_not_its_result_type() {
    let text=SOURCE.replace("fn compute(seed:i32)->i32 contract{resume(transfer)} {\n    let saved=seed+1;\n    task_yield();\n    return saved+4;\n}","fn leaf(seed:i32)->i32 contract{resume(transfer)} {task_yield();return seed;}\nfn compute(give input:Task<i32>)->i32 contract{resume(transfer)} {var child=give input;task_yield();task_request_cancel(&mut child);loop{match task_poll_close(&mut child){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}task_finish_destroy(give child);return 42;}")
        .replace("var task=task_create(compute,37)?;","let child=task_create(leaf,37)?;var task=task_create(compute,give child)?;");
    for (name,text,accept) in [("actual-transfer-child",text.clone(),true),("hidden-local-child",text.replacen("fn leaf(seed:i32)->i32 contract{resume(transfer)}","fn leaf(seed:i32)->i32",1),false)] {
        let report=compile_source(&source(&text),&CompileOptions{stop_after:Stage::Verify,..CompileOptions::default()});
        assert_eq!(report.status==ResultCategory::Success,accept,"{name}: {}",report.to_json());
        assert_ne!(report.status,ResultCategory::InfraError,"{name}: {}",report.to_json());
    }
}
#[test]
fn actual_provider_contract_raw_wire_mutations_are_independently_rejected() {
    let limits=zil_base::Limits::default();let target=zil_base::TargetProfile::default();
    let input=source(THREADED);let ast=zil_syntax::parse(&input).into_result().unwrap();let core=zil_expand::expand(&input,&ast).unwrap();let typed=zil_types::check(&core,&target,&limits).unwrap();let plan=zil_analyze::analyze(&typed,&limits).unwrap();
    for wire in [false,true] {
        let checked=if wire {let bytes=zil_ir::wire::encode_module_candidate(&typed,&plan,&limits).unwrap();zil_verify::verify_module_bytes(&bytes,&target,&limits).unwrap()}else{zil_verify::verify_module(typed.clone(),plan.clone(),&target,&limits).unwrap()};assert_eq!(checked.module().fingerprint(),typed.fingerprint());
        for name in ["thread_register","thread_request","thread_drain"] {
            let mut forged=typed.clone();forged.functions.iter_mut().find(|f|f.name==name).unwrap().foreign.as_mut().unwrap().thread_driver_transfer=false;
            assert_ne!(forged.fingerprint(),typed.fingerprint());assert!(zil_analyze::analyze(&forged,&limits).is_err(),"Analyze {name}");
            let mut claim=plan.clone();claim.module_fingerprint=forged.fingerprint();
            let result=if wire {let bytes=zil_ir::wire::encode_module_candidate(&forged,&claim,&limits).unwrap();zil_verify::verify_module_bytes(&bytes,&target,&limits)}else{zil_verify::verify_module(forged,claim,&target,&limits)};
            assert!(result.is_err(),"Verify {name}/wire={wire}");
        }
    }
}

#[test]
fn explicit_finish_through_owned_helper_and_borrowed_alias() {
    let helper="fn finish<F:FnMut(i32)->i32 + ClosedEnvironment>(give input:AsyncRegistration<F>) contract{requires(state(input,Closed));}{async_registration_finish_destroy_checked(give input);}\n";
    let with_helper=THREADED.replacen("fn compute",&format!("{helper}fn compute"),1).replace("async_registration_finish_destroy_checked(give owner);","finish(give owner);");
    // The exclusive alias ends before the same head is moved to its helper.
    let with_alias=with_helper.replace("async_registration_request_close(&mut owner);","let closing=&mut owner;async_registration_request_close(closing);");
    for (name,text) in [("owned-helper",with_helper),("borrowed-alias",with_alias)] {
        let report=compile_source(&source(&text),&CompileOptions{stop_after:Stage::Verify,..CompileOptions::default()});
        assert_eq!(report.status,ResultCategory::Success,"{name}: {}",report.to_json());
    }
}

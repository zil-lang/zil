//! Actual Package-owned storage head projection and access-mode retention.
use super::*;
use zil_driver::{NativeBackend, Optimization};
const READ: &str = "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{return bytes_len(p.data)}fn push(p:&mut Text){bytes_reserve(p.data,1);bytes_push(p.data,42u8)}fn view(p:&Text)->&[u8]{return bytes_view(p.data)}fn result()->i64{var text=Text{data:bytes_new()};push(&mut text);let bytes=view(&text);return i64(bytes[0])+size(&text)-1}";
#[test]
fn terminal342_owned_storage_head_projects_readonly_and_exclusive_actual_modes() {
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for text in [
        READ,
        "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{let d=p.data;let f=()=>bytes_len(d);return f()}fn result()->i64{let text=Text{data:bytes_new()};return size(&text)+42}",
        "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{let f=()=>bytes_len(p.data);return f()}fn result()->i64{let text=Text{data:bytes_new()};return size(&text)+42}",
        "package Text{manages data:ByteBuffer}fn push(p:&mut Text){let d=p.data;let f=()=>{bytes_reserve(d,1);bytes_push(d,42u8)};f()}fn result()->i64{var text=Text{data:bytes_new()};push(&mut text);return i64(bytes_view(text.data)[0])}",
        "package Text{manages data:ByteBuffer}fn act(a:&Text,b:&mut Text){let read=a.data;let write=b.data;let f=()=>{let n=bytes_len(read);bytes_reserve(write,n+1);bytes_push(write,42u8)};f()}fn result()->i64{let a=Text{data:bytes_new()};var b=Text{data:bytes_new()};act(&a,&mut b);return i64(bytes_view(b.data)[0])}",
        "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{return bytes_len(p.data)}fn worker(value:i32)->i32{return value}fn act(a:ByteBuffer,b:ByteBuffer)->i64{let body=()->i32=>{let view=bytes_view(b);bytes_truncate(a,0);return i32(view.len())};let r=thread_scope(worker,0,body);return i64(r.2)}fn result()->i64{let a=bytes_new();let b=bytes_new();bytes_reserve(b,1);bytes_push(b,1u8);return act(a,b)+41}",
    ] {
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend: *backend,
                        optimization,
                        entry: Some("result".into()),
                        output: Some(temp.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{text}: {}",
                    report.to_json()
                );
                execute(&report.artifacts["executable"], 42);
            }
        }
    }
}
#[test]
fn terminal342_shared_storage_head_cannot_launder_a_write() {
    for text in [
        "package Text{manages data:ByteBuffer}fn bad(p:&Text){bytes_truncate(p.data,0)}fn main(){}",
        "package Text{manages data:ByteBuffer}fn mutate(b:ByteBuffer){bytes_truncate(b,0)}fn bad(p:&Text){mutate(p.data)}fn main(){}",
        "package Text{manages data:ByteBuffer}fn bad(p:&Text){let d=p.data;let f=()=>bytes_truncate(d,0);f()}fn main(){}",
        "package Text{manages data:ByteBuffer}fn bad(p:&Text){let f=()=>bytes_truncate(p.data,0);f()}fn main(){}",
        "package Text{manages data:ByteBuffer}fn bad(p:&mut Text){let q=&*p;bytes_push(p.data,1u8);bytes_len(q.data)}fn main(){}",
        "package Text{manages data:ByteBuffer}fn bad(p:&mut Text)->i64{let v=bytes_view(p.data);bytes_push(p.data,1u8);return v.len()}fn main(){}",
        "package Text{manages data:ByteBuffer}fn result()->i64{let text=Text{data:bytes_new()};let d=(&text).data;let moved=text;return bytes_len(d)}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn terminal342_actual_head_modes_are_independent_on_raw_and_wire_paths() {
    for source in [
        "package Text{manages data:ByteBuffer}fn mutate(p:&mut Text){bytes_truncate(p.data,0)}fn result()->i64{return 42}",
        "package Text{manages data:ByteBuffer}fn mutate(p:&mut Text){let data=p.data;let body=(cell:&MutexI32)->i32=>{bytes_truncate(data,0);return 0};mutex_scope(0,body)}fn result()->i64{return 42}",
        "package Text{manages data:ByteBuffer}fn mutate(p:&mut Text){let data=p.data;let body=(cell:&AtomicI32)->i32=>{bytes_truncate(data,0);return 0};atomic_scope(0,body)}fn result()->i64{return 42}",
    ] {
        let module = typed(source);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let wire =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
        let mut changed = module.clone();
        let function = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "mutate")
            .unwrap();
        for ty in function
            .locals
            .iter_mut()
            .map(|l| &mut l.ty)
            .chain(function.values.iter_mut())
        {
            if let zil_ir::Type::Address { mutable, .. } = ty {
                *mutable = false;
            }
        }
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::ReborrowAddress { mutable, .. } = &mut instruction.kind {
                *mutable = false;
            }
        }
        assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
        let mut candidate = plan;
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default())
            .unwrap();
        let error = zil_verify::verify_module(
            changed.clone(),
            candidate,
            &changed.target,
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-VERIFY-HEAD-PERMISSION"),
            "{error:?}"
        );
        let error = zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default())
            .unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-VERIFY-HEAD-PERMISSION"),
            "{error:?}"
        );
    }
}

#[test]
fn terminal342_head_proofs_have_fresh_exact_phase_budgets() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        phase: Phase,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap();
        while low + 1 < high {
            let n = low + (high - low) / 2;
            match run(n) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = n;
                }
                Err(errors) => {
                    assert!(
                        errors.iter().all(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{errors:?}"
                    );
                    low = n;
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap(), expected);
        high
    }
    let unit = source(READ);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limit = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let expand = boundary(Phase::Expand, |n| {
        zil_expand::expand_with_map_limits(&unit, &syntax, &limit(n)).map(|m| m.core)
    });
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let types = boundary(Phase::Types, |n| {
        zil_types::check(&core, &TargetProfile::default(), &limit(n))
    });
    let module = typed(READ);
    let analyze = boundary(Phase::Analyze, |n| zil_analyze::analyze(&module, &limit(n)));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verify = boundary(Phase::Verify, |n| {
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limit(n))
            .map(|v| v.fingerprint())
    });
    println!(
        "342 resource head N: expand={expand} types={types} analyze={analyze} verify={verify}; N-1 ResourceLimit, fresh N identical"
    );
}

#[test]
fn terminal342_original_std_string_consumer_uses_actual_borrowed_owned_heads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/terminal/345/string.zil");
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = zil_driver::compile_path(
                &path,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(temp.0.join("string")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let mut child = Command::new(&report.artifacts["executable"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() {
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    panic!("original std.string executable exceeded five seconds");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

#[test]
fn terminal342_head_module_cache_rechecks_current_body_and_actual_caller() {
    let temp = Temp::new();
    let api = "pub package Text{manages data:ByteBuffer}pub fn make()->Text{return Text{data:bytes_new()}}pub fn size(p:&Text)->i64{return bytes_len(p.data)}pub fn view(p:&Text)->&[u8]{return bytes_view(p.data)}pub fn push(p:&mut Text){bytes_push(p.data,1u8)}";
    let main = "import api;fn number()->i64{return 42}fn result()->i64{var text=api.make();return api.size(&text)+#(number())}";
    let project = |api: &str, main: &str| zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "api.zil", api),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    };
    let options = CompileOptions {
        semantic_cache: Some(temp.0.join("cache")),
        ..CompileOptions::default()
    };
    let mut session = zil_driver::CompilationSession::default();
    let cold = session.compile_project(&project(api, main), &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    for report in [
        session.compile_project(&project(api, main), &options),
        zil_driver::compile_project(&project(api, main), &options),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.cache_hit);
        assert_eq!(report.verification_runs, 1);
        assert_eq!(report.scalar_interfaces, cold.scalar_interfaces);
    }
    for (api, main) in [
        (
            api.replace(
                "return bytes_len(p.data)",
                "bytes_truncate(p.data,0);return bytes_len(p.data)",
            ),
            main.to_owned(),
        ),
        (
            api.to_owned(),
            main.replace(
                "return api.size(&text)+#(number())",
                "let view=api.view(&text);api.push(&mut text);return view.len()+#(number())",
            ),
        ),
    ] {
        for report in [
            session.compile_project(&project(&api, &main), &options),
            zil_driver::compile_project(&project(&api, &main), &options),
        ] {
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert!(report.artifacts.is_empty());
        }
    }
}

#[test]
fn terminal342_head_calls_substitute_real_storage_separation_in_both_kernels() {
    for source in [
        "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{return bytes_len(p.data)}fn act(a:ByteBuffer,b:ByteBuffer)->i64{let body=(cell:&MutexI32)->i32=>{let view=bytes_view(b);bytes_truncate(a,0);return i32(view.len())};let r=mutex_scope(0,body);return i64(r.1)}fn relay(a:ByteBuffer,b:ByteBuffer)->i64{return act(a,b)}fn entry(a:ByteBuffer,b:ByteBuffer)->i64{return relay(a,b)}",
        "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{return bytes_len(p.data)}fn act(a:ByteBuffer,b:ByteBuffer)->i64{let body=(cell:&AtomicI32)->i32=>{let view=bytes_view(b);bytes_truncate(a,0);return i32(view.len())};let r=atomic_scope(0,body);return i64(r.1)}fn relay(a:ByteBuffer,b:ByteBuffer)->i64{return act(a,b)}fn entry(a:ByteBuffer,b:ByteBuffer)->i64{return relay(a,b)}",
        "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{return bytes_len(p.data)}fn act(a:ByteBuffer,b:ByteBuffer)->i64{let view=bytes_view(b);bytes_truncate(a,0);return view.len()}fn relay(a:ByteBuffer,b:ByteBuffer)->i64{return act(a,b)}fn entry(a:ByteBuffer,b:ByteBuffer)->i64{return relay(a,b)}",
        "package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{return bytes_len(p.data)}fn worker(value:i32)->i32{return value}fn act(a:ByteBuffer,b:ByteBuffer)->i64{let body=()->i32=>{let view=bytes_view(b);bytes_truncate(a,0);return i32(view.len())};let r=thread_scope(worker,0,body);return i64(r.2)}fn relay(a:ByteBuffer,b:ByteBuffer)->i64{return act(a,b)}fn entry(a:ByteBuffer,b:ByteBuffer)->i64{return relay(a,b)}",
        r#"extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};extern "C" fn close(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};package Text{manages data:ByteBuffer}fn size(p:&Text)->i64{return bytes_len(p.data)}fn act(a:ByteBuffer,b:ByteBuffer)->i64{var callback=(value:i32)->i32=>value;let body=()->i32=>{let view=bytes_view(b);bytes_truncate(a,0);return i32(view.len())};unsafe{let r=c_with_callback(reg,close,&mut callback,body);return i64(r.1)}}fn relay(a:ByteBuffer,b:ByteBuffer)->i64{return act(a,b)}fn entry(a:ByteBuffer,b:ByteBuffer)->i64{return relay(a,b)}"#,
    ] {
        let module = typed(source);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        let mut changed = module.clone();
        let entry = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "entry")
            .unwrap();
        for i in entry.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::Call { arguments, .. } = &mut i.kind {
                arguments[1] = arguments[0];
            }
        }
        assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
        let mut candidate = plan;
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default())
            .unwrap();
        assert!(
            zil_verify::verify_module(
                changed.clone(),
                candidate,
                &changed.target,
                &Limits::default()
            )
            .is_err()
        );
        assert!(
            zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()).is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn terminal342_actual_borrowed_head_failure_restores_and_releases_owned_buffer() {
    use std::os::unix::fs::PermissionsExt;
    let text = "extern \"C\" fn byte_test_attempts()->i32;extern \"C\" fn byte_test_releases()->i32;extern \"C\" fn byte_test_live()->i32;package Text{manages data:ByteBuffer}enum Fault{Stop}fn grow(p:&mut Text)->bool{return bytes_reserve(p.data,13)}fn work()->Fault!i32{var text=Text{data:bytes_new()};if !bytes_reserve(text.data,4){return Fault.Stop}bytes_push(text.data,42u8);if !grow(&mut text){return Fault.Stop}return 7}fn result()->i32{let answer=work();unsafe{return byte_test_attempts()*100+byte_test_releases()*10000+byte_test_live()*1000000}}";
    let temp = Temp::new();
    let compiler = std::env::var("ZIL_CLANG").unwrap_or_else(|_| "clang".into());
    let shim =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/byte-buffer/096/fault-allocator.c");
    let wrapper = temp.0.join("fault-clang.py");
    fs::write(&wrapper,format!("#!/usr/bin/env python3\nimport os,sys\ncompiler={}\nextra=['-x','c',{},'-Wl,--wrap=malloc','-Wl,--wrap=realloc','-Wl,--wrap=free']\nos.execvp(compiler,[compiler]+sys.argv[1:]+extra)\n",zil_base::json_string(&compiler),zil_base::json_string(&shim.to_string_lossy()))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    entry: Some("result".into()),
                    output: Some(temp.0.join("fault")),
                    clang: Some(wrapper.clone()),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            execute(&report.artifacts["executable"], 10200);
        }
    }
}

#[test]
fn terminal342_mutable_nested_product_retains_actual_parent_and_head_modes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/terminal/342/nested_product.zil");
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = zil_driver::compile_path(
                &path,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(temp.0.join("nested-product")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let mut child = Command::new(&report.artifacts["executable"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() {
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    let _ = child.wait();
                    panic!("nested Product executable exceeded five seconds");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
    let text = "package Text{manages data:ByteBuffer}package Composition{text:Text;active:bool}fn clear(text:&mut Text){bytes_truncate(text.data,0)}fn begin(session:&mut Composition){clear(&mut session.text);session.active=true}fn result()->i64{return 42}";
    let module = typed(text);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let mut changed = module.clone();
    let f = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "begin")
        .unwrap();
    for ty in f
        .locals
        .iter_mut()
        .map(|l| &mut l.ty)
        .chain(f.values.iter_mut())
    {
        if let zil_ir::Type::Address { mutable, .. } = ty {
            *mutable = false;
        }
    }
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::ReborrowAddress { mutable, .. } = &mut i.kind {
            *mutable = false;
        }
    }
    assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
    let mut fake = plan;
    fake.module_fingerprint = changed.fingerprint();
    assert!(
        zil_verify::verify_module(
            changed.clone(),
            fake.clone(),
            &changed.target,
            &Limits::default()
        )
        .is_err()
    );
    let wire = zil_ir::wire::encode_module_candidate(&changed, &fake, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()).is_err());
    for body in [
        "fn bad(session:&Composition){clear(&mut session.text)}fn main(){}",
        "fn view(session:&Composition)->&[u8]{return bytes_view(session.text.data)}fn bad(session:&mut Composition)->i64{let v=view(&*session);begin(session);return v.len()}fn main(){}",
        "fn view(session:&Composition)->&[u8]{return bytes_view(session.text.data)}fn bad(session:&mut Composition,other:Composition)->i64{let v=view(&*session);*session=other;return v.len()}fn main(){}",
    ] {
        let report = compile_source(
            &source(&format!("{text}{body}")),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

const STORE_RESOURCE_HEAD: &str = include_str!("store_resource_head.zil");
#[test]
fn terminal342_store_resource_copy_field_uses_actual_exclusive_head_and_cleanup() {
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    let last_use = STORE_RESOURCE_HEAD.replace(
        "v.owner.code=90;return 0",
        "v.owner.code=90;let n=v.owner.code;store_reserve(s,16);return n-90",
    );
    for text in [STORE_RESOURCE_HEAD, &last_use] {
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend: *backend,
                        optimization,
                        output: Some(temp.0.join("store-head")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let mut child = Command::new(&report.artifacts["executable"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while child.try_wait().unwrap().is_none() {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("native Store head timed out");
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                let output = child.wait_with_output().unwrap();
                assert_eq!(output.status.code(), Some(0), "{output:?}");
                assert_eq!(output.stdout, b"ZB");
                assert!(output.stderr.is_empty());
            }
        }
    }
    let (model, _) = STORE_RESOURCE_HEAD.split_once("fn main()").unwrap();
    for body in [
        "let v=store_at(s,0);v.owner.code=90;return 0",
        "let v=store_at_mut(s,0);store_reserve(s,16);v.owner.code=90;return 0",
        "let v=store_at_mut(s,0);let moved=give s;v.owner.code=90;return 0",
        "let v=store_at_mut(s,0);let p=&v.owner.code;v.owner.code=90;return *p",
    ] {
        let text =
            STORE_RESOURCE_HEAD.replace("let v=store_at_mut(s,0);v.owner.code=90;return 0", body);
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{body}: {}",
            report.to_json()
        );
    }
    let text = format!(
        "{model}fn bad(s:Store<Cell>,i:i64,j:i64)->i32{{let a=store_at_mut(s,i);let b=store_at_mut(s,j);a.owner.code=90;return b.owner.code}}fn main(){{}}"
    );
    let report = compile_source(&source(&text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn terminal342_store_resource_head_raw_and_wire_keep_permissions_and_parent_windows() {
    use zil_ir::{Instruction, Type, ValueId};
    let module = typed(STORE_RESOURCE_HEAD);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    for mode in 0..3 {
        let mut changed = module.clone();
        let f = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        match mode {
            0 => {
                for ty in f
                    .values
                    .iter_mut()
                    .chain(f.locals.iter_mut().map(|l| &mut l.ty))
                {
                    if let Type::Address { mutable, .. } = ty {
                        *mutable = false;
                    }
                }
                for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                    match &mut i.kind {
                        InstructionKind::OwnedStoreBorrowMut { store, index } => {
                            i.kind = InstructionKind::OwnedStoreBorrow {
                                store: *store,
                                index: *index,
                            }
                        }
                        InstructionKind::ReborrowAddress { mutable, .. } => *mutable = false,
                        _ => {}
                    }
                }
            }
            1 => {
                let (db, di) = f
                    .blocks
                    .iter()
                    .enumerate()
                    .find_map(|(b, block)| {
                        block
                            .instructions
                            .iter()
                            .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                            .map(|i| (b, i))
                    })
                    .unwrap();
                let drop = f.blocks[db].instructions.remove(di);
                let (bb, bi) = f
                    .blocks
                    .iter()
                    .enumerate()
                    .find_map(|(b, block)| {
                        block
                            .instructions
                            .iter()
                            .position(|i| {
                                matches!(i.kind, InstructionKind::OwnedStoreBorrowMut { .. })
                            })
                            .map(|i| (b, i))
                    })
                    .unwrap();
                f.blocks[bb].instructions.insert(bi + 1, drop);
            }
            _ => {
                let capacity = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| {
                        if let InstructionKind::OwnedStoreReserve { capacity, .. } = i.kind {
                            Some(capacity)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let (bb, bi, store, span) = f
                    .blocks
                    .iter()
                    .enumerate()
                    .find_map(|(b, block)| {
                        block.instructions.iter().enumerate().find_map(|(i, inst)| {
                            if let InstructionKind::OwnedStoreBorrowMut { store, .. } = inst.kind {
                                Some((b, i, store, inst.span))
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap();
                let result = ValueId(f.values.len() as u32);
                f.values.push(Type::Bool);
                f.blocks[bb].instructions.insert(
                    bi + 1,
                    Instruction {
                        result: Some(result),
                        kind: InstructionKind::OwnedStoreReserve { store, capacity },
                        span,
                    },
                );
            }
        }
        let errors = zil_analyze::analyze(&changed, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{mode}: {errors:?}"
        );
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
        let (decoded, decoded_plan) =
            zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert!(zil_analyze::analyze(&decoded, &limits).is_err());
        for result in [
            zil_verify::verify_module(changed, candidate, &module.target, &limits),
            zil_verify::verify_module(decoded, decoded_plan, &module.target, &limits),
        ] {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject),
                "{mode}: {errors:?}"
            );
        }
    }
}

#[test]
fn terminal342_store_resource_head_substitutes_actual_store_roots_without_index_apart() {
    let (model, _) = STORE_RESOURCE_HEAD.split_once("fn main()").unwrap();
    let text = format!(
        "{model}fn act(a:Store<Cell>,b:Store<Cell>,i:i64,j:i64)->i32{{let read=store_at(b,j);let edit=store_at_mut(a,i);edit.owner.code=90;return read.owner.code+edit.owner.code}}fn main()->i32{{let a=empty();let b=empty();store_reserve(a,2);store_reserve(b,2);store_push(a,cell(65));store_push(b,cell(66));return act(a,b,0,0)-156}}"
    );
    let module = typed(&text);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let act = module
        .functions
        .iter()
        .find(|f| f.name == "act")
        .unwrap()
        .id;
    let mut changed = module.clone();
    let f = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let arguments = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            InstructionKind::Call {
                function,
                arguments,
            } if *function == act => Some(arguments),
            _ => None,
        })
        .unwrap();
    arguments[1] = arguments[0];
    assert!(zil_analyze::analyze(&changed, &limits).is_err());
    let mut candidate = plan;
    candidate.module_fingerprint = changed.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
    let (decoded, decoded_plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert!(zil_analyze::analyze(&decoded, &limits).is_err());
    assert!(zil_verify::verify_module(changed, candidate, &module.target, &limits).is_err());
    assert!(zil_verify::verify_module(decoded, decoded_plan, &module.target, &limits).is_err());
}

#[test]
fn terminal342_store_resource_head_pays_work_and_rechecks_live_restored_cache() {
    fn boundary<T: PartialEq + std::fmt::Debug>(
        phase: Phase,
        mut run: impl FnMut(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let expected = run(Limits::default().max_analysis_steps).unwrap();
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(value) => {
                    assert_eq!(value, expected);
                    high = mid
                }
                Err(errors) => {
                    assert!(
                        errors.iter().all(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{phase:?}: {errors:?}"
                    );
                    low = mid
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap(), expected);
        high
    }
    let module = typed(STORE_RESOURCE_HEAD);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let analyze = boundary(Phase::Analyze, |n| {
        zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    });
    let verify = boundary(Phase::Verify, |n| {
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|v| v.fingerprint())
    });
    println!(
        "342 Store resource head N Analyze={analyze} Verify={verify}; N-1 ResourceLimit and fresh N success"
    );
    let temp = Temp::new();
    let text = format!(
        "{}fn zero()->i32{{return 0}}",
        STORE_RESOURCE_HEAD.replace(
            "v.owner.code=90;return 0",
            "v.owner.code=90;return #(zero())"
        )
    );
    let project = |text: &str| zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)],
        paths: std::collections::BTreeMap::new(),
    };
    let options = CompileOptions {
        semantic_cache: Some(temp.0.join("store-head-cache")),
        ..CompileOptions::default()
    };
    let mut session = zil_driver::CompilationSession::default();
    let cold = session.compile_project(&project(&text), &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    for report in [
        session.compile_project(&project(&text), &options),
        zil_driver::compile_project(&project(&text), &options),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.cache_hit, "{}", report.to_json());
        assert_eq!(report.verification_runs, 1);
    }
    for bad in [
        text.replace("v.owner.code=90", "store_reserve(s,16);v.owner.code=90"),
        text.replace("store_at_mut(s,0)", "store_at(s,0)"),
    ] {
        for report in [
            session.compile_project(&project(&bad), &options),
            zil_driver::compile_project(&project(&bad), &options),
        ] {
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert!(!report.artifacts.contains_key("executable"));
        }
    }
}

#[test]
fn terminal342_stored_source_actuals_preserve_lifetime_type_const_positions() {
    use zil_syntax::ast::{GenericArg, ItemKind, TypeKind};
    let unit = source("type Alias<life a>=Carrier<'a,i64,7>;fn main(){}");
    let parsed = zil_syntax::parse(&unit).into_result().unwrap();
    let ItemKind::TypeAlias(alias) = &parsed.items[0].kind else {
        panic!("alias");
    };
    let TypeKind::Named { args, .. } = &alias.value.as_ref().unwrap().kind else {
        panic!("application");
    };
    assert!(
        matches!(args.as_slice(),[GenericArg::Lifetime(name),GenericArg::Type(_),GenericArg::Const(_)]if name=="a")
    );
    // This preserves syntax only; the actual ordinary stored-borrow consumer
    // must prove the scopes and loans before it may compile this source.
    let unit = source(include_str!("slice_iter.zil"));
    let parsed = zil_syntax::parse(&unit).into_result().unwrap();
    let ItemKind::Aggregate(iter) = &parsed.items[0].kind else {
        panic!("iterator record");
    };
    assert!(
        matches!(&iter.fields[0].ty.kind,TypeKind::Reference{lifetime:Some(name),mutable:false,..}if name=="s")
    );
}

//! Real-source controls for scalar slot promotion and its conservative boundary.
#![cfg(all(feature = "llvm-api", target_os = "linux"))]

use std::{
    fs,
    io::Write,
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization, Stage,
};
use zil_ir::{LocalId, TypedModule, ValueId};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-scalar-local-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn source(&self, text: &str) -> SourceUnit {
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            self.0.join("main.zil").to_string_lossy(),
            text,
        )
    }

    fn options(&self, backend: NativeBackend, optimization: Optimization) -> CompileOptions {
        CompileOptions {
            backend,
            optimization,
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(self.0.join("program")),
            ..CompileOptions::default()
        }
    }

    fn compile(
        &self,
        session: &mut CompilationSession,
        text: &str,
        options: &CompileOptions,
    ) -> (CompileReport, String) {
        let report = session.compile_source(&self.source(text), options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
        let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
        (report, llvm)
    }

    fn run(&self, input: &[u8]) -> Output {
        let mut child = Command::new(self.0.join("program"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("scalar-local native control timed out");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        child.wait_with_output().unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn first_function(llvm: &str) -> &str {
    // Every source below declares the function under inspection first, before
    // foreign declarations or generated closure entries. Inspect the emitted
    // source function, not the executable's result-printing wrapper.
    source_function(llvm, 0)
}

fn source_function(llvm: &str, index: u32) -> &str {
    let symbol = format!("@zil.fn.{index}(");
    llvm.split("define ")
        .find(|part| {
            part.lines()
                .next()
                .is_some_and(|line| line.contains(&symbol))
        })
        .and_then(|part| part.split_once("\n}"))
        .map(|(body, _)| body)
        .expect("source function was not emitted")
}

fn returned(output: Output, expected: i64) {
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, format!("{expected}\n").as_bytes());
    assert!(output.stderr.is_empty(), "{output:?}");
}

fn all_backends(mut control: impl FnMut(NativeBackend, Optimization)) {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        for optimization in [Optimization::O0, Optimization::O2] {
            control(backend, optimization);
        }
    }
}

#[test]
fn direct_parameters_and_same_block_assignments_keep_load_time_snapshots() {
    let text = "fn f(x:i32)->i32{var y=x;y=y+1;let old=y;y=y+2;return old+y;} \
                fn read_only(x:i32)->i32{if x>0{return x+1;}return x-1;} \
                fn result()->i32{return f(10)+read_only(1)+read_only(-1);}";
    all_backends(|backend, optimization| {
        let dir = Dir::new();
        let (_, llvm) = dir.compile(
            &mut CompilationSession::default(),
            text,
            &dir.options(backend, optimization),
        );
        let body = first_function(&llvm);
        assert!(!body.contains("alloca"), "{body}");
        assert!(!body.contains("load i32"), "{body}");
        assert!(body.contains("@llvm.sadd.with.overflow.i32"), "{body}");
        assert!(body.contains("label %zil.trap"), "{body}");
        let readonly = source_function(&llvm, 1);
        assert!(!readonly.contains("alloca"), "{readonly}");
        assert!(!readonly.contains("load i32"), "{readonly}");
        returned(dir.run(b""), 24);
    });
}

#[test]
fn narrow_wide_and_boolean_values_keep_exact_scalar_results() {
    let text = "fn byte(x:u8)->u8{var y=x;let old=y;y=y+1u8;return old^y;} \
                fn word(x:u16)->u16{var y=x;let old=y;y=y+1u16;return old^y;} \
                fn wide(x:i64)->i64{var y=x;let old=y;y=y+1;return old-y;} \
                fn flag(x:bool)->bool{var y=x;let old=y;y=!y;return old!=y;} \
                fn result()->i64{if flag(true){return i64(byte(254u8))+i64(word(65534u16))+wide(-2147483649);}return 999;}";
    all_backends(|backend, optimization| {
        let dir = Dir::new();
        let (_, llvm) = dir.compile(
            &mut CompilationSession::default(),
            text,
            &dir.options(backend, optimization),
        );
        for index in 0..4 {
            let body = source_function(&llvm, index);
            assert!(!body.contains("alloca"), "{body}");
        }
        returned(dir.run(b""), 1);
    });
}

#[test]
fn branches_loops_addresses_and_captures_retain_required_storage() {
    for (text, expected, minimum_i32_slots, borrowed) in [
        (
            "fn f(flag:bool)->i32{var y=1;if flag{y=2;}else{y=3;}return y;} fn result()->i32{return f(true)+f(false);}",
            5,
            1,
            false,
        ),
        (
            "fn f(n:i32)->i32{var y=0;var i=0;while i<n{y=y+i;i=i+1;}return y;} fn result()->i32{return f(0)+f(8);}",
            28,
            2,
            false,
        ),
        (
            "fn result()->i32{var total=0;for i in 0..4{var x=i;let old=x;x=x+1;total=total+old+x;}return total;}",
            16,
            1,
            false,
        ),
        (
            "fn result()->i32{var x=40;let old=read(&x);x=2;return old+x;} fn read(p:&i32)->i32{return *p;}",
            42,
            1,
            true,
        ),
        (
            "fn calc(base:i32)->i32{let f=x=>base+x;return f(1)+f(2);} fn result()->i32{return calc(40)+calc(100);}",
            286,
            1,
            true,
        ),
        (
            "fn result()->i32{var x=40;let a=move ()=>x;let b=()=>x;let first=b();x=99;return first+a()+x;}",
            179,
            1,
            true,
        ),
    ] {
        all_backends(|backend, optimization| {
            let dir = Dir::new();
            let (_, llvm) = dir.compile(
                &mut CompilationSession::default(),
                text,
                &dir.options(backend, optimization),
            );
            let body = first_function(&llvm);
            assert!(
                body.matches("alloca i32").count() >= minimum_i32_slots,
                "{body}"
            );
            if borrowed {
                assert!(body.contains("getelementptr i8, ptr %local."), "{body}");
            }
            returned(dir.run(b""), expected);
        });
    }
}

#[test]
fn checked_failure_occurs_between_the_same_observable_calls() {
    for (operation, failing, succeeding, expected) in
        [("y+1", "2147483647", "1", 2), ("10/y", "0", "2", 5)]
    {
        // write has no userspace stdio buffer, so a trap cannot hide an
        // incorrectly moved emit call as it could with buffered putchar.
        let text = format!(
            "fn checked(x:i32)->i32{{var y=x;emit(65u8);y={operation};emit(66u8);return y;}} \
             fn emit(c:u8){{let bytes:[u8;1]=[c];unsafe{{write(1,&bytes,1);}}}} \
             extern \"C\" fn write(fd:i32,buffer:&[u8;1],count:i64)->i64 \
             contract{{provides(noescape(buffer));effects(ffi);}}; \
             extern \"C\" fn getchar()->i32; \
             fn main()->i32{{var seed=0;unsafe{{seed=getchar();}}if seed==0{{return checked({failing});}}return checked({succeeding});}}"
        );
        all_backends(|backend, optimization| {
            let dir = Dir::new();
            let mut options = dir.options(backend, optimization);
            options.entry = None;
            let (_, llvm) = dir.compile(&mut CompilationSession::default(), &text, &options);
            let body = first_function(&llvm);
            assert!(!body.contains("alloca"), "{body}");
            let failed = dir.run(&[0]);
            assert!(failed.status.signal().is_some(), "{failed:?}");
            assert_eq!(failed.stdout, b"A", "{failed:?}");
            assert!(failed.stderr.is_empty(), "{failed:?}");
            let succeeded = dir.run(&[1]);
            assert_eq!(succeeded.status.code(), Some(expected), "{succeeded:?}");
            assert_eq!(succeeded.stdout, b"AB", "{succeeded:?}");
            assert!(succeeded.stderr.is_empty(), "{succeeded:?}");
        });
    }
}

#[test]
fn warm_objects_and_restored_source_still_require_current_verification() {
    let dir = Dir::new();
    let mut options = dir.options(NativeBackend::LlvmApi, Optimization::O2);
    options.object_cache = Some(dir.0.join("objects"));
    let mut session = CompilationSession::default();
    let mut original = None;
    for (index, amount) in [1, 1, 2, 1].into_iter().enumerate() {
        let text = format!(
            "fn f(x:i32)->i32{{var y=x;y=y+{amount};return y;}} \
             fn result()->i32{{return f(41);}}"
        );
        let (report, llvm) = dir.compile(&mut session, &text, &options);
        assert!(!first_function(&llvm).contains("alloca"), "{llvm}");
        let cache = &report.native_codegen.as_ref().unwrap().object_cache;
        assert!(cache.enabled, "{}", report.to_json());
        if index == 0 || index == 2 {
            assert!(cache.misses > 0 && cache.stored > 0, "{}", report.to_json());
        } else {
            assert!(cache.hits > 0, "{}", report.to_json());
            assert_eq!(cache.misses, 0, "{}", report.to_json());
        }
        if index == 0 {
            original = Some(llvm);
        } else if index == 3 {
            assert_eq!(Some(llvm), original);
        }
        returned(dir.run(b""), 41 + amount);
    }
}

fn typed(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "scalar-raw.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

#[test]
fn source_derived_out_of_range_ids_do_not_obtain_a_verified_capability() {
    let original = typed("fn f(x:i32)->i32{var y=x;y=y+1;return y;}");
    let candidate = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    for invalid_local in [true, false] {
        let mut module = original.clone();
        if invalid_local {
            module.functions[0].locals[0].id = LocalId(u32::MAX);
        } else {
            module.functions[0]
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.instructions)
                .find(|instruction| instruction.result.is_some())
                .unwrap()
                .result = Some(ValueId(u32::MAX));
        }
        let mut plan = candidate.clone();
        // A recomputed public fingerprint cannot make malformed IDs valid.
        plan.module_fingerprint = module.fingerprint();
        let target = module.target.clone();
        let errors =
            zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Verify);
        assert_eq!(errors[0].code, "E-VERIFY-ID");
    }
}

#[test]
fn a_borrow_conflict_remains_rejected_before_machine_emission() {
    let dir = Dir::new();
    let text = "fn result()->i32{var x=1;let f=()=>x;x=2;return f();}";
    let report =
        CompilationSession::default().compile_source(&dir.source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, "E-BORROW-CONFLICT");
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
}

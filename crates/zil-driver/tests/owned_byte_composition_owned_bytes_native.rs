//! Owning ByteBuffer composition through real LLVM, allocator and processes.
//! The C allocator observes allocation debt; it does not implement ZIL semantics.
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit, json_string};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, Stage, compile_path, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf, bool);
impl Temp {
    fn new() -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR").is_some();
        let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&base).unwrap();
        let path = base.join(format!(
            "zil-owned-byte-composition-owned-bytes-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, keep)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn build(text: &str, level: Optimization, fault: bool) -> Temp {
    build_backend(text, level, fault, NativeBackend::default())
}
fn build_backend(text: &str, level: Optimization, fault: bool, backend: NativeBackend) -> Temp {
    let temp = Temp::new();
    fs::write(temp.0.join("source.zil"), text).unwrap();
    let clang = fault.then(|| fault_clang(&temp.0));
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "owned_byte_composition_owned_bytes_native.zil",
            text,
        ),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("value".into()),
            output: Some(temp.0.join("program")),
            emit_dir: Some(temp.0.join("phases")),
            optimization: level,
            backend,
            clang,
            ..CompileOptions::default()
        },
    );
    fs::write(temp.0.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let native = report.native_codegen.as_ref().unwrap();
    assert_eq!(native.backend, backend);
    assert!(native.fallback.is_none());
    if backend == NativeBackend::LlvmApi {
        assert!(!native.object_reports.is_empty());
        assert!(native.object_reports.iter().all(|record| {
            record.contains("\"input_kind\":\"builder\"") && record.contains("\"parse_ns\":0")
        }));
    }
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("{ ptr, i64, i64 }"));
    assert!(!llvm.contains(" noalias "));
    temp
}
fn fault_clang(dir: &Path) -> PathBuf {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let source = root().join("tools/byte-buffer/owned-storage-abi/fault-allocator.c");
    let local = dir.join("fault-allocator.c");
    fs::copy(source, &local).unwrap();
    let compiler = std::env::var("ZIL_CLANG").unwrap_or_else(|_| "clang".into());
    let wrapper = dir.join("fault-clang.py");
    fs::write(&wrapper, format!("#!/usr/bin/env python3\nimport os,sys\ncompiler={}\nextra=['-x','c',{},'-Wl,--wrap=malloc','-Wl,--wrap=realloc','-Wl,--wrap=free']\nos.execvp(compiler,[compiler]+sys.argv[1:]+extra)\n", json_string(&compiler), json_string(&local.to_string_lossy()))).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    wrapper
}
fn run(path: &Path, input: &[u8]) -> Output {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let record = path.with_file_name(format!(
        "execution-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(record.with_extension("command"), format!("{command:?}\n")).unwrap();
    fs::write(record.with_extension("stdin"), input).unwrap();
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let bytes = input.to_vec();
    let writer = thread::spawn(move || stdin.write_all(&bytes));
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || {
        let mut b = Vec::new();
        stdout.read_to_end(&mut b).unwrap();
        b
    });
    let err = thread::spawn(move || {
        let mut b = Vec::new();
        stderr.read_to_end(&mut b).unwrap();
        b
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("byte consumer timed out: {}", path.display());
        }
        thread::sleep(Duration::from_millis(2));
    };
    writer.join().unwrap().unwrap();
    let output = Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    };
    fs::write(record.with_extension("stdout"), &output.stdout).unwrap();
    fs::write(record.with_extension("stderr"), &output.stderr).unwrap();
    fs::write(
        record.with_extension("status"),
        format!("{:?}\n", output.status),
    )
    .unwrap();
    output
}
fn expect(text: &str, expected: &str, fault: bool) {
    for level in [Optimization::O0, Optimization::O2] {
        let temp = build(text, level, fault);
        let output = run(&temp.0.join("program"), &[]);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
}

const COUNTERS: &str = "extern \"C\" fn byte_test_attempts()->i32;extern \"C\" fn byte_test_releases()->i32;extern \"C\" fn byte_test_live()->i32;fn counts()->i32{unsafe{return byte_test_attempts()*100+byte_test_releases()*10000+byte_test_live()*1000000}}";
const BUFFER: &str = "fn buf(v:u8)->ByteBuffer contract{result(create)}{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,v);return give b}fn get(b:ByteBuffer)->i32{return i32(bytes_view(b)[0])}fn consume(give b:ByteBuffer)->i32{return get(b)}";
const PACKAGE: &str = "package P{manages left:ByteBuffer;manages right:ByteBuffer;n:i64}fn make()->P{return P{left:buf(2u8),right:buf(3u8),n:i64(9)}}fn relay(p:P)->P{return p}";

#[test]
fn package_transfer_take_refill_and_replace_release_each_payload_once() {
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}fn work()->i32{{var p=relay(make());let old=give p.left;p.left=buf(5u8);p.right=buf(7u8);return get(old)+get(p.left)+get(p.right)+i32(p.n)}}fn value()->i32{{let answer=work();return answer+counts()}}"
    );
    expect(&text, "40423\n", true);
}

#[test]
fn direct_and_package_owned_factories_keep_state_and_release_when_dropped() {
    let text = format!(
        "{COUNTERS}{BUFFER}fn factory()->impl FnMut()->i32{{let b=buf(2u8);return move()=>{{bytes_push(b,9u8);return i32(bytes_len(b))}}}}fn work()->i32{{var f=factory();return f()+f()}}fn value()->i32{{let answer=work();return answer+counts()}}"
    );
    expect(&text, "10105\n", true);
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}fn factory()->impl FnMut()->i32{{var p=make();return move()=>{{p.left=buf(5u8);return i32(bytes_len(p.left)+bytes_len(p.right))}}}}fn work()->i32{{var f=factory();return f()+f()}}fn value()->i32{{let answer=work();return answer+counts()}}"
    );
    expect(&text, "40404\n", true);
    // A never-called returned environment must discharge both fields as well.
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}fn factory()->impl Fn()->i32{{let p=make();return move()=>get(p.left)+get(p.right)}}fn work(){{let f=factory()}}fn value()->i32{{work();return counts()}}"
    );
    expect(&text, "20200\n", true);
}

#[test]
fn once_capture_consumption_and_partial_package_error_discharge_remaining_debt() {
    let text = format!(
        "{COUNTERS}{BUFFER}fn factory()->impl FnOnce()->i32{{let b=buf(7u8);return ()=>consume(give b)}}fn work()->i32{{let f=factory();return f()}}fn value()->i32{{let answer=work();return answer+counts()}}"
    );
    expect(&text, "10107\n", true);
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}enum Fault{{Stop}}fn fail()->Fault!i32{{return Fault.Stop}}fn factory()->impl FnOnce()->Fault!i32{{let p=make();return move()=>{{let old=give p.left;return fail()?+get(old)}}}}fn work(){{let f=factory();let answer=f()}}fn value()->i32{{work();return counts()}}"
    );
    expect(&text, "20200\n", true);
}

#[test]
fn ordinary_errors_and_failed_preparation_settle_only_acquired_responsibilities() {
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}enum Fault{{Stop}}fn fail()->Fault!i64{{return Fault.Stop}}fn work()->Fault!i64{{let p=P{{left:buf(2u8),right:buf(3u8),n:fail()?}};return p.n}}fn value()->i32{{let result=work();return counts()}}"
    );
    expect(&text, "20200\n", true);
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}enum Fault{{Stop}}fn fail()->Fault!i64{{return Fault.Stop}}fn accept(p:P,n:i64)->i64{{return n}}fn work()->Fault!i64{{return accept(make(),fail()?)}}fn value()->i32{{let result=work();return counts()}}"
    );
    expect(&text, "20200\n", true);
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}enum Fault{{Stop}}fn work(fail:bool)->Fault!i32{{let p=make();if fail{{return Fault.Stop}}return get(p.left)+get(p.right)}}fn value()->i32{{let first=work(false);let second=work(true);return counts()}}"
    );
    expect(&text, "40400\n", true);
}

#[test]
fn failed_growth_inside_saved_environment_preserves_payload_and_later_use() {
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}fn factory()->impl FnMut()->i32{{let p=make();return move()=>{{if bytes_reserve(p.left,13){{return -100}}return get(p.left)+get(p.right)+i32(bytes_capacity(p.left))}}}}fn work()->i32{{var f=factory();return f()+f()}}fn value()->i32{{let answer=work();return answer+counts()}}"
    );
    expect(&text, "20426\n", true);
}

#[test]
fn mutable_factory_recovers_after_an_error_before_successful_replacement() {
    let text = format!(
        "{COUNTERS}{BUFFER}{PACKAGE}enum Fault{{Stop}}fn ready(ok:bool)->Fault!i32{{if ok{{return 1}}return Fault.Stop}}fn factory()->impl FnMut(bool)->Fault!i32{{var p=make();return move(ok)=>{{let n=ready(ok)?;p.left=buf(5u8);return n+get(p.left)+get(p.right)}}}}fn work()->Fault!i32{{var f=factory();let failed=f(false);return f(true)?}}fn value()->i32{{let answer=work();return counts()}}"
    );
    expect(&text, "30300\n", true);
}

#[test]
fn four_module_saved_state_consumer_matches_rust_on_binary_batches() {
    let app = root().join("examples/mainline/byte_buffer_owned");
    let inputs = [
        vec![],
        b"ab\0cd\0\0tail".to_vec(),
        (0..=255).collect(),
        vec![73; 8193],
        (0..32769).map(|i| (i % 251) as u8).collect(),
    ];
    for level in [Optimization::O0, Optimization::O2] {
        let temp = Temp::new();
        let report = compile_path(
            &app.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(temp.0.join("zil")),
                emit_dir: Some(temp.0.join("phases")),
                optimization: level,
                ..CompileOptions::default()
            },
        );
        fs::write(temp.0.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert_eq!(report.source_files.len(), 4);
        let mut command = Command::new("rustc");
        command
            .arg(app.join("main.rs"))
            .arg("-C")
            .arg(if level == Optimization::O0 {
                "opt-level=0"
            } else {
                "opt-level=2"
            })
            .arg("-o")
            .arg(temp.0.join("rust"));
        let output = command.output().unwrap();
        fs::write(temp.0.join("rust-command"), format!("{command:?}\n")).unwrap();
        fs::write(temp.0.join("rust-stderr"), &output.stderr).unwrap();
        assert!(output.status.success(), "{output:?}");
        for input in &inputs {
            let zil = run(&temp.0.join("zil"), input);
            let rust = run(&temp.0.join("rust"), input);
            let mut sum = 0i64;
            let mut length = 0i64;
            let expected: Vec<u8> = input
                .iter()
                .map(|&byte| {
                    if byte == 0 {
                        sum = 0;
                        length = 0;
                    } else {
                        sum += i64::from(byte);
                        length += 1;
                    }
                    ((sum + length) % 251) as u8
                })
                .collect();
            assert!(zil.status.success(), "{zil:?}");
            assert!(rust.status.success(), "{rust:?}");
            assert_eq!(zil.stdout, expected);
            assert_eq!(zil.stdout, rust.stdout);
            assert!(zil.stderr.is_empty());
            assert!(rust.stderr.is_empty());
        }
    }
}

// Compact source builds the same 16KiB Copy field as [i32;4096], without
// spending the independent verifier's default budget on 4096 literal operands.
fn large_owned_program(body: &str) -> String {
    let mut ty = "i32".to_owned();
    let mut init = "let a0=seed;".to_owned();
    for level in 1..=12 {
        ty = format!("[{ty};2]");
        init.push_str(&format!("let a{level}=[a{},a{}];", level - 1, level - 1));
    }
    format!(
        "{COUNTERS}{BUFFER}package Large{{manages bytes:ByteBuffer;values:{ty}}}fn make(seed:i32)->Large{{{init}return Large{{bytes:buf(u8(seed)),values:a12}}}}fn relay(p:Large)->Large{{return p}}{body}"
    )
}
fn expect_large_owned(text: &str, expected: &str) {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for level in [Optimization::O0, Optimization::O2] {
            let temp = build_backend(text, level, true, backend);
            let llvm = fs::read_to_string(temp.0.join("phases/llvm.ll")).unwrap();
            assert!(llvm.contains(".lc1("));
            assert!(llvm.contains("@zil.copy.bytes"));
            if text.contains("f=move(") {
                let entries = llvm
                    .lines()
                    .filter(|line| {
                        line.starts_with("define ")
                            && line.contains("%env")
                            && line.contains(".lc1(")
                    })
                    .collect::<Vec<_>>();
                assert_eq!(entries.len(), 1, "{llvm}");
                assert!(
                    entries[0].contains("ptr %arg.") || entries[0].contains("ptr %lc1.result"),
                    "{}",
                    entries[0]
                );
                if let Some((before_env, _)) = entries[0].split_once("%env") {
                    assert!(
                        !before_env.contains("%lc1.result"),
                        "environment must precede output: {}",
                        entries[0]
                    );
                }
            }
            assert!(!llvm.contains("insertvalue { { ptr, i64, i64 }, [2 x"));
            // Every scratch/value slot is emitted in the entry block. Repeated
            // construction/moves inside a loop must never grow its stack.
            for function in llvm.split("define ").skip(1) {
                if let Some((_, blocks)) = function.split_once("bb.") {
                    assert!(!blocks.contains("alloca "), "{function}");
                }
            }
            let output = run(&temp.0.join("program"), &[]);
            assert!(output.status.success(), "{backend:?}/{level:?}: {output:?}");
            assert_eq!(output.stdout, expected.as_bytes());
            assert!(output.stderr.is_empty());
        }
    }
}
#[test]
fn large_owned_package_move_call_return_fields_and_cleanup() {
    let first = "[0]".repeat(12);
    let text = large_owned_program(&format!(
        "fn work()->i32{{var p=relay(make(3));let saved=p.values;let old=give p.bytes;p.bytes=buf(7u8);let donor=make(9);p.values=donor.values;let q=relay(p);let a=q.values;let b=donor.values;return saved{first}*1000+a{first}*100+b{first}*10+get(old)+get(q.bytes)+get(donor.bytes)}}fn value()->i32{{let answer=work();return answer+counts()}}"
    ));
    expect_large_owned(&text, "34309\n");
}
#[test]
fn large_owned_package_failed_allocation_preserves_payload_and_single_cleanup() {
    let first = "[0]".repeat(12);
    let text = large_owned_program(&format!(
        "fn failed(p:Large)->i32{{if bytes_reserve(p.bytes,13){{return -1}}let a=p.values;return a{first}+i32(bytes_len(p.bytes)+bytes_capacity(p.bytes))}}fn value()->i32{{let answer=failed(relay(make(3)));return answer+counts()}}"
    ));
    expect_large_owned(&text, "10212\n");
}
#[test]
fn large_owned_package_loop_reuses_fixed_stack_slots() {
    let first = "[0]".repeat(12);
    let text = large_owned_program(&format!(
        "fn work()->i32{{var n=0;var answer=0;while n<1024{{let p=relay(make(3));let a=p.values;answer+=a{first}+get(p.bytes);n+=1}}return answer}}fn value()->i32{{let answer=work();return answer+counts()}}"
    ));
    expect_large_owned(&text, "10348544\n");
}
#[test]
fn large_owned_package_snapshot_does_not_grant_copy_or_end_live_borrows() {
    use zil_base::Phase;
    for (body, phase, code) in [
        (
            "fn value()->i32{let p=make(3);let a=relay(p);let b=relay(p);return 0}",
            Phase::Analyze,
            "E-UNINITIALIZED",
        ),
        (
            "fn value()->i32{let p=make(3);let borrowed=&p.values;let moved=relay(p);return (*borrowed)[0][0][0][0][0][0][0][0][0][0][0][0]}",
            Phase::Verify,
            "E-BORROW-CONFLICT",
        ),
    ] {
        let report = compile_source(
            &SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "large_owned.zil",
                large_owned_program(body),
            ),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn large_owned_package_codegen_budget_is_exact_and_request_local() {
    use zil_base::{Limits, Phase, TargetProfile};
    let src = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "large_owned.zil",
        large_owned_program("fn value()->i32{let p=relay(make(3));return i32(bytes_len(p.bytes))}"),
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    let errors = emit(low - 1).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].phase, Phase::Codegen);
    let exact = emit(low).unwrap();
    assert!(exact.contains(".lc1("));
    assert_eq!(exact, emit(low).unwrap());
    assert_eq!(exact, emit(defaults.max_analysis_steps).unwrap());
    eprintln!("large owned Package exact Codegen budget: {low}");
}

#[test]
fn large_owned_package_child_take_refill_and_multiple_live_arguments() {
    let first = "[0]".repeat(12);
    let text = large_owned_program(&format!(
        "package Outer{{child:Large;mark:i32}}fn wrap(p:Large)->Outer{{return Outer{{child:p,mark:1}}}}fn outer_relay(p:Outer)->Outer{{return p}}fn unwrap(p:Outer)->Large{{let child=p.child;return child}}fn select(p:Large,q:Large)->Large{{if bytes_len(q.bytes)!=1{{return q}}return p}}fn work()->i32{{var outer=outer_relay(wrap(make(3)));let child=outer.child;outer.child=make(7);let returned=unwrap(outer_relay(outer));let a=child.values;let b=returned.values;let selected=select(child,returned);return a{first}*10+b{first}+get(selected.bytes)}}fn value()->i32{{let answer=work();return answer+counts()}}"
    ));
    expect_large_owned(&text, "20240\n");
}

#[test]
fn large_owned_package_raw_and_wire_reject_duplicated_move_arguments() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::InstructionKind;
    let src = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "large_owned.zil",
        large_owned_program(
            "fn value()->i32{let p=relay(make(3));let q=relay(make(7));return i32(bytes_len(p.bytes)+bytes_len(q.bytes))}",
        ),
    );
    let limits = Limits::default();
    let target = TargetProfile::default();
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let verified =
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    assert!(
        zil_backend_llvm::emit(&verified, &lir)
            .unwrap()
            .contains(".lc1(")
    );
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let relay = typed
        .functions
        .iter()
        .find(|f| f.name == "relay")
        .unwrap()
        .id;
    let mut raw = typed;
    let caller = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "value")
        .unwrap();
    let mut previous = None;
    let mut changed = false;
    for instruction in caller.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::Call {
            function,
            arguments,
        } = &mut instruction.kind
            && *function == relay
        {
            if let Some(first) = previous {
                arguments[0] = first;
                changed = true;
                break;
            }
            previous = Some(arguments[0]);
        }
    }
    assert!(changed);
    // Preserve the producer's actual claims, but bind them to the mutated
    // module so Verify/Wire must inspect the duplicated ownership transfer.
    assert_ne!(plan.module_fingerprint, raw.fingerprint());
    let mut claim = plan;
    claim.module_fingerprint = raw.fingerprint();
    let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert_eq!(analyzed[0].category, ResultCategory::SafeReject);
    assert_eq!(analyzed[0].phase, zil_base::Phase::Analyze);
    assert_eq!(analyzed[0].code, "E-ANALYZE-IR");
    assert_eq!(
        analyzed[0].message,
        "resource closure: SSA responsibility was already consumed or absent"
    );
    let verified = zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits)
        .err()
        .unwrap();
    assert_eq!(verified[0].category, ResultCategory::SafeReject);
    assert_eq!(verified[0].phase, zil_base::Phase::Verify);
    assert_eq!(verified[0].code, "E-VERIFY-RESOURCE");
    assert_eq!(
        verified[0].message,
        "managed SSA value was not available exactly once"
    );
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    let wire = zil_verify::verify_module_bytes(&bytes, &target, &limits)
        .err()
        .unwrap();
    assert_eq!(wire[0].category, ResultCategory::SafeReject);
    assert_eq!(wire[0].phase, zil_base::Phase::Verify);
    assert_eq!(wire[0].code, "E-VERIFY-RESOURCE");
    assert_eq!(
        wire[0].message,
        "managed SSA value was not available exactly once"
    );
}

// A shallow 16^3 field keeps this ABI probe independent of recursive frontend
// stack depth while retaining a real 16KiB owned argument/result.
fn large_closure_program(body: &str) -> String {
    let mut ty = "i32".to_owned();
    let mut init = "let a0=seed;".to_owned();
    for level in 1..=3 {
        ty = format!("[{ty};16]");
        let fields = vec![format!("a{}", level - 1); 16].join(",");
        init.push_str(&format!("let a{level}=[{fields}];"));
    }
    format!(
        "{COUNTERS}{BUFFER}package Large{{manages bytes:ByteBuffer;values:{ty}}}fn make(seed:i32)->Large{{{init}return Large{{bytes:buf(u8(seed)),values:a3}}}}{body}"
    )
}

#[test]
fn large_owned_closure_parameter_result_and_roundtrip_use_real_environments() {
    let first = "[0]".repeat(3);
    for (work, expected) in [
        (
            format!("fn work()->i32{{let bias=7;let f=move(p:Large)->i32=>{{let a=p.values;return a{first}+get(p.bytes)+bias}};return f(make(3))}}"),
            "10113\n",
        ),
        (
            "fn work()->i32{let bias=2;let f=move(seed:i32)->Large=>make(seed+bias);let p=f(3);let a=p.values;return get(p.bytes)+a[0][0][0]}".into(),
            "10110\n",
        ),
        (
            "fn work()->i32{let bias=2;let f=move(p:Large)->Large=>{var q=p;q.bytes=buf(u8(bias));return q};let p=f(make(3));let a=p.values;return get(p.bytes)+a[0][0][0]}".into(),
            "20205\n",
        ),
    ] {
        let text = large_closure_program(&format!("{work}fn value()->i32{{let answer=work();return answer+counts()}}"));
        expect_large_owned(&text, expected);
    }
}

#[test]
fn large_owned_mutable_closure_borrowed_calls_reuse_fixed_stack_slots() {
    let first = "[0]".repeat(3);
    let text = large_closure_program(&format!(
        "fn work()->i32{{var calls=0;var f=move(p:Large)->Large=>{{calls+=1;bytes_push(p.bytes,u8(calls%200));return p}};var n=0;var answer=0;while n<1024{{let borrowed=&mut f;let p=borrowed(make(3));let a=p.values;answer+=a{first}+get(p.bytes)+i32(bytes_len(p.bytes));n+=1}}return answer}}fn value()->i32{{let answer=work();return answer+counts()}}"
    ));
    expect_large_owned(&text, "10350592\n");
}

#[test]
fn large_owned_once_closure_result_and_error_clean_each_owner_once() {
    let first = "[0]".repeat(3);
    for (body, expected) in [
        (
            format!("fn work()->i32{{let b=buf(7u8);let f=move(p:Large)->Large=>{{var q=p;let old=give q.bytes;q.bytes=give b;return q}};let p=f(make(3));let a=p.values;return a{first}+get(p.bytes)}}"),
            "20210\n",
        ),
        (
            "enum Fault{Stop}fn fail()->Fault!i32{return Fault.Stop}fn work(){let b=buf(7u8);let f=move(p:Large)->Fault!i32=>{let old=give p.bytes;let captured=give b;return fail()?+get(old)+get(captured)};let result=f(make(3))}".into(),
            "20200\n",
        ),
        (
            format!("fn work()->i32{{let bias=1;let f=move(p:Large)->i32=>{{if bytes_reserve(p.bytes,13){{return -1}}let a=p.values;return a{first}+i32(bytes_len(p.bytes)+bytes_capacity(p.bytes))+bias}};return f(make(3))}}"),
            "10213\n",
        ),
    ] {
        let value = if expected == "20200\n" {
            "fn value()->i32{work();return counts()}"
        } else {
            "fn value()->i32{let answer=work();return answer+counts()}"
        };
        expect_large_owned(&large_closure_program(&format!("{body}{value}")), expected);
    }
}

#[test]
fn large_owned_closure_factories_and_generic_helpers_preserve_responsibility() {
    let first = "[0]".repeat(3);
    // The first three sources retain the initial failing probes byte for byte.
    // Their fingerprints preserve the original target bytes as acceptance consumers.
    for (label, body, fingerprint, expected) in [
        (
            "opaque-parameter",
            format!("fn factory()->impl Fn(Large)->i32{{let bias=7;return move(p)=>{{let a=p.values;return a{first}+get(p.bytes)+bias}}}}fn work()->i32{{let f=factory();return f(make(3))}}fn value()->i32{{let answer=work();return answer+counts()}}"),
            Some("0f38d27d2c7361bafec7b9713a4a14aad2a9e2efa7f86c6c171fb6a5cd804e07"),
            "10113\n",
        ),
        (
            "opaque-and-generic",
            format!("fn factory()->impl FnMut(Large)->Large{{var calls=0;return move(p)=>{{calls+=1;bytes_push(p.bytes,u8(calls%200));return p}}}}fn invoke<F:FnMut(Large)->Large>(f:&mut F,p:Large)->Large{{return f(p)}}fn work()->i32{{var f=factory();var n=0;var answer=0;while n<1024{{let p=invoke(&mut f,make(3));let a=p.values;answer+=a{first}+get(p.bytes)+i32(bytes_len(p.bytes));n+=1}}return answer}}fn value()->i32{{let answer=work();return answer+counts()}}"),
            Some("2bd33276f522f6808e74772cf1f66ff8ea11fc33f6be174eaff9ba7f3c289021"),
            "10350592\n",
        ),
        (
            "opaque-once",
            format!("fn factory()->impl FnOnce(Large)->Large{{let b=buf(7u8);return move(p)=>{{var q=p;let old=give q.bytes;q.bytes=give b;return q}}}}fn work()->i32{{let f=factory();let p=f(make(3));let a=p.values;return a{first}+get(p.bytes)}}fn value()->i32{{let answer=work();return answer+counts()}}"),
            Some("2bed2c26198efa2fb4e6f88de8761effe65edeea4463e3efe3d6403ec0c57540"),
            "20210\n",
        ),
        (
            "opaque-result",
            "fn factory()->impl Fn(i32)->Large{let bias=2;return move(seed)=>make(seed+bias)}fn work()->i32{let f=factory();let p=f(3);let a=p.values;return a[0][0][0]+get(p.bytes)}fn value()->i32{let answer=work();return answer+counts()}".into(),
            None,
            "10110\n",
        ),
        (
            "generic-borrowed-parameter",
            "fn invoke<F:FnMut(Large)->Large>(f:&mut F,p:Large)->Large{return f(p)}fn value()->i32{return 0}".into(),
            None,
            "0\n",
        ),
    ] {
        let temp = Temp::new();
        let text = large_closure_program(&body);
        fs::write(temp.0.join(format!("{label}.zil")), &text).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "owned_byte_composition_owned_bytes_native.zil", text.clone()),
            &CompileOptions { stop_after: Stage::Verify, ..CompileOptions::default() },
        );
        fs::write(temp.0.join("compile.json"), report.to_json()).unwrap();
        if let Some(expected) = fingerprint {
            assert_eq!(report.input_fingerprint.to_hex(), expected, "{label}");
        }
        assert_eq!(report.status, ResultCategory::Success, "{label}: {}", report.to_json());
        assert_eq!(report.verification_runs, 1, "{label}");
        expect_large_owned(&text, expected);
    }
}

#[test]
fn small_ordinary_closure_keeps_environment_and_scalar_calling_convention() {
    let text = format!(
        "{COUNTERS}{BUFFER}fn work()->i32{{let b=buf(9u8);let bias=7;let f=move(n:i32)->i32=>n+bias;return f(3)+get(b)-9}}fn value()->i32{{let answer=work();return answer+counts()}}"
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for level in [Optimization::O0, Optimization::O2] {
            let temp = build_backend(&text, level, true, backend);
            let llvm = fs::read_to_string(temp.0.join("phases/llvm.ll")).unwrap();
            let entries = llvm
                .lines()
                .filter(|line| {
                    line.starts_with("define ")
                        && line.contains("@zil.fn.")
                        && line.contains("%env")
                })
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            assert!(!entries[0].contains(".lc1("), "{}", entries[0]);
            assert!(entries[0].contains("i32 %arg.0"), "{}", entries[0]);
            assert!(!llvm.contains("%lc1.result"));
            let output = run(&temp.0.join("program"), &[]);
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, b"10110\n");
            assert!(output.stderr.is_empty());
        }
    }
}

#[test]
fn large_owned_closure_codegen_budget_is_exact_and_request_local() {
    use zil_base::{Limits, Phase, TargetProfile};
    let temp = Temp::new();
    let text = large_closure_program(
        "fn value()->i32{let bias=2;let f=move(p:Large)->Large=>{var q=p;q.bytes=buf(u8(bias));return q};let p=f(make(3));return get(p.bytes)}",
    );
    fs::write(temp.0.join("source.zil"), &text).unwrap();
    let src = SourceUnit::new(FileId(0), ModuleId(0), "large_owned_closure.zil", text);
    let defaults = Limits::default();
    let target = TargetProfile::default();
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &defaults).unwrap();
    let core = zil_ct::evaluate_source_constants(&core, &target, &defaults).unwrap();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    let default_ir = emit(high).unwrap();
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    let errors = emit(low - 1).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].phase, Phase::Codegen);
    let exact = emit(low).unwrap();
    assert!(exact.lines().any(|line| line.starts_with("define ")
        && line.contains(".lc1(")
        && line.contains("%env, ptr %lc1.result, ptr %arg.0")));
    assert_eq!(exact, emit(low).unwrap());
    assert_eq!(exact, default_ir);
    fs::write(temp.0.join("exact.ll"), exact).unwrap();
    fs::write(
        temp.0.join("budget.txt"),
        format!(
            "source={}\nexact={low}\nbelow={}\nfailure={errors:#?}\n",
            src.fingerprint().to_hex(),
            low - 1
        ),
    )
    .unwrap();
    eprintln!("large owned Closure exact Codegen budget: {low}");
}

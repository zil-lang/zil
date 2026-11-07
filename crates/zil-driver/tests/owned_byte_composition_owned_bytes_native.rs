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
use zil_driver::{CompileOptions, Optimization, Stage, compile_path, compile_source};

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
            "zil-owned099-{}-{}",
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
    let temp = Temp::new();
    fs::write(temp.0.join("source.zil"), text).unwrap();
    let clang = fault.then(|| fault_clang(&temp.0));
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "owned099.zil", text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("value".into()),
            output: Some(temp.0.join("program")),
            emit_dir: Some(temp.0.join("phases")),
            optimization: level,
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
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("{ ptr, i64, i64 }"));
    assert!(!llvm.contains(" noalias "));
    temp
}
fn fault_clang(dir: &Path) -> PathBuf {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let source = root().join("tools/byte-buffer/096/fault-allocator.c");
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

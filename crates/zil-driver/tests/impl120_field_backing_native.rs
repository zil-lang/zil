//! Independent owning fields through real LLVM, allocator and processes.
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
            "zil-field120-{}-{}",
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
        &SourceUnit::new(FileId(0), ModuleId(0), "field120.zil", text),
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
const BUFFER: &str = "fn buf(v:u8)->ByteBuffer contract{result(create)}{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,v);return give b}fn get(b:ByteBuffer)->i32{return i32(bytes_view(b)[0])}";
const PACKAGE: &str = "package P{manages b:ByteBuffer;manages c:ByteBuffer}fn make()->P{return P{b:buf(2u8),c:buf(3u8)}}";

#[test]
fn live_sibling_survives_take_refill_replace_and_growth_with_exact_cleanup() {
    for (body, answer) in [
        (
            "fn work()->i32{var p=make();let s=bytes_view(p.b);let old=give p.c;p.c=buf(5u8);bytes_push(p.c,7u8);return i32(s[0])+get(old)+i32(bytes_len(p.c))}fn value()->i32{let answer=work();return answer+counts()}",
            "30307\n",
        ),
        (
            "fn work()->i32{var p=make();let s=bytes_view(p.b);p.c=buf(5u8);return i32(s[0])+get(p.c)}fn value()->i32{let answer=work();return answer+counts()}",
            "30307\n",
        ),
        (
            "fn work()->i32{let p=make();let s=bytes_view(p.b);if bytes_reserve(p.c,13){return -100}return i32(s[0])+get(p.c)}fn value()->i32{let answer=work();return answer+counts()}",
            "20305\n",
        ),
    ] {
        expect(&format!("{COUNTERS}{BUFFER}{PACKAGE}{body}"), answer, true);
    }
}

#[test]
fn disjoint_exclusive_fields_and_helper_apart_are_real_native_accesses() {
    for (body, answer) in [
        (
            "fn work()->i32{let p=make();let s=bytes_view_mut(p.b);let t=bytes_view_mut(p.c);s[0]=9u8;t[0]=11u8;return i32(s[0])+i32(t[0])}fn value()->i32{let answer=work();return answer+counts()}",
            "20220\n",
        ),
        (
            "fn act(a:ByteBuffer,b:ByteBuffer)->i32{let s=bytes_view(a);bytes_push(b,8u8);return i32(s[0])+i32(bytes_len(b))}fn work()->i32{let p=make();return act(p.b,p.c)}fn value()->i32{let answer=work();return answer+counts()}",
            "20204\n",
        ),
    ] {
        expect(&format!("{COUNTERS}{BUFFER}{PACKAGE}{body}"), answer, true);
    }
}

#[test]
fn owned_fnmut_and_partial_fnonce_error_release_actual_fields() {
    for (body, answer) in [
        (
            "fn factory()->impl FnMut()->i32{var p=make();return move()=>{let s=bytes_view(p.b);p.c=buf(5u8);return i32(s[0])+get(p.c)}}fn work()->i32{var f=factory();return f()+f()}fn value()->i32{let answer=work();return answer+counts()}",
            "40414\n",
        ),
        (
            "enum Fault{Stop}fn fail()->Fault!i32{return Fault.Stop}fn factory()->impl FnOnce()->Fault!i32{let p=make();return move()=>{let s=bytes_view(p.b);let old=give p.c;let n=i32(s[0]);return fail()?+n+get(old)}}fn work(){let f=factory();let answer=f()}fn value()->i32{work();return counts()}",
            "20200\n",
        ),
    ] {
        expect(&format!("{COUNTERS}{BUFFER}{PACKAGE}{body}"), answer, true);
    }
}

#[test]
fn four_modules_keep_helper_returns_and_owned_field_state() {
    for level in [Optimization::O0, Optimization::O2] {
        let temp = Temp::new();
        for (name, text) in [
            (
                "main.zil",
                "import owner;fn value()->i32{var f=owner.make();return f()+f()}",
            ),
            (
                "owner.zil",
                "import buffer;import views;package P{manages b:ByteBuffer;manages c:ByteBuffer}pub fn make()->impl FnMut()->i32{var p=P{b:buffer.make(2u8),c:buffer.make(3u8)};return move()=>{let s=views.view(p.b);p.c=buffer.make(5u8);return i32(s[0])+views.read(p.c)}}",
            ),
            (
                "buffer.zil",
                "pub fn make(v:u8)->ByteBuffer contract{result(create)}{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,v);return give b}",
            ),
            (
                "views.zil",
                "pub fn view(b:ByteBuffer)->&[u8]{return bytes_view(b)}pub fn read(b:ByteBuffer)->i32{return i32(bytes_view(b)[0])}",
            ),
        ] {
            fs::write(temp.0.join(name), text).unwrap();
        }
        let report = compile_path(
            &temp.0.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Build,
                entry: Some("value".into()),
                output: Some(temp.0.join("program")),
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
        let output = run(&temp.0.join("program"), &[]);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"14\n");
        assert!(output.stderr.is_empty());
    }
}

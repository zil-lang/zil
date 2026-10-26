//! Real ZIL source through the full driver, LLVM, linker and native process.
//! The explicit C file is only a fault-injection allocator, never a UI host or
//! an implementation of the source consumer. Rust is an output oracle only.
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
            "zil-bytes096-{}-{}",
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
        &SourceUnit::new(FileId(0), ModuleId(0), "bytes096.zil", text),
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

#[test]
fn empty_growth_initialized_prefix_and_mutable_views_are_real_storage() {
    expect(
        "fn value()->i64{let b=bytes_new();if bytes_push(b,9u8){return -1}if bytes_reserve(b,-1){return -2}if !bytes_reserve(b,0){return -3}if bytes_view(b).len()!=0{return -4}if !bytes_reserve(b,3){return -5}bytes_push(b,10u8);bytes_push(b,20u8);bytes_push(b,30u8);if bytes_push(b,99u8){return -6}let p=bytes_view_mut(b);p[1]=7u8;let q=&p[1..];let sum=i64(q[0])+i64(q[1]);return sum+bytes_len(b)+bytes_capacity(b)}",
        "43\n",
        false,
    );
}

#[test]
fn factories_give_returns_and_parameter_origin_views_compose() {
    expect(
        "fn make()->ByteBuffer contract{result(create)}{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,31u8);return give b}fn relay(give b:ByteBuffer)->ByteBuffer contract{result(give)}{return give b}fn view(b:ByteBuffer)->&[u8]{return bytes_view(b)}fn edit(b:ByteBuffer)->&mut[u8]{return bytes_view_mut(b)}fn value()->i64{let a=make();let b=relay(give a);let p=edit(b);p[0]=39u8;return i64(view(b)[0])+bytes_capacity(b)}",
        "47\n",
        false,
    );
}

#[test]
fn ordinary_generic_algorithms_apply_to_dynamic_and_fixed_windows() {
    expect(
        "fn tail<T>(p:&[T],i:i64)->&[T]{return &p[i..]}fn put<T>(p:&mut[T],i:i64,v:T){p[i]=v}fn value()->i64{let b=bytes_new();bytes_reserve(b,4);bytes_push(b,7u8);bytes_push(b,9u8);put(bytes_view_mut(b),1,5u8);var fixed=[1u8,2u8];put(&mut fixed[..],0,8u8);return i64(tail(bytes_view(b),1)[0])+i64(tail(&fixed[..],0)[0])}",
        "13\n",
        false,
    );
}

#[test]
#[cfg(target_os = "linux")]
fn capacity_does_not_expose_uninitialized_bytes_and_dynamic_bounds_trap() {
    use std::os::unix::process::ExitStatusExt;
    for text in [
        "fn value()->i64{let b=bytes_new();return i64(bytes_view(b)[0])}",
        "fn value()->i64{let b=bytes_new();if !bytes_reserve(b,8){return -1}return i64(bytes_view(b)[0])}",
        "fn value()->i64{let b=bytes_new();if !bytes_reserve(b,8){return -1}bytes_push(b,7u8);return i64(bytes_view(b)[1])}",
        "fn value()->i64{let b=bytes_new();if !bytes_reserve(b,8){return -1}bytes_push(b,7u8);let p=bytes_view(b);return (&p[..2]).len()}",
    ] {
        for level in [Optimization::O0, Optimization::O2] {
            let temp = build(text, level, false);
            let output = run(&temp.0.join("program"), &[]);
            // x64 LLVM trap is SIGILL. A parser rejection, linker failure,
            // segmentation fault or arbitrary nonzero exit is not this proof.
            assert_eq!(output.status.signal(), Some(4), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}

const COUNTERS: &str = "extern \"C\" fn byte_test_attempts()->i32;extern \"C\" fn byte_test_releases()->i32;extern \"C\" fn byte_test_live()->i32;fn counts()->i32{unsafe{return byte_test_attempts()*100+byte_test_releases()*10000+byte_test_live()*1000000}}";

#[test]
fn injected_allocation_failure_keeps_old_bytes_and_releases_exactly_once() {
    let text = format!(
        "{COUNTERS}fn work()->i32{{let b=bytes_new();if !bytes_reserve(b,4){{return -1}}bytes_push(b,65u8);bytes_push(b,66u8);if bytes_reserve(b,13){{return -2}}let p=bytes_view(b);return i32(p[0])+i32(p[1])+i32(bytes_len(b))+i32(bytes_capacity(b))}}fn value()->i32{{let answer=work();return answer+counts()}}"
    );
    expect(&text, "10337\n", true);
    let text = format!(
        "{COUNTERS}fn work()->i32{{let b=bytes_new();if bytes_reserve(b,13){{return -1}}return i32(bytes_len(b)+bytes_capacity(b))}}fn value()->i32{{let answer=work();return answer+counts()}}"
    );
    expect(&text, "100\n", true);
}

#[test]
fn ordinary_error_exits_and_failed_later_arguments_settle_temporary_owners() {
    let text = format!(
        "{COUNTERS}enum Fault{{Stop}}fn work(fail:bool)->Fault!i32{{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,9u8);if fail{{return Fault.Stop}}return 1}}fn value()->i32{{let a=work(false);let b=work(true);return counts()}}"
    );
    expect(&text, "20200\n", true);
    let text = format!(
        "{COUNTERS}enum Fault{{Stop}}fn make()->ByteBuffer contract{{result(create)}}{{let b=bytes_new();bytes_reserve(b,8);return give b}}fn capacity()->Fault!i64{{return Fault.Stop}}fn work()->Fault!bool{{return bytes_reserve(make(),capacity()?)}}fn value()->i32{{let x=work();return counts()}}"
    );
    expect(&text, "10100\n", true);
}

#[test]
fn dynamic_multimodule_byte_consumer_matches_rust_on_binary_inputs() {
    let app = root().join("examples/mainline/byte_buffer");
    let inputs = [
        vec![],
        b"aBc xyz!\n".to_vec(),
        (0..=255).collect(),
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
            assert!(zil.status.success(), "{zil:?}");
            assert!(rust.status.success(), "{rust:?}");
            assert_eq!(zil.stdout, rust.stdout);
            assert_eq!(
                zil.stdout,
                input.iter().map(u8::to_ascii_uppercase).collect::<Vec<_>>()
            );
            assert!(zil.stderr.is_empty());
        }
    }
}

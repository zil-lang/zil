//! Ordinary public ZIL mapping, explicit writable windows and real native output.
//! The byte oracle uses Rust's value operation; it does not grant ZIL permissions.
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
use zil_base::{Phase, ResultCategory};
use zil_driver::{CompileOptions, Optimization, Stage, compile_path};

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
            "ascii109-{}-{}",
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
fn project(main: &str) -> Temp {
    let temp = Temp::new();
    fs::write(temp.0.join("main.zil"), main).unwrap();
    fs::copy(
        root().join("examples/mainline/byte_buffer/sequence.zil"),
        temp.0.join("sequence.zil"),
    )
    .unwrap();
    fs::copy(
        root().join("benches/rust-parity/109/original/buffer/zil/io.zil"),
        temp.0.join("io.zil"),
    )
    .unwrap();
    temp
}
fn build(temp: &Temp, level: Optimization) {
    let report = compile_path(
        &temp.0.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Build,
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
}
fn run(temp: &Temp, input: &[u8]) -> Output {
    let mut command = Command::new(temp.0.join("program"));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let record = temp.0.join(format!(
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
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            fs::write(record.with_extension("status"), "timeout\n").unwrap();
            panic!("ASCII consumer timed out: {}", temp.0.display());
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
fn expect(temp: &Temp, input: &[u8], expected: &[u8]) {
    let output = run(temp, input);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn all_byte_values_keep_the_checked_subtraction_in_its_branch() {
    let text = "import sequence\nimport io\nfn main()->i32{var code:i64=0;while code<256{io.write_byte(sequence.ascii_upper(u8(code)));code+=1}return 0}";
    let expected = (0..=255u8)
        .map(|b| b.to_ascii_uppercase())
        .collect::<Vec<_>>();
    for level in [Optimization::O0, Optimization::O2] {
        let temp = project(text);
        build(&temp, level);
        expect(&temp, &[], &expected);
    }
}

#[test]
fn empty_and_interior_mutable_windows_leave_outside_bytes_unchanged() {
    let mut original = vec![b'z'];
    original.extend(0..=255u8);
    original.push(b'z');
    let items = original
        .iter()
        .map(|b| format!("{b}u8"))
        .collect::<Vec<_>>()
        .join(",");
    let text = format!(
        "import sequence\nimport io\nfn main()->i32{{var items=[{items}];sequence.uppercase(&mut items[0..0]);sequence.uppercase(&mut items[1..257]);sequence.uppercase(&mut items[258..258]);var index:i64=0;while index<258{{io.write_byte(items[index]);index+=1}}return 0}}"
    );
    let mut expected = original;
    expected[1..257].make_ascii_uppercase();
    for level in [Optimization::O0, Optimization::O2] {
        let temp = project(&text);
        build(&temp, level);
        expect(&temp, &[], &expected);
    }
}

#[test]
fn mapped_multimodule_consumer_preserves_binary_io_and_vector_tails() {
    let source = root().join("benches/rust-parity/109/mapped/buffer/zil");
    assert_eq!(
        fs::read(source.join("sequence.zil")).unwrap(),
        fs::read(root().join("examples/mainline/byte_buffer/sequence.zil")).unwrap()
    );
    for level in [Optimization::O0, Optimization::O2] {
        let temp = Temp::new();
        for name in ["main.zil", "buffer.zil", "io.zil", "sequence.zil"] {
            fs::copy(source.join(name), temp.0.join(name)).unwrap();
        }
        build(&temp, level);
        for length in [0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 255, 256, 257, 272] {
            let input = (0..length).map(|i| (i % 256) as u8).collect::<Vec<_>>();
            let expected = input.iter().map(u8::to_ascii_uppercase).collect::<Vec<_>>();
            expect(&temp, &input, &expected);
        }
    }
}

#[test]
fn shared_arguments_and_live_shared_loans_do_not_gain_write_permission() {
    for (text, phase, code) in [
        (
            "import sequence\nfn main()->i32{var items=[97u8];sequence.uppercase(&items[..]);return 0}",
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "import sequence\nfn main()->i32{var items=[97u8];let shared=&items[..];sequence.uppercase(&mut items[..]);return i32(shared[0])}",
            Phase::Verify,
            "E-BORROW-CONFLICT",
        ),
    ] {
        let temp = project(text);
        let report = compile_path(
            &temp.0.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(temp.0.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(temp.0.join("compile.json"), report.to_json()).unwrap();
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
                .any(|d| d.phase == phase && d.code == code),
            "{}",
            report.to_json()
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn an_invalid_initialized_window_still_traps_before_mapping() {
    use std::os::unix::process::ExitStatusExt;
    let text = "import sequence\nfn main()->i32{let b=bytes_new();if !bytes_reserve(b,4){return 2}bytes_push(b,97u8);let items=bytes_view_mut(b);sequence.uppercase(&mut items[..2]);return 0}";
    for level in [Optimization::O0, Optimization::O2] {
        let temp = project(text);
        build(&temp, level);
        let output = run(&temp, &[]);
        assert_eq!(output.status.signal(), Some(4), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
    }
}

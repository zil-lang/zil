//! Source/native evidence for the bounded byte/foreign-buffer profile.
//! Rust comparisons establish behavior; no runtime or compilation speed claim.
use std::os::unix::process::ExitStatusExt;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_path, compile_source};

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
            "zil-bytes083-{}-{}",
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
fn build(command: &mut Command, record: &Path) {
    let output = command.output().unwrap();
    fs::write(record.with_extension("command"), format!("{command:?}\n")).unwrap();
    fs::write(record.with_extension("stdout"), &output.stdout).unwrap();
    fs::write(record.with_extension("stderr"), &output.stderr).unwrap();
    fs::write(
        record.with_extension("exit"),
        format!("{:?}\n", output.status.code()),
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn run(path: &Path, input: &[u8], mode: Option<&str>) -> Output {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(mode) = mode {
        command.env("ZIL_SHORT_WRITE_MODE", mode);
    }
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("byte application timed out: {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let output = child.wait_with_output().unwrap();
    let record = path.with_file_name(format!(
        "run-{}-{}",
        path.file_name().unwrap().to_string_lossy(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(record.with_extension("command"), format!("{command:?}\n")).unwrap();
    fs::write(record.with_extension("stdin"), input).unwrap();
    fs::write(record.with_extension("stdout"), &output.stdout).unwrap();
    fs::write(record.with_extension("stderr"), &output.stderr).unwrap();
    fs::write(
        record.with_extension("exit"),
        format!("{:?}\n", output.status),
    )
    .unwrap();
    output
}
fn succeeds(path: &Path, input: &[u8], expected: &[u8], mode: Option<&str>) {
    let output = run(path, input, mode);
    assert!(output.status.success(), "{}: {output:?}", path.display());
    assert_eq!(
        output.stdout,
        expected,
        "{} input {input:?}",
        path.display()
    );
    assert!(output.stderr.is_empty(), "{}: {output:?}", path.display());
}
fn binaries(temp: &Path, stem: &str, short_write: bool) -> Vec<PathBuf> {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/byte_io");
    let report = compile_path(
        &app.join(format!("{stem}.zil")),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.join(format!("{stem}-zil-default"))),
            emit_dir: Some(temp.join(format!("{stem}-ir"))),
            ..CompileOptions::default()
        },
    );
    fs::write(
        temp.join(format!("{stem}-compile-report.json")),
        report.to_json(),
    )
    .unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    assert_eq!(report.source_files.len(), 2);
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("[64 x i8]"));
    assert!(!llvm.contains("@malloc("));
    assert!(!llvm.contains(" noalias "));
    assert!(!llvm.contains(" nocapture "));
    let shim = temp.join("short-write.o");
    if short_write {
        build(
            Command::new("clang")
                .args(["-std=c11", "-Werror", "-Wall", "-Wextra", "-c"])
                .arg(app.join("short_write.c"))
                .arg("-o")
                .arg(&shim),
            &temp.join("build-shim"),
        );
    }
    let mut binaries = vec![];
    for level in ["0", "2"] {
        let binary = temp.join(format!("{stem}-zil-O{level}"));
        let mut command = Command::new("clang");
        command
            .args(["-x", "ir", &format!("-O{level}")])
            .arg(&report.artifacts["llvm"]);
        if short_write {
            command
                .args(["-x", "none"])
                .arg(&shim)
                .arg("-Wl,--wrap=write");
        }
        command.arg("-o").arg(&binary);
        build(
            &mut command,
            &temp.join(format!("build-{stem}-zil-O{level}")),
        );
        binaries.push(binary);
    }
    for level in ["0", "2"] {
        let binary = temp.join(format!("{stem}-rust-O{level}"));
        let mut command = Command::new("rustc");
        command.arg(app.join(format!("{stem}.rs"))).args([
            "--edition=2024",
            "-C",
            &format!("opt-level={level}"),
            "-C",
            "overflow-checks=yes",
            "-D",
            "warnings",
        ]);
        if short_write {
            command
                .arg("-C")
                .arg(format!("link-arg={}", shim.display()))
                .args(["-C", "link-arg=-Wl,--wrap=write"]);
        }
        command.arg("-o").arg(&binary);
        build(
            &mut command,
            &temp.join(format!("build-{stem}-rust-O{level}")),
        );
        binaries.push(binary);
    }
    binaries
}

#[test]
fn bytes083_streaming_normalization_preserves_nul_high_bits_and_chunk_edges() {
    let temp = Temp::new();
    let mut cases = vec![
        vec![],
        b"hello, Zil!\n".to_vec(),
        "汉字、é、🌍\naBc".as_bytes().to_vec(),
        vec![0, 97, 0, 255, 128, 122],
    ];
    for length in [1, 63, 64, 65, 127, 128, 129, 513] {
        cases.push((0..length).map(|i| (i % 256) as u8).collect());
    }
    for binary in binaries(&temp.0, "main", false) {
        for input in &cases {
            let expected: Vec<_> = input.iter().map(u8::to_ascii_uppercase).collect();
            succeeds(&binary, input, &expected, None);
        }
    }
}

#[test]
fn bytes083_real_pipe_eof_and_resource_close_match_rust() {
    let temp = Temp::new();
    for binary in binaries(&temp.0, "pipe", false) {
        succeeds(&binary, b"", &[0, 127, 128, 255, 228, 184, 173, 10], None);
    }
    for binary in binaries(&temp.0, "errors", false) {
        succeeds(&binary, b"", b"E", None);
    }
}

#[test]
fn bytes083_unsigned_operations_cover_all_65536_pairs_at_o0_and_o2() {
    let temp = Temp::new();
    // Independent wider-integer model, without calling the ZIL implementation.
    let mut checksum = 0i64;
    for a in 0i64..256 {
        for b in 0i64..256 {
            checksum += (a + b) % 256 + (a - b + 256) % 256 + (a * b) % 256;
            checksum += (a & b) + (a | b) + (a ^ b) + (255 - a);
            checksum += if a < b { 1 } else { 0 };
            checksum += if a <= b { 2 } else { 0 };
            checksum += if a > b { 4 } else { 0 };
            checksum += if a >= b { 8 } else { 0 };
            checksum += if a == b { 16 } else { 0 };
            checksum += if a != b { 32 } else { 0 };
            if b != 0 {
                checksum += a / b + a % b;
            }
            if b < 8 {
                checksum += (a * (1 << b)) % 256 + a / (1 << b);
            }
            if a + b <= 255 {
                checksum += a + b;
            }
            if a >= b {
                checksum += a - b;
            }
            if a * b <= 255 {
                checksum += a * b;
            }
        }
    }
    assert_eq!(checksum, 73_740_310);
    let expected = format!("{checksum}\n");
    for binary in binaries(&temp.0, "arithmetic", false) {
        succeeds(&binary, b"", expected.as_bytes(), None);
    }
}

#[test]
fn bytes083_controlled_short_writes_and_no_progress_errors_preserve_prefix() {
    let temp = Temp::new();
    for binary in binaries(&temp.0, "partial", true) {
        for (mode, expected) in [
            ("0", vec![0, 127, 128, 255, 228, 184, 173, 10]),
            ("1", vec![0, 127, 128, b'E']),
            ("2", vec![0, 127, 128, b'E']),
            ("3", vec![b'E']),
        ] {
            succeeds(&binary, mode.as_bytes(), &expected, Some(mode));
        }
    }
}

fn check_source(temp: &Path, index: usize, text: &str, expected: Option<(Phase, &str)>) {
    let input = SourceUnit::new(FileId(0), ModuleId(0), "bytes083.zil", text);
    let report = compile_source(&input, &CompileOptions::default());
    fs::write(temp.join(format!("source-{index}.zil")), text).unwrap();
    fs::write(temp.join(format!("source-{index}.json")), report.to_json()).unwrap();
    if let Some((phase, code)) = expected {
        assert_ne!(report.status, ResultCategory::Success, "{text}");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == phase && d.code == code),
            "{text}\n{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    } else {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}\n{}",
            report.to_json()
        );
    }
}

#[test]
fn bytes083_ct_conversion_and_unsigned_operator_boundaries_fail_before_publication() {
    let temp = Temp::new();
    let good = [
        "fn main()->i32{let b=#(1+2u8);return i32(b)}",
        "fn main()->i32{let b=#(1<255u8);if b{return 0}return 1}",
        "fn main()->i32{let b:u8=#(255u8>>7u8);return i32(b)}",
        "fn main()->i32{let b:u8=#(128u8<<1u8);return i32(b)}",
        "fn main()->i32{let b:u8=#(u8(255));return i32(b)}",
        "fn f(a:u8)->u8{return a+%1u8}fn main()->i32{let b=#(f(255u8));return i32(b)}",
        "fn f()->i32{let n:i64=2147483647;return i32(n)}fn main()->i32{return #(f())}",
    ];
    for (i, text) in good.iter().enumerate() {
        check_source(&temp.0, i, text, None);
    }
    for (i, text) in [
        "fn main()->i32{let b=#(u8(-1));return i32(b)}",
        "fn main()->i32{let b=#(u8(256));return i32(b)}",
        "fn main()->i32{let b=#(255u8+1u8);return i32(b)}",
        "fn main()->i32{let b=#(0u8-1u8);return i32(b)}",
        "fn main()->i32{let b=#(16u8*16u8);return i32(b)}",
        "fn main()->i32{let b=#(1u8/0u8);return i32(b)}",
        "fn main()->i32{let b=#(1u8%0u8);return i32(b)}",
        "fn main()->i32{let b=#(1u8<<8u8);return i32(b)}",
        "fn main()->i32{let b=#(255u8>>8u8);return i32(b)}",
        "fn f()->i32{let n:i64=2147483648;return i32(n)}fn main()->i32{return #(f())}",
    ]
    .iter()
    .enumerate()
    {
        check_source(
            &temp.0,
            i + good.len(),
            text,
            Some((Phase::Ct, "E-CT-ARITHMETIC")),
        );
    }
}

#[test]
fn bytes083_runtime_traps_and_byte_entry_are_observed_in_real_native_code() {
    let temp = Temp::new();
    let text = r#"extern "C" fn getchar()->i32;
fn main()->i32{
var code=0;unsafe{code=getchar()}
let one:u8=u8(code-code+1);let zero:u8=one-1u8;let max:u8=255
if code==110{return i32(u8(code-111))}
if code==112{return i32(u8(code+144))}
if code==119{let n:i64=2147483648;return i32(n)}
if code==109{let n:i64=-2147483649;return i32(n)}
if code==97{return i32(max+one)}
if code==115{return i32(zero-one)}
if code==116{let a:u8=16;return i32(a*a)}
if code==122{return i32(one/zero)}
if code==114{return i32(one%zero)}
if code==108{return i32(one<<8u8)}
if code==104{return i32(max>>8u8)}
if code==101{return i32(-one)}
return 0}"#;
    fs::write(temp.0.join("traps.zil"), text).unwrap();
    let report = compile_path(
        &temp.0.join("traps.zil"),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("traps-default")),
            emit_dir: Some(temp.0.join("traps-ir")),
            ..CompileOptions::default()
        },
    );
    fs::write(temp.0.join("traps-report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    for level in ["0", "2"] {
        let binary = temp.0.join(format!("traps-O{level}"));
        build(
            Command::new("clang")
                .args(["-x", "ir", &format!("-O{level}")])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&binary),
            &temp.0.join(format!("build-traps-O{level}")),
        );
        succeeds(&binary, b"q", b"", None);
        for code in b"npwamstzrlhe" {
            let output = run(&binary, &[*code], None);
            // This suite's x86_64 Linux LLVM trap is SIGILL. A normal nonzero
            // return (e.g. an incorrect wrapped byte 255) is not a trap.
            assert_eq!(output.status.signal(), Some(4), "trap {code}: {output:?}");
            assert!(output.stdout.is_empty(), "trap published an output");
            assert!(output.stderr.is_empty(), "trap published diagnostics");
        }
    }
    let byte = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "byte_entry.zil",
        "fn byte()->u8{return 255u8}",
    );
    let report = compile_source(
        &byte,
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("byte-entry")),
            entry: Some("byte".into()),
            ..CompileOptions::default()
        },
    );
    fs::write(temp.0.join("byte-entry.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    succeeds(&report.artifacts["executable"], b"", b"255\n", None);
    let main = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "byte_main.zil",
        "fn main()->u8{return 255u8}",
    );
    let report = compile_source(
        &main,
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("bad-main")),
            ..CompileOptions::default()
        },
    );
    fs::write(temp.0.join("byte-main.json"), report.to_json()).unwrap();
    assert_ne!(report.status, ResultCategory::Success);
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn bytes083_copy_capture_address_mutations_survive_every_exit_and_loop_order() {
    let temp = Temp::new();
    let prelude = "extern \"C\" fn putchar(n:i32)->i32;fn bump(p:&mut u8){*p=*p+1u8}";
    let cases: [(&str, &[u8]); 8] = [
        (
            "fn result()->i32{var b=128u8;var f=move ()=>{bump(&mut b);return b};return i32(f())+i32(f())}",
            b"259\n",
        ),
        (
            "fn result()->i32{var b:[u8;2]=[128u8,7u8];var f=move ()=>{bump(&mut b[0]);return b[0]};return i32(f())+i32(f())}",
            b"259\n",
        ),
        (
            "fn result()->i32{var b=128u8;var f=move ()=>{let p=&mut b;bump(p);return b};return i32(f())+i32(f())}",
            b"259\n",
        ),
        (
            "fn result()->i32{var b=128u8;var f=move ()=>{var step=0;while step<2{if step==1{return b}bump(&mut b);step+=1}return b};return i32(f())+i32(f())}",
            b"259\n",
        ),
        (
            "fn result()->i32{var b=128u8;var f=move ()=>{bump(&mut b);unsafe{putchar(i32(b))}};f();f();return 0}",
            b"\x81\x820\n",
        ),
        (
            "fn fail(p:&mut u8)->bool!u8{bump(p);unsafe{putchar(i32(*p))}return true}fn result()->i32{var b=128u8;var f=move ()->bool!u8=>{fail(&mut b)?;return b};let first=f();let second=f();return 0}",
            b"\x81\x820\n",
        ),
        (
            "fn result()->i32{var b=128u8;var f=move (flag:bool)=>{if flag{bump(&mut b)}else{b=b+1u8}return b};return i32(f(true))+i32(f(false))+i32(f(true))}",
            b"390\n",
        ),
        (
            "fn status(p:&mut u8)->i32{bump(p);return -1}fn result()->i32{var b=128u8;var f=move ()=>{if status(&mut b)==-1{return b}return 0u8};return i32(f())+i32(f())+i32(f())}",
            b"390\n",
        ),
    ];
    for (index, (text, expected)) in cases.into_iter().enumerate() {
        let text = format!("{prelude}{text}");
        let source = temp.0.join(format!("captured-{index}.zil"));
        fs::write(&source, &text).unwrap();
        let report = compile_path(
            &source,
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(temp.0.join(format!("captured-{index}-default"))),
                emit_dir: Some(temp.0.join(format!("captured-{index}-ir"))),
                entry: Some("result".into()),
                ..CompileOptions::default()
            },
        );
        fs::write(
            temp.0.join(format!("captured-{index}-report.json")),
            report.to_json(),
        )
        .unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}\n{}",
            report.to_json()
        );
        for level in ["0", "2"] {
            let binary = temp.0.join(format!("captured-{index}-O{level}"));
            build(
                Command::new("clang")
                    .args(["-x", "ir", &format!("-O{level}")])
                    .arg(&report.artifacts["llvm"])
                    .arg("-o")
                    .arg(&binary),
                &temp.0.join(format!("build-captured-{index}-O{level}")),
            );
            succeeds(&binary, b"", expected, None);
        }
    }
    // Account for both the new capture scan and the emitted Load/WriteCapture.
    // Neither category may silently exceed the caller's existing allowance.
    let input = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "capture-budget.zil",
        format!("{prelude}{}", cases[0].0),
    );
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let mut boundaries = vec![];
    for analysis in [true, false] {
        let mut low = 0;
        let defaults = zil_base::Limits::default();
        let mut high = if analysis {
            defaults.max_analysis_steps
        } else {
            defaults.max_instructions
        };
        let check = |budget| {
            let mut limits = defaults.clone();
            if analysis {
                limits.max_analysis_steps = budget;
            } else {
                limits.max_instructions = budget;
            }
            match zil_types::check(&core, &zil_base::TargetProfile::default(), &limits) {
                Ok(_) => true,
                Err(errors) => {
                    assert!(
                        errors.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                        "{errors:?}"
                    );
                    false
                }
            }
        };
        assert!(check(high));
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if check(middle) {
                high = middle
            } else {
                low = middle
            }
        }
        assert!(check(high));
        assert!(!check(high - 1));
        boundaries.push(high);
    }
    fs::write(
        temp.0.join("capture-budgets.json"),
        format!(
            "{{\"types_analysis_n\":{},\"types_instruction_n\":{}}}\n",
            boundaries[0], boundaries[1]
        ),
    )
    .unwrap();
    println!(
        "capture083 budgets: analysis={} instructions={} (N passes, N-1 rejects)",
        boundaries[0], boundaries[1]
    );
}

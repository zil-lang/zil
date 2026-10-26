//! Checked integer shifts and ordinary text algorithms through the real pipeline.
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
use zil_driver::{
    CompileOptions, CompileReport, Optimization, Stage, compile_path, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "shift124-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, retained.is_some())
    }
    fn compile(&self, source: &str, optimization: Option<Optimization>) -> CompileReport {
        fs::write(self.0.join("main.zil"), source).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "shift124.zil", source),
            &self.options(optimization),
        );
        self.record(&report);
        report
    }
    fn options(&self, optimization: Option<Optimization>) -> CompileOptions {
        CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Lower
            },
            entry: Some("value".into()),
            output: optimization.map(|_| self.0.join("program")),
            emit_dir: Some(self.0.join("phases")),
            optimization: optimization.unwrap_or(Optimization::O0),
            ..CompileOptions::default()
        }
    }
    fn record(&self, report: &CompileReport) {
        fs::write(self.0.join("compile.json"), report.to_json()).unwrap();
    }
    fn modules(&self, main: &str, optimization: Optimization) -> CompileReport {
        fs::write(self.0.join("main.zil"), main).unwrap();
        for (name, source) in [
            (
                "utf8",
                include_str!("../../../examples/mainline/text_decode/utf8.zil"),
            ),
            (
                "text",
                include_str!("../../../examples/mainline/text_decode/text.zil"),
            ),
            (
                "buffer",
                include_str!("../../../examples/mainline/text_decode/buffer.zil"),
            ),
        ] {
            fs::write(self.0.join(format!("{name}.zil")), source).unwrap();
        }
        let report = compile_path(&self.0.join("main.zil"), &self.options(Some(optimization)));
        self.record(&report);
        report
    }
    fn run(&self, executable: &Path, input: &[u8], expected: Option<&[u8]>) -> Output {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let prefix = self.0.join(format!("native-{id}"));
        let mut child = Command::new(executable)
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
                fs::write(
                    prefix.with_extension("timeout"),
                    b"5-second native deadline exceeded",
                )
                .unwrap();
                panic!("native text/shift test timed out");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let output = child.wait_with_output().unwrap();
        fs::write(prefix.with_extension("stdin"), input).unwrap();
        fs::write(prefix.with_extension("stdout"), &output.stdout).unwrap();
        fs::write(prefix.with_extension("stderr"), &output.stderr).unwrap();
        fs::write(prefix.with_extension("status"), output.status.to_string()).unwrap();
        if let Some(expected) = expected {
            fs::write(prefix.with_extension("expected"), expected).unwrap();
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(output.stdout, expected);
            assert!(output.stderr.is_empty());
        }
        output
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

// Independent mathematical bit-vector model, not the CT VM's checked_shl/shr.
fn expected(value: i64, count: u32, width: u32, left: bool) -> i64 {
    if !left {
        return (i128::from(value).div_euclid(1i128 << count)) as i64;
    }
    let modulus = 1u128 << width;
    let bits = ((i128::from(value) as u128) << count) & (modulus - 1);
    let signed = if bits >= modulus / 2 {
        bits as i128 - modulus as i128
    } else {
        bits as i128
    };
    signed as i64
}
const READ: &str = r#"extern "C" fn getchar()->i32;
fn read()->i64{var c:i32=0;unsafe{c=getchar()};let negative=c==45;if negative{unsafe{c=getchar()}};var n:i64=0;while c>=48 && c<=57{n=n*10-i64(c-48);unsafe{c=getchar()}};if negative{return n};return -n}
"#;

#[test]
fn dynamic_signed_bit_patterns_and_invalid_counts_match_the_contract() {
    for (ty, width) in [("i32", 32), ("i64", 64)] {
        let min = if width == 32 {
            i64::from(i32::MIN)
        } else {
            i64::MIN
        };
        let max = if width == 32 {
            i64::from(i32::MAX)
        } else {
            i64::MAX
        };
        for (operator, left) in [("<<", true), (">>", false)] {
            for optimization in [Optimization::O0, Optimization::O2] {
                let case = Case::new(&format!("dynamic-{ty}-{left}-{optimization:?}"));
                let report = case.compile(&format!("{READ}fn value()->i64{{let a={ty}(read());let b={ty}(read());return i64(a{operator}b)}}"), Some(optimization));
                success(&report);
                let binary = &report.artifacts["executable"];
                for value in [min, min + 1, -3, -1, 0, 1, max] {
                    for count in [0, 1, 6, width - 1] {
                        let input = format!("{value} {count}\n");
                        let result = format!("{}\n", expected(value, count, width, left));
                        case.run(binary, input.as_bytes(), Some(result.as_bytes()));
                    }
                }
                for count in [-1, i64::from(width), i64::from(width) + 1, min, max] {
                    let input = format!("-3 {count}\n");
                    let output = case.run(binary, input.as_bytes(), None);
                    assert_eq!(
                        output.status.signal(),
                        Some(4),
                        "count {count}: {:?}",
                        output.status
                    );
                    assert!(output.stdout.is_empty());
                    assert!(output.stderr.is_empty());
                }
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains(&format!("icmp uge {ty}")));
                assert!(llvm.contains(if left { " = shl " } else { " = ashr " }));
                assert!(
                    !llvm.contains("shl nsw")
                        && !llvm.contains("shl nuw")
                        && !llvm.contains("ashr exact")
                );
            }
        }
    }
}

#[test]
fn source_ct_agrees_on_signed_extremes_and_bit_discarding() {
    let mut functions = String::new();
    let mut checks = String::new();
    let mut index = 0;
    for (ty, width) in [("i32", 32), ("i64", 64)] {
        let min = if width == 32 {
            i64::from(i32::MIN)
        } else {
            i64::MIN
        };
        let max = if width == 32 {
            i64::from(i32::MAX)
        } else {
            i64::MAX
        };
        for (operator, left) in [("<<", true), (">>", false)] {
            for (value, count) in [
                (min, 1),
                (min, width - 1),
                (max, 1),
                (max, width - 1),
                (-3, 1),
                (-1, width - 1),
                (1, width - 1),
                (0, 0),
            ] {
                index += 1;
                functions.push_str(&format!("fn f{index}()->{ty}{{let a:{ty}={value};let n:{ty}={count};return a{operator}n}}"));
                let result = expected(value, count, width, left);
                checks.push_str(&format!("let x{index}:{ty}=#(f{index}());let e{index}:{ty}={result};if x{index}!=e{index}{{return {index}}}"));
            }
        }
    }
    let source = format!("{functions}fn value()->i64{{{checks}return 42}}");
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("ct-extremes");
        let report = case.compile(&source, Some(optimization));
        success(&report);
        case.run(&report.artifacts["executable"], b"", Some(b"42\n"));
    }
}

#[test]
fn source_ct_rejects_negative_equal_width_and_larger_counts_precisely() {
    for (ty, width) in [("i32", 32), ("i64", 64)] {
        for operator in ["<<", ">>"] {
            for count in [-1, width, width + 1] {
                let case = Case::new("ct-count");
                let report = case.compile(&format!("fn f()->{ty}{{let a:{ty}=-3;let n:{ty}={count};return a{operator}n}}fn value()->i64{{return i64(#(f()))}}"), None);
                assert_eq!(
                    report.status,
                    ResultCategory::SafeReject,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
                assert_eq!(report.diagnostics[0].code, "E-CT-ARITHMETIC");
                assert!(!report.artifacts.contains_key("executable"));
            }
        }
    }
}

fn text_expected(input: &[u8]) -> i64 {
    if input.len() > 1024 {
        return -2000;
    }
    match std::str::from_utf8(input) {
        Err(error) => -(error.valid_up_to() as i64 + 1),
        Ok(text) => {
            let units = text.encode_utf16().collect::<Vec<_>>();
            if units.len() > 32 {
                return -3000;
            }
            let checksum = units.iter().fold(0i64, |hash, unit| {
                (hash * 33 + i64::from(*unit)) % 1_000_000_000
            });
            text.chars().count() as i64 * 1_000_000_000_000
                + units.len() as i64 * 1_000_000_000
                + checksum
        }
    }
}

#[test]
fn four_module_dynamic_text_consumer_matches_rust_unicode_validation() {
    let mut inputs = vec![
        vec![],
        b"ASCII".to_vec(),
        b"A\0B".to_vec(),
        "A中🙂".as_bytes().to_vec(),
        vec![b'a'; 1025],
    ];
    for value in [
        0, 0x7f, 0x80, 0x7ff, 0x800, 0xd7ff, 0xe000, 0xffff, 0x10000, 0x10ffff,
    ] {
        inputs.push(char::from_u32(value).unwrap().to_string().into_bytes());
    }
    for invalid in [
        &b"\x80"[..],
        &b"\xbf"[..],
        &b"\xc0\xaf"[..],
        &b"\xc1\xbf"[..],
        &b"\xc2"[..],
        &b"\xe0\x80\x80"[..],
        &b"\xe4\xb8"[..],
        &b"\xed\xa0\x80"[..],
        &b"\xef\xff\xbf"[..],
        &b"\xf0\x80\x80\x80"[..],
        &b"\xf0\x9f\x99"[..],
        &b"\xf4\x90\x80\x80"[..],
        &b"\xf5\x80\x80\x80"[..],
        &b"\xff"[..],
    ] {
        inputs.push(invalid.to_vec());
        inputs.push([b"A".as_slice(), invalid].concat());
    }
    inputs.push("🙂".repeat(16).into_bytes());
    inputs.push("🙂".repeat(17).into_bytes());
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("text");
        let report = case.modules(
            include_str!("../../../examples/mainline/text_decode/main.zil"),
            optimization,
        );
        success(&report);
        for input in &inputs {
            let expected = format!("{}\n", text_expected(input));
            case.run(
                &report.artifacts["executable"],
                input,
                Some(expected.as_bytes()),
            );
        }
    }
}

#[test]
fn ordinary_text_interfaces_preserve_output_on_failure_and_map_scalar_positions() {
    let source = r#"import text
import utf8
fn value()->i64{
 let valid=[65u8,228u8,184u8,173u8,240u8,159u8,153u8,130u8];
 let bad=[65u8,237u8,160u8,128u8];
 var small=[170u16,171u16];
 if text.write_utf16(&bad[..],&mut small[..])!=-1{return 1}
 if small[0]!=170u16 || small[1]!=171u16{return 2}
 if text.write_utf16(&valid[..],&mut small[..])!=-2{return 3}
 if small[0]!=170u16 || small[1]!=171u16{return 4}
 if text.byte_position(&valid[..],0)!=0{return 5}
 if text.byte_position(&valid[..],1)!=1{return 6}
 if text.byte_position(&valid[..],2)!=4{return 7}
 if text.byte_position(&valid[..],3)!=8{return 8}
 if text.byte_position(&valid[..],4)!=-1{return 9}
 if text.byte_position(&valid[..],-1)!=-1{return 10}
 if text.byte_position(&bad[..],0)!=-1{return 11}
 if utf8.decode(&valid[..],-1).valid{return 12}
 if utf8.decode(&valid[..],8).valid{return 13}
 return 42
}"#;
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("text-interfaces");
        let report = case.modules(source, optimization);
        success(&report);
        case.run(&report.artifacts["executable"], b"", Some(b"42\n"));
    }
}

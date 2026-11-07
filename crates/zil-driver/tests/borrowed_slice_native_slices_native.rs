//! Actual source-to-native slice behavior. Rust is an independent observable
//! behavior control, not the ZIL safety checker or a performance comparison.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
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
            "zil-slices092-{}-{}",
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
fn compile(text: &str, optimization: Optimization) -> Temp {
    let temp = Temp::new();
    fs::write(temp.0.join("source.zil"), text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "slices092.zil", text);
    let report = compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("value".into()),
            output: Some(temp.0.join("program")),
            emit_dir: Some(temp.0.join("phases")),
            optimization,
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
    temp
}
fn run(path: &Path, input: &[u8]) -> Output {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("slice program timed out: {}", path.display());
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
        record.with_extension("status"),
        format!("{:?}\n", output.status),
    )
    .unwrap();
    output
}
fn succeeds(text: &str, expected: &str) {
    for level in [Optimization::O0, Optimization::O2] {
        let temp = compile(text, level);
        let output = run(&temp.0.join("program"), &[]);
        assert!(output.status.success(), "{text}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{text}");
        assert!(output.stderr.is_empty(), "{text}: {output:?}");
    }
}

#[test]
fn ordinary_functions_share_runtime_lengths_and_preserve_mutable_returned_windows() {
    succeeds(
        "fn sum(p:&[i64])->i64{var i:i64=0;var s:i64=0;while i<p.len(){s+=p[i];i+=1}return s}fn value()->i64{let a:[i64;4]=[2,5,9,12];let b:[i64;2]=[20,30];return sum(&a[1..])+sum(&b[..])}",
        "76\n",
    );
    succeeds(
        "fn tail(p:&mut [i32],start:i64)->&mut [i32]{return &mut p[start..]}fn relay(p:&mut [i32])->&mut [i32]{return tail(p,1)}fn value()->i64{var a=[1,2,3,4];let p=&mut a[..];let q=relay(p);q[0]=7;q[2]=9;let s=&*q;let n=s.len();let answer=s[0]+s[2];return i64(answer)+n+i64(a[0])}",
        "20\n",
    );
    succeeds(
        "fn inc(p:&mut [i32]){var i:i64=0;while i<p.len(){p[i]+=1;i+=1}}fn value()->i32{var a=[1,2,3,4];var count=0;while count<3{inc(&mut a[..]);count+=1}return a[0]+a[1]+a[2]+a[3]}",
        "22\n",
    );
}

#[test]
fn nested_ranges_empty_windows_and_integer_endpoint_widths_have_exact_lengths() {
    succeeds(
        "fn value()->i64{let a=[10u8,20,30,40,50];let p=&a[1u8..5u16];let q=&p[i64(1)..3];let empty=&q[2..2];return i64(q[0u16])+i64(q[1u8])+q.len()+empty.len()}",
        "72\n",
    );
    succeeds(
        "fn value()->i64{let a:[i32;0]=[];let p=&a[..];let q=&p[..];return q.len()}",
        "0\n",
    );
    succeeds(
        "fn choose(a:&[i32],b:&[i32],first:bool)->&[i32]{return if first{a}else{b}}fn value()->i64{let a=[1,2];let b=[3,4,5];let p=choose(&a[..],&b[..],false);return i64(p[2])+p.len()}",
        "8\n",
    );
}

#[test]
fn copy_records_zero_sized_elements_and_existing_method_names_keep_their_layout() {
    succeeds(
        "struct Row{byte:u8;wide:i64}fn modify(p:&mut [Row]){p[1].wide=41}fn value()->i64{var rows=[Row{byte:1u8,wide:2},Row{byte:3u8,wide:4}];modify(&mut rows[..]);let p=&rows[..];return p[1].wide+i64(p[0].byte)+p.len()}",
        "44\n",
    );
    succeeds(
        "fn value()->i64{var units=[(),(),()];let p=&mut units[..];p[0]=();let u=p[1];return p.len()}",
        "3\n",
    );
    succeeds(
        "struct Empty{} fn value()->i64{var empty=[Empty{},Empty{}];let p=&mut empty[..];p[1]=Empty{};let e=p[0];return p.len()}",
        "2\n",
    );
    succeeds(
        "fn value()->i64{let a:[[u8;0];2]=[[],[]];let p=&a[..];let e=p[1];return p.len()}",
        "2\n",
    );
    succeeds(
        "fn len()->i64{return 5}struct Row{n:i64}impl Row{fn len(self:&Self)->i64{return .n}}fn value()->i64{let a=[1,2,3];let p=&a[..];let row=Row{n:7};return len()+row.len()+p.len()}",
        "15\n",
    );
}

#[test]
fn receiver_and_range_endpoints_are_evaluated_once_in_source_order() {
    succeeds(
        "extern \"C\" fn putchar(x:i32)->i32;fn base(p:&[i32;4])->&[i32;4]{unsafe{putchar(65)}return p}fn low()->i32{unsafe{putchar(66)}return 1}fn high()->i64{unsafe{putchar(67)}return 3}fn value()->i32{let a=[10,20,30,40];let p=&base(&a)[low()..high()];return p[0]+p[1]}",
        "ABC50\n",
    );
}

#[test]
fn closure_local_views_use_the_actual_body_or_captured_array_storage() {
    succeeds(
        "fn value()->i64{let f=()=>{let a=[1,2];let s=&a[..];return s.len();};return f();}",
        "2\n",
    );
    succeeds(
        "fn value()->i32{var a=[1,2];var f=move()=>{let s=&mut a[..];s[0]=s[0]+1;return s[0];};return f()+f();}",
        "5\n",
    );
    succeeds(
        "fn value()->i32{var a=[1,2];var f=()=>{let s=&mut a[..];s[0]=s[0]+1;return s[0];};let last=f()+f();return last+a[0];}",
        "8\n",
    );
}

#[test]
fn ordinary_error_and_success_returns_release_the_window_and_clean_resources_once() {
    succeeds(
        "extern \"C\" fn putchar(x:i32)->i32;resource Token{code:i32}impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}fn work(p:&mut [i32],fail:bool,code:i32)->bool!i32{let owner=Token{code};p[0]+=1;if fail{return true}return 0}fn value()->i32{var a=[1,2];let failed=work(&mut a[..],true,65);let passed=work(&mut a[..],false,66);return a[0]}",
        "AB3\n",
    );
}

#[cfg(unix)]
#[test]
fn all_invalid_dynamic_ranges_and_indices_trap_at_both_optimization_levels() {
    use std::os::unix::process::ExitStatusExt;
    let cases = [
        "let p=&a[-1..1];return p.len()",
        "let p=&a[0..-1];return p.len()",
        "let p=&a[-2..-1];return p.len()",
        "let p=&a[2..1];return p.len()",
        "let p=&a[..4];return p.len()",
        "let p=&a[4..];return p.len()",
        "let high:i64=9223372036854775807;let p=&a[..high];return p.len()",
        "let p=&a[..];let q=&p[1..4];return q.len()",
        "let p=&a[..];return i64(p[-1])",
        "let p=&a[..];return i64(p[3])",
        "let p=&a[1..1];return i64(p[0])",
    ];
    for level in [Optimization::O0, Optimization::O2] {
        for body in cases {
            let text = format!("fn value()->i64{{let a=[10,20,30];{body}}}");
            let temp = compile(&text, level);
            let output = run(&temp.0.join("program"), &[]);
            assert_eq!(output.status.signal(), Some(4), "{text}: {output:?}");
            assert!(output.stdout.is_empty(), "{text}: {output:?}");
        }
    }
}

#[test]
fn independently_verified_slice_lir_cannot_be_tampered_after_checking() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "bound092.zil",
        "fn main()->i32{let a=[1,2,3];let p=&a[1..3];return p[0]}",
    );
    let limits = Limits::default();
    for target in [
        TargetProfile::host_llvm(),
        TargetProfile::windows_x64_llvm(),
    ] {
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &syntax).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        let verified = zil_verify::verify_module(typed, plan, &target, &limits).unwrap();
        let mut lir = zil_lower::lower(&verified).unwrap();
        let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
        assert!(llvm.contains("{ ptr, i64 }"));
        assert!(!llvm.contains("@malloc("));
        assert!(!llvm.contains(" noalias "));
        let instruction = lir
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, zil_ir::InstructionKind::AddressSlice { .. }))
            .unwrap();
        let zil_ir::InstructionKind::AddressSlice { end, .. } = &mut instruction.kind else {
            unreachable!()
        };
        *end = None;
        let errors = zil_backend_llvm::emit(&verified, &lir).unwrap_err();
        assert_eq!(errors[0].code, "E-CODEGEN-UNBOUND-LIR");
    }
}

#[test]
fn streaming_cross_module_consumer_matches_independent_rust_for_binary_inputs() {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/slices");
    let inputs = vec![
        vec![],
        b"a".to_vec(),
        b"Hello ZIL\n".to_vec(),
        vec![0, 127, 128, 255],
        "ZIL中文😀\0abc\n".as_bytes().to_vec(),
        vec![b'a'; 15],
        vec![b'b'; 16],
        vec![b'c'; 17],
        vec![b'd'; 31],
        vec![b'e'; 32],
        vec![b'f'; 33],
        (0..=255).collect::<Vec<u8>>(),
    ];
    for (level, number) in [(Optimization::O0, "0"), (Optimization::O2, "2")] {
        let temp = Temp::new();
        let zil = temp.0.join("zil-program");
        let rust = temp.0.join("rust-program");
        let report = compile_path(
            &app.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(zil.clone()),
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
        assert_eq!(report.source_files.len(), 3);
        let mut command = Command::new("rustc");
        command
            .arg(app.join("main.rs"))
            .args(["--edition=2024", "-C", &format!("opt-level={number}")])
            .arg("-o")
            .arg(&rust);
        let built = command.output().unwrap();
        fs::write(temp.0.join("rust-build-command"), format!("{command:?}\n")).unwrap();
        fs::write(temp.0.join("rust-build-stdout"), &built.stdout).unwrap();
        fs::write(temp.0.join("rust-build-stderr"), &built.stderr).unwrap();
        fs::write(
            temp.0.join("rust-build-status"),
            format!("{:?}\n", built.status),
        )
        .unwrap();
        assert!(built.status.success(), "{built:?}");
        for input in &inputs {
            let expected = input.iter().map(u8::to_ascii_uppercase).collect::<Vec<_>>();
            let actual = run(&zil, input);
            let oracle = run(&rust, input);
            assert!(
                actual.status.success() && oracle.status.success(),
                "{actual:?} {oracle:?}"
            );
            assert_eq!(actual.stdout, expected);
            assert_eq!(actual.stdout, oracle.stdout);
            assert!(actual.stderr.is_empty() && oracle.stderr.is_empty());
        }
    }
}

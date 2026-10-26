//! Real field ownership updates, exact native cleanup, and independent Rust oracles.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::ResultCategory;
use zil_driver::{CompileOptions, Stage, compile_path};

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
            "zil-update-ui-{}-{}",
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
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn run(path: &Path, input: &[u8], expected_code: i32, expected: &str) {
    let mut child = Command::new(path)
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
            panic!("package application timed out: {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    if std::env::var_os("ZIL_TEST_REPORT_DIR").is_some() {
        let record = path.with_file_name(format!(
            "run-{}-{}",
            path.file_name().unwrap().to_string_lossy(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(record.with_extension("stdin"), input).unwrap();
        fs::write(record.with_extension("stdout"), &output.stdout).unwrap();
        fs::write(record.with_extension("stderr"), &output.stderr).unwrap();
        fs::write(
            record.with_extension("exit"),
            format!("{:?}\n", output.status.code()),
        )
        .unwrap();
    }
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "{} input {input:?}: {output:?}",
        path.display()
    );
    assert_eq!(
        output.stdout,
        expected.as_bytes(),
        "{} input {input:?}",
        path.display()
    );
    assert!(output.stderr.is_empty(), "{}: {output:?}", path.display());
}
fn binaries(temp: &Path, stem: &str, source_files: usize) -> Vec<PathBuf> {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/update_ui");
    let report = compile_path(
        &app.join(format!("{stem}.zil")),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.join(format!("{stem}-zil-O0"))),
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
        "{stem}: {}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
    assert_eq!(report.source_files.len(), source_files);
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("%package.live."));
    assert!(llvm.contains("package.field.drop."));
    assert!(!llvm.contains("@malloc("));
    assert!(!llvm.contains(" noalias "));
    let mut binaries = vec![report.artifacts["executable"].clone()];
    let optimized = temp.join(format!("{stem}-zil-O2"));
    build(
        Command::new("clang")
            .args(["-x", "ir", "-O2"])
            .arg(&report.artifacts["llvm"])
            .arg("-o")
            .arg(&optimized),
        &temp.join(format!("build-{stem}-zil-O2")),
    );
    binaries.push(optimized);
    for level in ["0", "2"] {
        let binary = temp.join(format!("{stem}-rust-O{level}"));
        build(
            Command::new("rustc")
                .arg(app.join(format!("{stem}.rs")))
                .args([
                    "--edition=2024",
                    "-C",
                    &format!("opt-level={level}"),
                    "-C",
                    "overflow-checks=yes",
                    "-D",
                    "warnings",
                    "-o",
                ])
                .arg(&binary),
            &temp.join(format!("build-{stem}-rust-O{level}")),
        );
        binaries.push(binary);
    }
    binaries
}

#[test]
fn packages074_component_updates_preserve_sibling_descriptions_and_whole_abi() {
    let temp = Temp::new();
    let mut cases: Vec<(Vec<u8>, i32, String)> = [
        ("", 0, "0\n#1\nMSU"),
        ("q", 0, "0\n#1\nMSU"),
        ("+r-q", 0, "0\n1\nS1\n0\n#4\nMSU"),
        ("tbbq", 0, "0\nS0\nS0\nS0\n#4\nMSU"),
        ("+fq", 1, "0\n1\nMSU"),
        ("rx", 1, "0\nS0\nMSU"),
        (" \n=+q", 0, "0\n1\n#2\nMSU"),
    ]
    .into_iter()
    .map(|(input, code, output)| (input.as_bytes().to_vec(), code, output.into()))
    .collect();
    cases.push((vec![b'='; 128], 1, "0\nMSU".into()));
    let mut saturated = vec![b'+'; 52];
    saturated.push(b'q');
    let expected = (0..=50).map(|n| format!("{n}\n")).collect::<String>() + "#51\nMSU";
    cases.push((saturated, 0, expected));
    for binary in binaries(&temp.0, "main", 4) {
        for (input, code, output) in &cases {
            run(&binary, input, *code, output);
        }
    }
}

#[test]
fn packages074_field_preparation_failures_partial_exits_and_refills_drop_once() {
    let temp = Temp::new();
    for binary in binaries(&temp.0, "lifecycle", 3) {
        for (input, code, output) in [
            (b"0", 1, "ABCZ"),
            (b"1", 1, "DABCZ"),
            (b"2", 1, "ABCZ"),
            (b"3", 0, "ADBCZ"),
            (b"4", 0, "BACZ"),
            (b"5", 0, "CBAZ"),
            (b"6", 0, "ADBCADBC"),
            (b"7", 0, "ABCDBCZ"),
            (b"8", 0, "ABCZ"),
            (b"9", 1, "ADBCZ"),
        ] {
            run(&binary, input, code, output);
        }
    }
}

#[test]
fn packages074_real_descriptors_close_after_replacement_move_and_partial_cleanup() {
    let temp = Temp::new();
    for binary in binaries(&temp.0, "fd", 1) {
        run(&binary, b"", 0, "F");
    }
}

#[test]
fn packages074_new_field_operations_cannot_escape_ct_or_verified_lir_binding() {
    use zil_base::{FileId, Limits, ModuleId, Phase, SourceUnit, TargetProfile};
    use zil_ir::{CtProgram, InstructionKind};
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "field-boundary074.zil",
        "resource R{n:i32} package P{manages a:R;manages b:R;n:i32} fn main()->i32{var p=P{a:R{n:20},b:R{n:21},n:1};let a=give p.a;p.a=R{n:a.n+1};p.n=2;return p.a.n+p.b.n+p.n-44}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let verified =
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let entry = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap()
        .id;
    let raw = CtProgram {
        module,
        plan,
        entry,
        max_steps: 1000,
        max_call_depth: 8,
        max_live_slots: 1000,
    };
    let bytes = zil_ir::wire::encode_ct_program(&raw, &limits).unwrap();
    for rejected in [
        zil_verify::verify_ct_program(raw, &target, &limits),
        zil_verify::verify_ct_program_bytes(&bytes, &target, &limits),
    ] {
        let errors = rejected.unwrap_err();
        assert_eq!(errors[0].phase, Phase::Ct);
        assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
    }
    let original = zil_lower::lower(&verified).unwrap();
    zil_backend_llvm::emit(&verified, &original).unwrap();
    // Mutate each new operation after independent verification. A preserved
    // fingerprint field is not authority for a different LIR instruction.
    for operation in 0..3 {
        let mut changed = original.clone();
        let kind = changed
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .map(|i| &mut i.kind)
            .find(|kind| {
                matches!(
                    (operation, &**kind),
                    (0, InstructionKind::TakePackageField { .. })
                        | (1, InstructionKind::DropPackageField { .. })
                        | (2, InstructionKind::StorePackageField { .. })
                )
            })
            .unwrap();
        match kind {
            InstructionKind::TakePackageField { index, .. }
            | InstructionKind::DropPackageField { index, .. }
            | InstructionKind::StorePackageField { index, .. } => *index = u32::MAX,
            _ => unreachable!(),
        }
        let errors = zil_backend_llvm::emit(&verified, &changed).unwrap_err();
        assert_eq!(errors[0].code, "E-CODEGEN-UNBOUND-LIR");
    }
}

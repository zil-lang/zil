//! A real multi-module application and its independently compiled Rust behavior oracle.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_path};
use zil_ir::InstructionKind;

const EXPECTED: &str = "475000738663\n";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-mainline-app-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn build(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn run(path: &Path, args: &[&str], expected: &str) {
    let mut child = Command::new(path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native application timed out: {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}: {output:?}", path.display());
    assert_eq!(output.stdout, expected.as_bytes(), "{}", path.display());
    assert!(output.stderr.is_empty(), "{}: {output:?}", path.display());
}

#[test]
fn mainline_application_matches_rust_and_self_checks_at_o0_and_o2() {
    let directory = Temp::new();
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/app");
    let input = app.join("main.zil");
    for (entry, mode, expected) in [(Some("result"), "result", EXPECTED), (None, "main", "")] {
        let report = compile_path(
            &input,
            &CompileOptions {
                stop_after: Stage::Build,
                entry: entry.map(str::to_owned),
                output: Some(directory.0.join(format!("zil-{mode}-O0"))),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.verification_runs > 0, "{}", report.to_json());
        assert_eq!(report.source_files.len(), 3, "{}", report.to_json());
        assert!(
            report
                .source_files
                .values()
                .any(|p| p.ends_with("seed/data/mod.zil"))
        );
        run(&report.artifacts["executable"], &[], expected);
        let optimized = directory.0.join(format!("zil-{mode}-O2"));
        build(
            Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&optimized),
        );
        run(&optimized, &[], expected);
    }
    for level in ["0", "2"] {
        let executable = directory.0.join(format!("rust-O{level}"));
        build(
            Command::new("rustc")
                .arg(app.join("main.rs"))
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
                .arg(&executable),
        );
        run(&executable, &[], EXPECTED);
        run(&executable, &["--check"], "");
    }
}

#[test]
fn contextual_code_argument_uses_its_actual_array_index_in_raw_and_wire() {
    let input = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "thin-code-arg.zil",
        r#"
fn add(x:i64)->i64 { return x+1 }
fn subtract(x:i64)->i64 { return x-1 }
fn result(i:i64)->i64 {
    let callbacks:[fn(i64)->i64;2]=[add,subtract];
    let apply=(f,x)=>f(x);
    let previous=callbacks[0](i);
    return apply(callbacks[i],previous);
}
"#,
    );
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let core = zil_expand::expand_with_map_semantics(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &std::collections::BTreeMap::new(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let core = zil_types::infer_source_closures(&core, &limits).unwrap();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    assert!(module.semantic.is_some());
    let plan = zil_analyze::analyze(&module, &limits).unwrap();

    for swap in [false, true] {
        let mut module = module.clone();
        let mut plan = plan.clone();
        if swap {
            let function = module
                .functions
                .iter_mut()
                .find(|f| f.name == "result")
                .unwrap();
            let mut indexes = function
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.instructions)
                .filter_map(|instruction| match &mut instruction.kind {
                    InstructionKind::ExtractIndex { index, .. } => Some(index),
                    _ => None,
                });
            let previous = *indexes.next().unwrap();
            let argument = indexes.next().unwrap();
            assert_ne!(*argument, previous);
            *argument = previous;
            assert!(indexes.next().is_none());
            plan.module_fingerprint = module.fingerprint();
        }
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        for result in [
            zil_verify::verify_module(module, plan, &target, &limits),
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ] {
            if swap {
                let errors = result.unwrap_err();
                assert!(
                    errors.iter().any(|error| {
                        error.category == ResultCategory::SafeReject
                            && error
                                .notes
                                .iter()
                                .any(|note| note.contains("actual code argument 0"))
                    }),
                    "{errors:#?}"
                );
            } else {
                result.unwrap();
            }
        }
    }
}

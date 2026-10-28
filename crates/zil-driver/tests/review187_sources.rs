//! Non-author external byte capture source, field and call-site substitution controls.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}
fn directory() -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "review187-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn compile(files: &[(&str, &str)], options: CompileOptions, dir: &Path) -> CompileReport {
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..options
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn check(source: &str) -> CompileReport {
    let dir = directory();
    compile(&[("main", source)], CompileOptions::default(), &dir)
}
fn native(files: &[(&str, &str)], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        let executable = dir.join("program");
        let report = compile(
            files,
            CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                ..CompileOptions::default()
            },
            &dir,
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        fs::write(dir.join("exit.txt"), format!("{:?}", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert!(output.stdout.is_empty());
    }
}

const FILL: &str = "fn fill(b:ByteBuffer,n:u8)->bool{return bytes_reserve(b,1)&&bytes_push(b,n)}";
const PAIR: &str = "package Pair{manages left:ByteBuffer;manages right:ByteBuffer}fn pair()->Pair{return Pair{left:bytes_new(),right:bytes_new()}}";
fn conflict(source: &str) {
    let report = check(source);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.phase,
        zil_base::Phase::Verify,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn captured_byte_fields_preserve_actual_sibling_and_alias_identity() {
    let source = format!(
        "{FILL}{PAIR}fn main()->i32{{let p=pair();if !fill(p.left,65u8){{return 8}}let read=p.left;let edit=p.right;let f=()=>{{let view=bytes_view(read);bytes_reserve(edit,4);return i32(view[0])}};return f()-65}}"
    );
    native(&[("main", &source)], 0);
    conflict(&source.replace("let edit=p.right", "let edit=p.left"));
    conflict(&source.replace("let edit=p.right", "let again=p.left;let edit=again"));
}

#[test]
fn helper_capture_and_argument_constraints_reach_an_ordinary_callers_actuals() {
    let helper = "fn inspect<F:Fn()->i32>(pad:i32,f:&F,b:ByteBuffer)->i32{let view=bytes_view(b);let value=f();return pad+value+i32(view[0])}fn run(a:ByteBuffer,b:ByteBuffer)->i32{let f=()=>{bytes_reserve(a,4);return 0};return inspect(9,&f,b)-9}";
    let source = format!(
        "{FILL}{helper}fn main()->i32{{let a=bytes_new();let b=bytes_new();if !fill(a,65u8)||!fill(b,66u8){{return 8}}return run(a,b)-66}}"
    );
    native(&[("main", &source)], 0);
    conflict(&source.replace("run(a,b)-66", "run(a,a)-65"));
}

#[test]
fn a_descriptor_environment_does_not_extend_the_owners_last_use() {
    let source = format!(
        "{FILL}fn take(give b:ByteBuffer){{}}fn main()->i32{{let b=bytes_new();if !fill(b,65u8){{return 8}}let f=()=>bytes_len(b);let saved=f;let n=saved();take(give b);return i32(n)-1}}"
    );
    native(&[("main", &source)], 0);
    let stale = source.replace(
        "let n=saved();take(give b);return i32(n)-1",
        "take(give b);return i32(saved())-1",
    );
    let report = check(&stale);
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
            .any(|d| d.code == "E-VERIFY-RESOURCE" || d.code == "E-UNINITIALIZED"),
        "{}",
        report.to_json()
    );
}

#[test]
fn raw_same_slot_retargeting_cannot_discard_a_private_separation_obligation() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::InstructionKind;
    let text = format!(
        "{FILL}fn main()->i32{{let a=bytes_new();let b=bytes_new();if !fill(a,65u8){{return 8}}let read=a;let edit=b;let f=()=>{{let view=bytes_view(read);bytes_reserve(edit,4);return i32(view[0])}};return f()-65}}"
    );
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw187.zil", &text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let mut module =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let mut changed = 0;
    for f in &mut module.functions {
        for b in &mut f.blocks {
            for i in &mut b.instructions {
                if let InstructionKind::MakeClosure { captures, .. } = &mut i.kind {
                    assert_eq!(captures.len(), 2);
                    captures[1] = captures[0];
                    changed += 1;
                }
            }
        }
    }
    assert_eq!(changed, 1);
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let dir = directory();
    fs::write(dir.join("input.zil"), text).unwrap();
    fs::write(dir.join("candidate.bin"), &bytes).unwrap();
    for (name, result) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &Limits::default()),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()),
        ),
    ] {
        let diagnostics = result.unwrap_err();
        fs::write(dir.join(format!("{name}.txt")), format!("{diagnostics:#?}")).unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject && d.code == "E-BORROW-CONFLICT"),
            "{name}: {diagnostics:?}"
        );
    }
}

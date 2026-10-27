//! Independent 174 review: unrelated field roots, empty heads, and duplicate store debt.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
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
        "review174-store-fields-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(files: &[(&str, &str)]) -> CompileReport {
    let dir = directory();
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}

fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    native_input(files, b"", stdout, exit);
}

fn native_input(files: &[(&str, &str)], input: &[u8], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = dir.join("program");
        let report = compile_project(
            &project(files),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let mut child = Command::new(&executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let output = child.wait_with_output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
    }
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review174.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn rejected_candidate(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    category: ResultCategory,
    code: &str,
    message: &str,
) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory();
    fs::write(dir.join(format!("{label}.bin")), &bytes).unwrap();
    for (path, errors) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ),
    ] {
        fs::write(
            dir.join(format!("{label}-{path}.txt")),
            format!("{errors:#?}\n"),
        )
        .unwrap();
        assert!(
            errors.iter().all(|d| d.phase == zil_base::Phase::Verify),
            "{label}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|d| d.category == category && d.code == code && d.message.contains(message)),
            "{label}: {errors:?}"
        );
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource Empty {}
impl Drop for Empty { fn drop(self:Self) { unsafe { putchar(101) } } }
resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Cell { manages empty:Empty; manages tag:Mark; n:i64; wide:[i32;3] }
package State { manages tag:Mark; manages left:Store<Cell>; manages right:Store<Cell> }
fn cell(code:i32)->Cell => Cell { empty:Empty{},tag:Mark{code:code},n:7,wide:[1,2,3] }
fn empty()->Store<Cell> contract{result(create)} { return store_new() }
fn state()->State {
    let left=empty(); store_reserve(left,2);
    store_push(left,cell(65)); store_push(left,cell(66));
    let right=empty(); store_reserve(right,1); store_push(right,cell(67));
    return State {tag:Mark{code:90},left:give left,right:give right}
}
"#;

#[test]
fn unrelated_field_description_survives_left_take_and_refill_with_empty_head_drop() {
    let source = format!(
        "{MODEL} fn main()->i32{{var current=state();let right=current.right;let detached=give current.left;let replacement=empty();store_reserve(replacement,1);store_push(replacement,cell(68));current.left=give replacement;store_reserve(right,2);store_push(right,cell(69));return i32(store_len(current.right))-2}}"
    );
    native(&[("main", &source)], b"eAeBZeDeCeE", 0);
}

#[test]
fn failed_capacity_request_keeps_field_prefix_and_pop_error_carrier_cleanup() {
    let source = format!(
        "{MODEL} fn main()->void!i32{{let current=state();if store_reserve(current.left,-1){{return 2}} if store_len(current.left)!=2{{return 3}} let popped=store_pop(current.left)?;return ()}}"
    );
    native(&[("main", &source)], b"eBZeAeC", 1);
}

#[test]
fn old_field_description_cannot_be_revived_by_refilling_same_slot() {
    let source = format!(
        "{MODEL} fn main()->i32{{var current=state();let old=current.left;let moved=give current.left;current.left=empty();return i32(store_len(old))}}"
    );
    let report = check(&[("main", &source)]);
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
            .any(|d| d.phase == zil_base::Phase::Analyze
                && d.code == "E-ANALYZE-IR"
                && d.message
                    .contains("invalidates a description required later")),
        "{}",
        report.to_json()
    );
    assert!(report.commands.is_empty());
}

#[test]
fn raw_wire_cannot_consume_one_store_twice_into_two_equal_typed_fields() {
    use zil_ir::{InstructionKind as I, Type};
    let source = format!(
        "{MODEL} fn main()->i32{{let current=state();return i32(store_len(current.left))+i32(store_len(current.right))-3}}"
    );
    let (mut module, plan) = candidate(&source);
    let mut modified = false;
    for function in &mut module.functions {
        for block in &mut function.blocks {
            for inst in &mut block.instructions {
                if let I::MakePackage { fields } = &mut inst.kind {
                    let stores:Vec<_>=fields.iter().enumerate().filter_map(|(index,value)| matches!(&function.values[value.0 as usize],Type::Managed(payload) if matches!(payload.as_ref(),Type::OwnedStore{..})).then_some(index)).collect();
                    if stores.len() == 2 {
                        fields[stores[1]] = fields[stores[0]];
                        modified = true;
                    }
                }
            }
        }
    }
    assert!(modified);
    rejected_candidate(
        "duplicate-store-in-distinct-fields",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
        "managed SSA value was not available exactly once",
    );
}

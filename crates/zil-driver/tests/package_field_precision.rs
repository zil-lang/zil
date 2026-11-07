//! Actual Package fields share parent lifetime, not their access footprint.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
use zil_ir::{HoldRoot, InstructionKind as I};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "field179-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
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
fn check(source: &str) -> CompileReport {
    let dir = directory("source");
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_project(
        &project(&[("main", source)]),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(files: &[(&str, &str)], expected: &[u8], code: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory("native");
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
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        fs::write(dir.join("execution.json"), format!("{{\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"expected_exit\":{code},\"stdout_matches\":{}}}\n", output.status.code().map_or("null".into(), |n| n.to_string()), output.stdout == expected)).unwrap();
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        assert_eq!(output.stdout, expected);
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages r:R;code:i32}
package Batch{manages items:Store<Item>;manages bytes:ByteBuffer}
package State<F>{manages items:Store<Item>;callback:F;hits:i32;manages bytes:ByteBuffer}
fn item(code:i32)->Item=>Item{r:R{code:code},code:code}
fn empty()->Store<Item> contract{result(create)}{return store_new()}
fn batch()->Batch{return Batch{items:empty(),bytes:bytes_new()}}
fn make(code:i32)->impl FnMut()->i32{let guard=R{code:code};var n=0;return move()=>{n+=1;return n+guard.code-code}}
fn pack<F:FnMut()->i32>(callback:F)->State<F>{let b=batch();return State::<F>{items:give b.items,callback:callback,hits:0,bytes:give b.bytes}}
fn read(value:&Item)->i32=>value.code
fn consume<F:FnMut()->i32>(value:State<F>){}
"#;

#[test]
fn callback_loan_allows_actual_sibling_store_and_copy_fields() {
    let source = format!(
        "{MODEL} fn main()->i32{{var state=pack(make(65));let cb=&mut state.callback;state.hits=7;if !store_reserve(state.items,2){{return 1}};store_push(state.items,item(66));let view=store_at(state.items,0);state.hits+=cb();return state.hits+read(view)-74}}"
    );
    native(&[("main", &source)], b"BA", 0);
}

#[test]
fn store_window_keeps_its_real_field_while_copy_and_callback_are_updated() {
    let source = format!(
        "{MODEL} fn main()->i32{{var state=pack(make(65));store_reserve(state.items,1);store_push(state.items,item(66));let view=store_at(state.items,0);state.hits=7;let cb=&mut state.callback;state.hits+=cb();state.hits+=cb();return state.hits+read(view)-76}}"
    );
    native(&[("main", &source)], b"BA", 0);
}

#[test]
fn callback_loan_survives_sibling_replacement_and_failing_preparation() {
    let source = format!(
        "{MODEL} fn main()->i32{{var state=pack(make(65));store_reserve(state.items,1);store_push(state.items,item(66));let cb=&mut state.callback;state.items=empty();state.bytes=bytes_new();bytes_reserve(state.bytes,2);bytes_push(state.bytes,9u8);state.hits=cb();return cb()+state.hits+i32(bytes_len(state.bytes))-4}}"
    );
    native(&[("main", &source)], b"BA", 0);
    let source = format!(
        "{MODEL}enum E{{Fail}}fn fail()->E!i32=>E.Fail;fn main()->E!i32{{var state=pack(make(65));store_reserve(state.items,1);store_push(state.items,item(66));let cb=&mut state.callback;state.hits=fail()?;return cb()}}"
    );
    native(&[("main", &source)], b"BA", 1);
}

#[test]
fn same_field_parent_movement_and_simultaneous_exclusive_use_still_reject() {
    for change in [
        "state.callback=make(67);",
        "let moved=state.callback;",
        "let moved=state;",
        "consume(state);",
        "let other=&mut state.callback;other();",
        "let other=&state.callback;",
    ] {
        let source = format!(
            "{MODEL} fn main()->i32{{var state=pack(make(65));let cb=&mut state.callback;{change}return cb()}}"
        );
        let report = check(&source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{change}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Verify,
            "{change}: {}",
            report.to_json()
        );
        assert_eq!(
            report.diagnostics[0].code,
            "E-BORROW-CONFLICT",
            "{change}: {}",
            report.to_json()
        );
    }
    // Whole-State assignment has an earlier existing responsibility boundary;
    // this slice does not add replacement cleanup for an unpaid whole State.
    let source = format!(
        "{MODEL} fn main()->i32{{var state=pack(make(65));let cb=&mut state.callback;state=pack(make(67));return cb()}}"
    );
    let report = check(&source);
    assert_eq!(report.status, ResultCategory::SafeReject);
    assert_eq!(report.phase, zil_base::Phase::Analyze);
    assert_eq!(report.diagnostics[0].code, "E-ANALYZE-IR");
    assert!(report.diagnostics[0].message.contains("unpaid resource"));
}

#[test]
fn shared_callback_and_callback_replacement_preserve_sibling_windows() {
    let shared =
        "fn shared(code:i32)->impl Fn()->i32{let guard=R{code:code};return move()=>guard.code}";
    let source = format!(
        "{MODEL}{shared} fn main()->i32{{var state=pack(shared(65));let cb=&state.callback;state.hits=7;store_reserve(state.items,1);store_push(state.items,item(66));let view=store_at(state.items,0);return cb()+state.hits+read(view)-138}}"
    );
    native(&[("main", &source)], b"BA", 0);
    let source = format!(
        "{MODEL} fn main()->i32{{var state=pack(make(65));store_reserve(state.items,1);store_push(state.items,item(66));let view=store_at(state.items,0);state.callback=make(67);let cb=&mut state.callback;return read(view)+cb()-67}}"
    );
    native(&[("main", &source)], b"ABC", 0);
}

#[test]
fn helper_effects_and_current_call_reject_real_overlap() {
    let helpers = r#"
fn grow(items:Store<Item>){store_reserve(items,4);}
fn relay(items:Store<Item>){grow(items);}
fn read_after_grow(view:&Item, items:Store<Item>)->i32{relay(items);return read(view)}
fn use_pair(left:Store<Item>,right:Store<Item>)->i32{let view=store_at(left,0);relay(right);return read(view)}
"#;
    let setup = "var state=pack(make(65));store_reserve(state.items,1);store_push(state.items,item(66));let cb=&mut state.callback;";
    for body in [
        "let view=store_at(state.items,0);relay(state.items);return cb()+read(view)",
        "return cb()+read_after_grow(store_at(state.items,0),state.items)",
        "return cb()+use_pair(state.items,state.items)",
    ] {
        let source = format!("{MODEL}{helpers}fn main()->i32{{{setup}{body}}}");
        let report = check(&source);
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
        assert_eq!(
            report.diagnostics[0].code,
            "E-BORROW-CONFLICT",
            "{}",
            report.to_json()
        );
    }
    let source = format!(
        "{MODEL}{helpers}fn main()->i32{{{setup}let view=store_at(state.items,0);let n=read(view);relay(state.items);state.hits=cb();return n+state.hits-67}}"
    );
    native(&[("main", &source)], b"BA", 0);
}

#[test]
fn borrowed_callback_capture_cannot_acquire_owning_field_authority() {
    let source = format!(
        "{MODEL}fn main()->i32{{let store=empty();let callback=()=>i32(store_len(store));let state=pack(callback);return 0}}"
    );
    let report = check(&source);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("self-contained")
                || d.message.contains("Store")
                || d.message.contains("buffer")),
        "{}",
        report.to_json()
    );
}

#[test]
fn generic_state_body_uses_same_field_precision() {
    let source = format!(
        "{MODEL} fn update<G:FnMut()->i32>(state:State<G>)->State<G>{{var next=state;let cb=&mut next.callback;next.hits=10;next.bytes=bytes_new();next.hits+=cb();next.hits+=cb();return next}}fn main()->i32{{let state=update(pack(make(65)));return state.hits-13}}"
    );
    native(&[("main", &source)], b"A", 0);
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw179.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let limits = Limits::default();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &TargetProfile::host_llvm(),
        &limits,
    )
    .unwrap();
    (module, plan)
}
fn verify_pair(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    expected: Option<&str>,
) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory(label);
    fs::write(dir.join("candidate.bin"), &bytes).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ),
    ] {
        match expected {
            Some(code) => {
                let errors = result.unwrap_err();
                fs::write(dir.join(format!("{route}.txt")), format!("{errors:#?}\n")).unwrap();
                assert_eq!(errors[0].category, ResultCategory::SafeReject);
                assert_eq!(errors[0].code, code, "{label}: {errors:#?}");
            }
            None => {
                result.unwrap();
                fs::write(dir.join(format!("{route}.txt")), "Pass\n").unwrap();
            }
        }
    }
}

#[test]
fn raw_wire_rebuild_field_origin_and_keep_parent_hold() {
    let source = format!(
        "{MODEL} fn main()->i32{{var state=pack(make(65));let cb=&mut state.callback;state.hits=10;return cb()+state.hits-11}}"
    );
    let (module, plan) = candidate(&source);
    let (fi, vi, local) = module
        .functions
        .iter()
        .enumerate()
        .find_map(|(fi, f)| {
            f.blocks.iter().flat_map(|b| &b.instructions).find_map(|i| {
                if let I::BorrowPackageField { local, .. } = i.kind {
                    Some((fi, i.result.unwrap().0 as usize, local))
                } else {
                    None
                }
            })
        })
        .unwrap();
    assert_eq!(
        plan.functions[fi].holds[vi].roots,
        vec![HoldRoot::Local(local)]
    );
    verify_pair("valid", module.clone(), plan.clone(), None);
    let mut forged = plan.clone();
    forged.functions[fi].holds[vi].roots.clear();
    verify_pair(
        "erase-parent-hold",
        module.clone(),
        forged,
        Some("E-VERIFY-HOLD"),
    );
    let mut changed = module;
    for i in changed.functions[fi]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if let I::BorrowPackageField { index, .. } = &mut i.kind {
            *index = u32::MAX;
        }
    }
    verify_pair(
        "invalid-field-index",
        changed,
        plan,
        Some("E-VERIFY-PACKAGE"),
    );
}

#[test]
fn raw_wire_cannot_hide_a_current_overlapping_callback_loan() {
    let source = format!(
        "{MODEL}fn main()->i32{{var state=pack(make(65));let first=&mut state.callback;let n=first();let second=&mut state.callback;return n+second()-3}}"
    );
    let (mut module, plan) = candidate(&source);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut first = None;
    let mut changed = 0;
    for instruction in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::CallClosureBorrowed { callee, .. } = &mut instruction.kind {
            if let Some(old) = first {
                *callee = old;
                changed += 1;
            } else {
                first = Some(*callee);
            }
        }
    }
    assert_eq!(changed, 1);
    verify_pair(
        "extend-first-loan-through-second",
        module,
        plan,
        Some("E-BORROW-CONFLICT"),
    );
}

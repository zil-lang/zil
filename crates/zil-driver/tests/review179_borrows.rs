//! Independent 179 review: actual field types, current-call aliasing, and parent lifetime.
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
fn reviewer_copy_aggregate_replacement_preserves_callback_and_store_windows() {
    let model = MODEL
        .replace(
            "package State<F>",
            "struct Stamp{lo:i32;hi:i32}package State<F>",
        )
        .replace("callback:F;hits:i32;", "callback:F;hits:i32;stamp:Stamp;")
        .replace("hits:0,bytes:", "hits:0,stamp:Stamp{lo:1,hi:2},bytes:");
    let source = format!(
        "{model}fn main()->i32{{var state=pack(make(65));store_reserve(state.items,1);store_push(state.items,item(66));let view=store_at(state.items,0);let cb=&mut state.callback;state.stamp=Stamp{{lo:9,hi:10}};state.bytes=bytes_new();return read(view)+state.stamp.lo+state.stamp.hi+cb()-86}}"
    );
    native(&[("main", &source)], b"BA", 0);
}

#[test]
fn reviewer_raw_wire_current_argument_cannot_alias_a_transitive_helper_write() {
    let helpers = "fn grow(items:Store<Item>){store_reserve(items,4)}fn transit(items:Store<Item>){grow(items)}fn after(view:&Item,items:Store<Item>)->i32{transit(items);return read(view)}";
    let source = format!(
        "{MODEL}{helpers}fn main()->i32{{var state=pack(make(65));store_reserve(state.items,1);store_push(state.items,item(66));let other=empty();let cb=&mut state.callback;let view=store_at(state.items,0);return after(view,other)+cb()-67}}"
    );
    // The borrowed view is used for the last time as a current call argument.
    // Rebuilding only future liveness would miss the hidden write in transit.
    native(&[("main", &source)], b"BA", 0);
    let report = check(&source.replace("after(view,other)", "after(view,state.items)"));
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Verify);
    assert_eq!(report.diagnostics[0].code, "E-BORROW-CONFLICT");
    let (mut module, plan) = candidate(&source);
    let callee = module
        .functions
        .iter()
        .find(|f| f.name == "after")
        .unwrap()
        .id;
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let actual_store = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let I::OwnedStoreBorrow { store, .. } = i.kind {
                Some(store)
            } else {
                None
            }
        })
        .unwrap();
    let mut changed = 0;
    for i in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::Call {
            function,
            arguments,
        } = &mut i.kind
            && *function == callee
        {
            assert_eq!(arguments.len(), 2);
            arguments[1] = actual_store;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    verify_pair(
        "review-helper-current-alias",
        module,
        plan,
        Some("E-BORROW-CONFLICT"),
    );
}

#[test]
fn reviewer_raw_wire_valid_copy_index_cannot_masquerade_as_callback() {
    let source = format!(
        "{MODEL}fn main()->i32{{var state=pack(make(65));let cb=&mut state.callback;state.hits=10;return cb()+state.hits-11}}"
    );
    let (mut module, plan) = candidate(&source);
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let (local, value) = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let I::BorrowPackageField { local, .. } = i.kind {
                Some((local, i.result.unwrap()))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        plan.functions[main.id.0 as usize].holds[value.0 as usize].roots,
        vec![HoldRoot::Local(local)]
    );
    let zil_ir::Type::Package { fields, .. } = &main.locals[local.0 as usize].ty else {
        panic!("real State required")
    };
    let copy_index = fields
        .iter()
        .position(|(_, ty)| matches!(ty, zil_ir::Type::I32))
        .unwrap() as u32;
    for i in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::BorrowPackageField { index, .. } = &mut i.kind {
            *index = copy_index;
        }
    }
    verify_pair(
        "review-copy-index-is-not-callback",
        module,
        plan,
        Some("E-VERIFY-TYPE"),
    );
}

#[test]
fn reviewer_raw_wire_whole_parent_cleanup_cannot_cross_live_callback_loan() {
    let source = format!(
        "{MODEL}fn main()->i32{{var state=pack(make(65));let cb=&mut state.callback;return cb()-1}}"
    );
    let (mut module, plan) = candidate(&source);
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut moved = false;
    for b in &mut main.blocks {
        let Some((borrow_index, local)) = b.instructions.iter().enumerate().find_map(|(n, i)| {
            if let I::BorrowPackageField { local, .. } = i.kind {
                Some((n, local))
            } else {
                None
            }
        }) else {
            continue;
        };
        let drop_index = b
            .instructions
            .iter()
            .position(|i| matches!(i.kind,I::DropPackage{local:l} if l==local))
            .unwrap();
        assert!(drop_index > borrow_index);
        let cleanup = b.instructions.remove(drop_index);
        b.instructions.insert(borrow_index + 1, cleanup);
        moved = true;
    }
    assert!(moved);
    verify_pair(
        "review-parent-drop-before-call",
        module,
        plan,
        Some("E-BORROW-CONFLICT"),
    );
}

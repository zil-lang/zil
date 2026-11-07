//! Concrete owning callbacks in complete Package state, with actual inline calls.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
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
        "callable-fields176-{}-{}",
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
        let out = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(exit), "{out:?}");
        assert_eq!(out.stdout, stdout);
    }
}
const STATE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<F>{manages ticket:Ticket;callback:F;hits:i32}
fn pack<F:FnMut()->i32>(callback:F, code:i32)->State<F>{return State::<F>{ticket:Ticket{code:code},callback:callback,hits:0}}
fn tick<G:FnMut()->i32>(state:State<G>)->State<G>{var next=state;let cb=&mut next.callback;next.hits=cb();return next}
fn make(n:i32)->impl FnMut()->i32{var value=n;return move ()=>{value+=1;return value}}
"#;
#[test]
fn whole_state_parameters_and_results_keep_original_fnmut_environment() {
    let source = format!(
        "{STATE} fn main()->i32{{var state=pack(make(40),65);state=tick(state);if state.hits!=41{{return 1}};state=tick(state);return state.hits-42}}"
    );
    native(&[("main", &source)], b"A", 0);
}

const OWNED: &str = r#"
fn owned(n:i32)->impl FnMut()->i32{let guard=Ticket{code:n};var value=n;return move()=>{value+=1;return value+guard.code-n}}
fn forward(n:i32)->impl FnMut()->i32{return owned(n)}
fn nested(n:i32)->impl FnMut()->i32{var child=owned(n);return move()=>child()}
"#;
#[test]
fn field_take_refill_replace_and_nested_cleanup_preserve_one_environment() {
    let source = format!(
        "{STATE}{OWNED} fn main()->i32{{var state=pack(nested(65),84);let cb=&mut state.callback;if cb()!=66||cb()!=67{{return 1}};var old=state.callback;state.callback=nested(66);let next=&mut state.callback;return old()+next()-135}}"
    );
    native(&[("main", &source)], b"ATB", 0);
    let source = format!(
        "{STATE}{OWNED} fn main()->i32{{var state=pack(owned(65),84);state.callback=owned(66);let cb=&mut state.callback;return cb()-67}}"
    );
    native(&[("main", &source)], b"ATB", 0);
}
#[test]
fn failed_preparation_keeps_old_field_and_unwinds_prepared_argument() {
    let source = format!(
        "{STATE}{OWNED} enum E{{Fail}}fn fail()->E!i32{{return E.Fail}}fn main()->E!i32{{var state=pack(owned(65),84);state.callback=owned(fail()?);return 0}}"
    );
    native(&[("main", &source)], b"TA", 1);
    let source = format!(
        "{STATE}{OWNED} enum E{{Fail}}fn fail()->E!i32{{return E.Fail}}fn main()->E!i32{{let state=pack(owned(65),fail()?);return 0}}"
    );
    native(&[("main", &source)], b"A", 1);
}
#[test]
fn fnonce_take_consumes_and_partial_state_exit_cleans_only_remaining_fields() {
    let source = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<F>{manages ticket:R;callback:F}
fn pack<F:FnOnce()->i32>(callback:F)->State<F>{return State::<F>{ticket:R{code:84},callback:callback}}
fn make(n:i32)->impl FnOnce()->i32{let r=R{code:n};return move()=>{let value=give r;return value.code}}
fn main()->i32{let state=pack(make(65));let once=state.callback;return once()-65}
"#;
    native(&[("main", source)], b"AT", 0);
}
#[test]
fn public_state_generic_api_preserves_identity_across_modules_and_composition() {
    let model = STATE
        .replace("resource Ticket", "pub resource Ticket")
        .replace("package State<F>", "pub package State<F>")
        .replace("callback:F;hits:i32", "pub callback:F;pub hits:i32")
        .replace("fn pack<", "pub fn pack<")
        .replace("fn tick<", "pub fn tick<")
        .replace("fn make(", "pub fn make(");
    let model = format!(
        "{model} pub fn twice<H:FnMut()->i32>(state:State<H>)->State<H>{{return tick(tick(state))}}"
    );
    native(
        &[
            (
                "main",
                "import model;fn main()->i32{let state=model.twice(model.pack(model.make(40),65));return state.hits-42}",
            ),
            ("model", &model),
        ],
        b"A",
        0,
    );
}
#[test]
fn different_source_opaque_factories_and_moved_fields_are_rejected() {
    for body in [
        "var state=pack(owned(65),84);state.callback=forward(66);return 0",
        "var state=pack(owned(65),84);state=pack(forward(66),85);return 0",
        "let state=pack(make(1),84);let old=state.callback;let again=state.callback;return 0",
        "let state=pack(make(1),84);let old=state.callback;let whole=state;return 0",
        "var state=pack(make(1),84);let cb=&mut state.callback;state.callback=make(2);return cb()",
        "let state=pack(make(1),84);let cb=&mut state.callback;return cb()",
        "let f=make(1);let state=pack(f,84);let duplicate=pack(f,85);return 0",
    ] {
        let source = format!("{STATE}{OWNED} fn main()->i32{{{body}}}");
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{body}: {}",
            report.to_json()
        );
        assert_ne!(
            report.phase,
            zil_base::Phase::Parse,
            "{body}: {}",
            report.to_json()
        );
    }
}
#[test]
fn borrowed_environments_cannot_enter_retained_state() {
    let source =
        format!("{STATE} fn main(){{var n=1;let cb=()=>{{n+=1;return n}};let state=pack(cb,84)}}");
    let report = check(&[("main", &source)]);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_ne!(report.phase, zil_base::Phase::Parse, "{}", report.to_json());
}
#[test]
fn generic_state_api_can_take_call_refill_and_return_complete_state() {
    let source = format!(
        "{STATE}{OWNED} fn transfer<F:FnMut()->i32>(state:State<F>)->State<F>{{var next=state;var cb=next.callback;next.hits=cb();next.callback=cb;return next}}fn main()->i32{{var state=pack(owned(65),84);state=transfer(state);state=tick(state);return state.hits-67}}"
    );
    native(&[("main", &source)], b"TA", 0);
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw176.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}
fn rejected(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    category: ResultCategory,
) {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
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
            errors.iter().any(|d| d.category == category),
            "{label}: {errors:?}"
        );
    }
}
#[test]
fn raw_and_wire_reconstruct_field_permission_liveness_and_unique_publication() {
    use zil_ir::{InstructionKind as I, Type};
    let source = format!(
        "{STATE}fn main()->i32{{let first=make(1);var state=pack(first,65);let p=&mut state.callback;let n=p();state.callback=make(3);let q=&mut state.callback;let m=q();var old=state.callback;state.callback=make(4);return old()-m}}"
    );
    let (original, plan) = candidate(&source);
    for variant in 0..7 {
        let mut module = original.clone();
        let mut changed = 0;
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let taken = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| {
                matches!(i.kind, I::TakePackageField { .. })
                    .then_some(i.result)
                    .flatten()
            })
            .unwrap();
        let mut first = None;
        for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            match &mut i.kind {
                I::BorrowPackageField { index, .. } if variant == 0 && changed == 0 => {
                    *index = u32::MAX;
                    changed += 1;
                }
                I::BorrowPackageField { mutable, .. } if variant == 1 && changed == 0 => {
                    *mutable = false;
                    changed += 1;
                }
                I::CallClosureBorrowed { callee, .. } if variant == 2 => {
                    if let Some(p) = first {
                        *callee = p;
                        changed += 1;
                    } else {
                        first = Some(*callee);
                    }
                }
                I::TakePackageField { local, index } if variant == 3 => {
                    i.kind = I::ReadPackageField {
                        local: *local,
                        index: *index,
                    };
                    changed += 1;
                }
                I::DropPackageField { index, .. } if variant == 4 && changed == 0 => {
                    *index = 0;
                    changed += 1;
                }
                I::TakeLocal { local }
                    if variant == 5
                        && changed == 0
                        && matches!(f.locals[local.0 as usize].ty, Type::Closure(_)) =>
                {
                    i.kind = I::Load(*local);
                    changed += 1;
                }
                I::StorePackageField { value, .. } if variant == 6 => {
                    if first.is_some() {
                        *value = taken;
                        changed += 1;
                    } else {
                        first = Some(*value);
                    }
                }
                _ => {}
            }
        }
        assert_eq!(changed, 1, "variant {variant}");
        rejected(
            [
                "outside-field",
                "false-permission",
                "stale-address",
                "copy-field",
                "wrong-cleanup",
                "load-publication",
                "reuse-field-ssa",
            ][variant],
            module,
            plan.clone(),
            ResultCategory::SafeReject,
        );
    }
}
#[test]
fn raw_and_wire_cannot_publish_the_same_resource_free_callback_twice() {
    use zil_ir::InstructionKind as I;
    let source = format!(
        "{STATE}fn main(){{let first=make(1);let second=make(2);let a=pack(first,65);let b=pack(second,66)}}"
    );
    let (mut module, plan) = candidate(&source);
    let helper = module
        .functions
        .iter()
        .find(|f| f.name.contains("helper") && f.parameters.len() == 2)
        .unwrap()
        .id;
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut first = None;
    let mut changed = 0;
    for i in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::Call {
            function,
            arguments,
        } = &mut i.kind
            && *function == helper
        {
            if let Some(value) = first {
                arguments[0] = value;
                changed += 1;
            } else {
                first = Some(arguments[0]);
            }
        }
    }
    assert_eq!(changed, 1);
    rejected(
        "duplicate-pure-callback",
        module,
        plan,
        ResultCategory::SafeReject,
    );
}
#[test]
fn exact_raw_and_wire_budget_boundary_is_deterministic_for_nested_callback_state() {
    let source = format!(
        "{STATE}{OWNED}fn main()->i32{{var state=pack(nested(65),84);let cb=&mut state.callback;return cb()-66}}"
    );
    let (module, plan) = candidate(&source);
    let defaults = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &defaults).unwrap();
    let mut notes = String::new();
    for wire in [false, true] {
        let run = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            };
            if wire {
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
            }
        };
        let (mut low, mut high) = (1, defaults.max_analysis_steps);
        run(high).unwrap();
        while low < high {
            let mid = low + (high - low) / 2;
            if run(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        for _ in 0..2 {
            run(low).unwrap();
            let errors = run(low - 1).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|e| e.category == ResultCategory::ResourceLimit),
                "{errors:?}"
            );
        }
        notes.push_str(&format!(
            "wire={wire} minimum_success={low} nearest_reject={}\n",
            low - 1
        ));
    }
    fs::write(directory().join("budget-boundary.txt"), notes).unwrap();
}
#[test]
fn bytebuffer_environment_survives_field_calls_and_state_transfer() {
    let source = format!(
        "{STATE}fn bytes()->impl FnMut()->i32{{let buffer=bytes_new();bytes_reserve(buffer,4);bytes_push(buffer,7u8);return move()=>{{let n=bytes_len(buffer);bytes_push(buffer,9u8);return i32(n)}}}}fn main()->i32{{var state=pack(bytes(),84);state=tick(state);if state.hits!=1{{return 1}};state=tick(state);return state.hits-2}}"
    );
    native(&[("main", &source)], b"T", 0);
}

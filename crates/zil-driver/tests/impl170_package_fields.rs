//! Complete owning array Package fields retain nested responsibility through native code.
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
        "package-fields170-{}-{}",
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
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
    }
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw170.zil", source);
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
            errors.iter().any(|d| d.category == category),
            "{label}: {errors:?}"
        );
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R { code:i32 }
impl Drop for R { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Item { manages item:R; n:i32 }
package State { manages tag:R; items:[Item;2]; n:i32 }
fn item(n:i32)->Item => Item { item:R{code:n}, n:n }
fn state(tag:i32,a:i32,b:i32)->State => State{tag:R{code:tag},items:[item(a),item(b)],n:4}
"#;

#[test]
fn complete_fields_construct_return_and_drop_in_declaration_order() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn relay(s:State)->State=>s;fn main()->i32{{let s=relay(state(90,65,66));return s.n-4}}"
            ),
        )],
        b"ZAB",
        0,
    );
}

#[test]
fn field_take_local_element_refill_and_parent_refill_are_exact() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn main()->i32{{var s=state(90,65,66);var items=s.items;let old=items[0];items[0]=item(67);s.items=items;return old.n-65}}"
            ),
        )],
        b"AZCB",
        0,
    );
}

#[test]
fn replacing_a_complete_array_field_cleans_the_old_field_first() {
    native(
        &[(
            "main",
            &format!("{MODEL} fn main(){{var s=state(90,65,66);s.items=[item(67),item(68)]}}"),
        )],
        b"ABZCD",
        0,
    );
}

#[test]
fn failed_preparation_cleans_only_actual_temporaries_and_keeps_the_old_field() {
    let failures = "enum E{Fail} fn fail()->E!i32=>E.Fail;";
    native(
        &[(
            "main",
            &format!(
                "{MODEL}{failures} fn main()->E!i32{{var s=state(90,65,66);s.items=[item(67),item(fail()?)];return 0}}"
            ),
        )],
        b"CZAB",
        1,
    );
    native(
        &[(
            "main",
            &format!(
                "{MODEL}{failures} fn main()->E!i32{{let s=State{{tag:R{{code:90}},items:[item(65),item(fail()?)],n:4}};return 0}}"
            ),
        )],
        b"AZ",
        1,
    );
}

#[test]
fn missing_parent_field_cleans_remaining_fields_and_moved_array_once() {
    native(
        &[(
            "main",
            &format!("{MODEL} fn main(){{let s=state(90,65,66);let items=s.items}}"),
        )],
        b"ABZ",
        0,
    );
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn refill(s:State)->State{{var x=s;let old=x.items;x.items=[item(67),item(68)];return x}} fn main(){{let s=refill(state(90,65,66))}}"
            ),
        )],
        b"ABZCD",
        0,
    );
}

#[test]
fn repeated_take_refill_preserves_parent_cleanup_position() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn main(){{var s=state(90,65,66);var i=0;while i<3{{var items=s.items;let old=items[0];items[0]=item(old.n+1);s.items=items;i+=1}}}}"
            ),
        )],
        b"ABCZDB",
        0,
    );
}

#[test]
fn arrays_and_both_error_union_payloads_recursively_clean_fields() {
    native(
        &[(
            "main",
            &format!("{MODEL} fn main(){{let states=[state(90,65,66),state(89,67,68)]}}"),
        )],
        b"ZABYCD",
        0,
    );
    native(
        &[(
            "main",
            &format!(
                "{MODEL} enum E{{Fail}} fn get()->E!State=>state(90,65,66); fn main(){{let result=get()}}"
            ),
        )],
        b"ZAB",
        0,
    );
    native(
        &[(
            "main",
            &format!("{MODEL} fn main()->State!i32{{return state(90,65,66)}}"),
        )],
        b"ZAB",
        1,
    );
    native(
        &[(
            "main",
            &format!(
                "{MODEL} enum E{{Fail}} fn get()->E!State=>state(90,65,66); fn main()->E!i32{{let result=get()?;return result.n-4}}"
            ),
        )],
        b"ZAB",
        0,
    );
}

#[test]
fn public_field_roundtrip_crosses_nominal_module_boundaries() {
    native(
        &[
            (
                "main",
                "import model;import flow;fn main()->i32{var s=flow.relay(model.make());let old=s.items;s.items=[model.item(67),model.item(68)];return old[0].n+old[1].n-131}",
            ),
            (
                "model",
                r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}pub package Item{manages r:R;pub n:i32}pub package State{manages tag:R;pub items:[Item;2]}pub fn item(n:i32)->Item=>Item{r:R{code:n},n:n};pub fn make()->State=>State{tag:R{code:90},items:[item(65),item(66)]}"#,
            ),
            (
                "flow",
                "import model;pub fn relay(s:model.State)->model.State=>s",
            ),
        ],
        b"ABZCD",
        0,
    );
}

#[test]
fn incomplete_and_reused_field_carriers_are_rejected() {
    for (body, code, message) in [
        (
            "let s=state(90,65,66);let first=s.items;let second=s.items;return 0",
            "E-ANALYZE-IR",
            "package field take requires that field live",
        ),
        (
            "let s=state(90,65,66);let first=s.items;let moved=s;return 0",
            "E-ANALYZE-IR",
            "whole aggregate move requires every owned element or field live",
        ),
        (
            "var s=state(90,65,66);let items=s.items;s.items=items;s.items=items;return 0",
            "E-UNINITIALIZED",
            "is not initialized on every incoming path",
        ),
        (
            "var s=state(90,65,66);let items=s.items;let old=items[0];s.items=items;return 0",
            "E-ANALYZE-IR",
            "whole aggregate move requires every owned element or field live",
        ),
    ] {
        let report = check(&[("main", &format!("{MODEL} fn main()->i32{{{body}}}"))]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, zil_base::Phase::Analyze);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == code && d.message.contains(message)),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn nested_partial_places_and_retained_parent_captures_remain_explicitly_closed() {
    for source in [
        format!("{MODEL} fn main()->i32{{let s=state(90,65,66);return s.items[0].n}}"),
        format!("{MODEL} fn main()->i32{{let s=state(90,65,66);let f=move()=>s.n;return f()}}"),
        format!("{MODEL} package Outer{{manages tag:R;states:[State;2]}} fn main(){{}}"),
    ] {
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, zil_base::Phase::Types, "{}", report.to_json());
    }
}

#[test]
fn raw_and_wire_cannot_copy_retake_or_overwrite_owned_fields() {
    use zil_ir::InstructionKind;
    let source = format!(
        "{MODEL} fn main(){{var s=state(90,65,66);let old=s.items;s.items=[item(67),item(68)]}}"
    );
    let (original, plan) = candidate(&source);
    for variant in 0..4 {
        let mut forged = original.clone();
        let mut changed = 0;
        for f in &mut forged.functions {
            if f.name != "main" {
                continue;
            }
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                match &mut i.kind {
                    InstructionKind::TakePackageField { local, index } if variant == 0 => {
                        i.kind = InstructionKind::ReadPackageField {
                            local: *local,
                            index: *index,
                        };
                        changed += 1;
                    }
                    InstructionKind::TakePackageField { index, .. } if variant == 1 => {
                        *index = u32::MAX;
                        changed += 1;
                    }
                    InstructionKind::DropPackageField { index, .. } if variant == 2 => {
                        *index = 0;
                        changed += 1;
                    }
                    InstructionKind::TakePackageField { local, .. } if variant == 3 => {
                        i.kind = InstructionKind::Load(*local);
                        changed += 1;
                    }
                    _ => {}
                }
            }
        }
        // The source takes its old field before refilling, so turn variant 2
        // into a live-field replacement in a separate valid candidate below.
        if variant == 2 {
            continue;
        }
        assert_eq!(changed, 1);
        rejected_candidate(
            [
                "copy-owned-field",
                "out-of-bounds-field",
                "wrong-field-drop",
                "load-parent-as-array",
            ][variant],
            forged,
            plan.clone(),
            ResultCategory::SafeReject,
        );
    }
    let (mut forged, plan) = candidate(&format!(
        "{MODEL} fn main(){{var s=state(90,65,66);s.items=[item(67),item(68)]}}"
    ));
    let mut changed = 0;
    for f in &mut forged.functions {
        for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::DropPackageField { index, .. } = &mut i.kind {
                *index = 0;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    rejected_candidate(
        "cleanup-wrong-parent-field",
        forged,
        plan,
        ResultCategory::SafeReject,
    );
}

#[test]
fn raw_and_wire_owning_field_cannot_hide_in_capture_metadata() {
    use zil_ir::Type;
    let (mut module, plan) = candidate(&format!(
        "{MODEL} fn main()->i32{{let s=state(90,65,66);let n=4;let f=move()=>n;return f()-4}}"
    ));
    let state = module
        .functions
        .iter()
        .flat_map(|f| &f.locals)
        .find_map(|l| {
            matches!(&l.ty,Type::Package{name,..} if name.ends_with(":State"))
                .then_some(l.ty.clone())
        })
        .unwrap();
    module.closures[0].captures[0] = state;
    rejected_candidate(
        "retained-nested-field-metadata",
        module,
        plan,
        ResultCategory::Unsupported,
    );
}

#[test]
fn raw_and_wire_charge_outer_times_inner_array_layout() {
    use zil_ir::Type;
    let source = "resource R{n:i32} package Item{manages r:R} package State{manages tag:R;items:[Item;2]} fn consume(s:[State;2]){} fn main(){}";
    let (original, plan) = candidate(source);
    let mut huge = original.clone();
    let mut changed = 0;
    for f in &mut huge.functions {
        for ty in f.locals.iter_mut().map(|l| &mut l.ty).chain(&mut f.values) {
            if let Type::Array { element, length } = ty
                && let Type::Package { fields, .. } = element.as_mut()
                && let Type::Array { length: inner, .. } = &mut fields[1].1
            {
                *length = 4096;
                *inner = 4096;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    rejected_candidate(
        "nested-layout-multiplication",
        huge,
        plan.clone(),
        ResultCategory::ResourceLimit,
    );
    let mut control = original;
    for f in &mut control.functions {
        for ty in f.locals.iter_mut().map(|l| &mut l.ty).chain(&mut f.values) {
            if let Type::Array { element, length } = ty
                && let Type::Package { fields, .. } = element.as_mut()
                && let Type::Array { length: inner, .. } = &mut fields[1].1
            {
                *length = 16;
                *inner = 16;
            }
        }
    }
    let mut plan = plan;
    plan.module_fingerprint = control.fingerprint();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let bytes = zil_ir::wire::encode_module_candidate(&control, &plan, &limits).unwrap();
    zil_verify::verify_module(control, plan, &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}

#[test]
fn raw_and_wire_budget_boundary_is_exact_for_nested_field_module() {
    let source = format!(
        "{MODEL} fn main(){{var s=state(90,65,66);let old=s.items;s.items=[item(67),item(68)]}}"
    );
    let (module, plan) = candidate(&source);
    let defaults = Limits::default();
    let target = TargetProfile::host_llvm();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &defaults).unwrap();
    for wire in [false, true] {
        let run = |limit| {
            let limits = Limits {
                max_analysis_steps: limit,
                ..defaults.clone()
            };
            if wire {
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
            }
        };
        let (mut low, mut high) = (0, defaults.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if run(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        assert!(low > 0);
        run(low).unwrap();
        let errors = run(low - 1).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        let dir = directory();
        fs::write(
            dir.join(if wire {
                "wire-budget.txt"
            } else {
                "raw-budget.txt"
            }),
            format!("accepted={low}\nrejected={}\n{errors:#?}\n", low - 1),
        )
        .unwrap();
    }
}

#[test]
fn conditional_missing_field_and_self_replacement_keep_exact_flags() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn choose(flag:bool){{var s=state(90,65,66);if flag{{let a=s.items}}else{{s.items=s.items}}}} fn main(){{choose(true);choose(false)}}"
            ),
        )],
        b"ABZZAB",
        0,
    );
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn main()->i32{{let f=()=>{{let s=state(90,65,66);let a=s.items;return a[0].n+a[1].n-131}};return f()}}"
            ),
        )],
        b"ABZ",
        0,
    );
}

#[test]
fn raw_and_wire_consume_array_inputs_once_and_reconstruct_field_state() {
    use zil_ir::{InstructionKind, Type};
    let source = format!(
        "{MODEL} package Pair{{manages tag:R;first:[Item;2];second:[Item;2]}}fn main(){{let pair=Pair{{tag:R{{code:90}},first:[item(65),item(66)],second:[item(67),item(68)]}};let first=pair.first;let second=pair.second}}"
    );
    let (original, plan) = candidate(&source);
    for duplicate_input in [true, false] {
        let mut forged = original.clone();
        let mut changed = 0;
        for f in &mut forged.functions {
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                match &mut i.kind {
                    InstructionKind::MakePackage{fields} if duplicate_input && i.result.is_some_and(|v|matches!(&f.values[v.0 as usize],Type::Package{name,..} if name.ends_with(":Pair")))=>{fields[2]=fields[1];changed+=1;},
                    InstructionKind::TakePackageField{index,..} if !duplicate_input && *index==2=>{*index=1;changed+=1;},
                    _=>{}
                }
            }
        }
        assert_eq!(changed, 1);
        rejected_candidate(
            if duplicate_input {
                "duplicate-array-input"
            } else {
                "retake-one-array-field"
            },
            forged,
            plan.clone(),
            ResultCategory::SafeReject,
        );
    }
}

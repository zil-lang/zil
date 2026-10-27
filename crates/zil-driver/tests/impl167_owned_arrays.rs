//! Finite owning arrays retain per-element responsibility through real native code.
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
        "owned-arrays167-{}-{}",
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

const PACKAGE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
fn make(code:i32)->P=>P{r:R{code:code},n:code}
"#;

#[test]
fn construction_and_copy_projection_clean_each_element_once() {
    let source = format!(
        "{PACKAGE} fn main()->i32{{let a=[make(65),make(66)];return a[0].n+a[1].r.code-131}}"
    );
    native(&[("main", &source)], b"AB", 0);
}

#[test]
fn whole_array_roundtrip_and_move_keep_inline_owners() {
    let source = format!(
        "{PACKAGE} fn relay(a:[P;2])->[P;2]=>a;fn main()->i32{{let a=relay([make(65),make(66)]);let b=a;return b[0].n+b[1].n-131}}"
    );
    native(&[("main", &source)], b"AB", 0);
}

#[test]
fn taking_and_refilling_an_element_preserves_unique_cleanup() {
    let source = format!(
        "{PACKAGE} fn main()->i32{{var a=[make(65),make(66)];let old=a[0];a[0]=make(67);return old.n+a[0].n+a[1].n-198}}"
    );
    native(&[("main", &source)], b"ACB", 0);
}

#[test]
fn replacement_cleans_previous_element_before_installing_prepared_value() {
    let source = format!(
        "{PACKAGE} fn main()->i32{{var a=[make(65),make(66)];a[0]=make(67);return a[0].n+a[1].n-133}}"
    );
    native(&[("main", &source)], b"ACB", 0);
}

#[test]
fn partial_array_exit_and_branch_join_clean_only_live_elements() {
    let source = format!(
        "{PACKAGE} fn run(take:bool)->i32{{let a=[make(65),make(66)];if take{{let old=a[0];return old.n-65}};return a[1].n-66}}fn main()->i32{{return run(true)+run(false)}}"
    );
    native(&[("main", &source)], b"ABAB", 0);
}

#[test]
fn prepared_prefix_is_cleaned_in_reverse_order_on_later_error() {
    let source = format!(
        "{PACKAGE} enum E{{Fail}}fn fail()->E!i32{{return E.Fail}}fn main()->E!i32{{let a=[make(65),make(66),make(fail()?)];return a[0].n}}"
    );
    native(&[("main", &source)], b"BA", 1);
}

#[test]
fn failed_replacement_keeps_the_old_element_until_scope_cleanup() {
    let source = format!(
        "{PACKAGE} enum E{{Fail}}fn fail()->E!i32{{return E.Fail}}fn main()->E!i32{{var a=[make(65),make(66)];a[0]=make(fail()?);return a[0].n}}"
    );
    native(&[("main", &source)], b"AB", 1);
}

#[test]
fn package_fields_clean_in_declaration_order_inside_each_array_element() {
    native(
        &[(
            "main",
            r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Pair{manages first:R;manages second:R}
fn main(){let a=[Pair{first:R{code:65},second:R{code:66}},Pair{first:R{code:67},second:R{code:68}}]}
"#,
        )],
        b"ABCD",
        0,
    );
}

#[test]
fn byte_buffer_field_windows_use_the_actual_inline_element() {
    native(
        &[(
            "main",
            r#"
package P{manages b:ByteBuffer}
fn make(n:u8)->P{let b=bytes_new();if !bytes_reserve(b,1)||!bytes_push(b,n){return P{b:give b}};return P{b:give b}}
fn main()->i32{var a=[make(20u8),make(22u8)];let v=bytes_view(a[0].b);if v[0]!=20u8{return 1};a[0]=make(21u8);let w=bytes_view(a[1].b);return i32(w[0])-22}
"#,
        )],
        b"",
        0,
    );
}

#[test]
fn private_array_element_fields_remain_private_across_modules() {
    let report = check(&[
        (
            "main",
            "import model;fn main()->i32{let a=[model.make(),model.make()];return a[0].secret}",
        ),
        (
            "model",
            "pub resource R{pub code:i32}pub package P{manages r:R;secret:i32}pub fn make()->P=>P{r:R{code:65},secret:42}",
        ),
    ]);
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
            .any(|d| d.code.contains("PRIVATE")),
        "{}",
        report.to_json()
    );
}

#[test]
fn partial_slots_and_descriptors_cannot_be_reused_after_transfer_or_replacement() {
    for body in [
        "let a=[make(65),make(66)];let first=a[0];let again=a[0];return first.n+again.n",
        "let a=[make(65),make(66)];let first=a[0];let whole=a;return first.n+whole[1].n",
        "var a=[make(65),make(66)];let old=a[0].r;a[0]=make(67);return old.code",
        "let a=[make(65),make(66)];let old=a[0].r;let moved=a[0];return old.code+moved.n",
        "let a=[make(65),make(66)];a[0]=make(67);return 0",
        "let a=[make(65),make(66)];return a[2].n",
        "let a=[make(65),make(66)];return a[-1].n",
    ] {
        let source = format!("{PACKAGE} fn main()->i32{{{body}}}");
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_ne!(report.phase, zil_base::Phase::Parse, "{}", report.to_json());
    }
}

#[test]
fn old_byte_buffer_window_cannot_survive_any_array_element_replacement() {
    for replacement in [
        "a[0]=make()",
        "a[1]=make()",
        "let moved=a[0]",
        "let moved=a",
    ] {
        let source = format!(
            "package P{{manages b:ByteBuffer}}fn make()->P=>P{{b:bytes_new()}};fn main()->i32{{var a=[make(),make()];let v=bytes_view(a[0].b);{replacement};return i32(v[0])}}"
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
                .any(|d| d.code.starts_with("E-BORROW") || d.code.contains("RESOURCE")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn dynamic_indices_and_implicit_resource_arrays_remain_explicit_boundaries() {
    for source in [
        format!("{PACKAGE} fn main()->i32{{let a=[make(65),make(66)];let i=0;return a[i].n}}"),
        "resource R{n:i32}fn main(){let a=[R{n:1},R{n:2}]}".into(),
    ] {
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_ne!(report.phase, zil_base::Phase::Parse, "{}", report.to_json());
    }
}

#[test]
fn repeated_loop_refill_reinitializes_only_the_chosen_slot() {
    let source = format!(
        "{PACKAGE} fn main()->i32{{var a=[make(65),make(66)];var i=0;while i<3{{let old=a[0];a[0]=make(old.n+1);i+=1}};return a[0].n-68}}"
    );
    native(&[("main", &source)], b"ABCDB", 0);
}

#[test]
fn whole_array_can_cross_modules_with_exact_public_package_identity() {
    native(
        &[
            (
                "main",
                "import model;import bridge;fn main()->i32{var a=bridge.relay([model.make(65),model.make(66)]);let old=a[0];a[0]=model.make(67);return old.n+a[0].n+a[1].n-198}",
            ),
            (
                "model",
                r#"extern "C" fn putchar(x:i32)->i32;pub resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}pub package P{manages r:R;pub n:i32}pub fn make(n:i32)->P=>P{r:R{code:n},n:n}"#,
            ),
            (
                "bridge",
                "import model;pub fn relay(a:[model.P;2])->[model.P;2]=>a",
            ),
        ],
        b"ACB",
        0,
    );
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw167.zil", source);
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

#[test]
fn raw_and_wire_cannot_copy_elements_or_retake_the_same_slot() {
    use zil_ir::InstructionKind;
    let source = format!(
        "{PACKAGE} fn main()->i32{{let a=[make(65),make(66)];let first=a[0];let second=a[1];return first.n+second.n-131}}"
    );
    let (original, plan) = candidate(&source);
    for duplicate_construction in [true, false] {
        let mut forged = original.clone();
        let mut changed = 0;
        for instruction in forged
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
        {
            match &mut instruction.kind {
                InstructionKind::MakeOwnedArray { elements } if duplicate_construction => {
                    elements[1] = elements[0];
                    changed += 1;
                }
                InstructionKind::TakeOwnedArrayElement { index, .. }
                    if !duplicate_construction && *index == 1 =>
                {
                    *index = 0;
                    changed += 1;
                }
                _ => {}
            }
        }
        assert_eq!(changed, 1);
        rejected_candidate(
            if duplicate_construction {
                "duplicate-element-ssa"
            } else {
                "second-take-same-slot"
            },
            forged,
            plan.clone(),
            ResultCategory::SafeReject,
        );
    }
}

#[test]
fn raw_and_wire_require_cleanup_before_overwriting_and_complete_whole_moves() {
    use zil_ir::{InstructionKind, Type};
    let source = format!(
        "{PACKAGE} fn relay(a:[P;2])->[P;2]=>a;fn main()->i32{{var a=[make(65),make(66)];a[0]=make(67);let all=relay(a);return all[0].n+all[1].n-133}}"
    );
    let (original, plan) = candidate(&source);
    for variant in 0..3 {
        let mut forged = original.clone();
        let mut changed = 0;
        for function in &mut forged.functions {
            for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                match &mut instruction.kind {
                    InstructionKind::DropOwnedArrayElement { index, .. } if variant == 0 => {
                        *index = 1;
                        changed += 1;
                    }
                    InstructionKind::StoreOwnedArrayElement { local, index, .. }
                        if variant == 1 =>
                    {
                        instruction.kind = InstructionKind::DropOwnedArrayElement {
                            local: *local,
                            index: *index,
                        };
                        changed += 1;
                    }
                    InstructionKind::TakeLocal { local }
                        if variant == 2
                            && matches!(
                                function.locals[local.0 as usize].ty,
                                Type::Array { .. }
                            ) =>
                    {
                        instruction.kind = InstructionKind::Load(*local);
                        changed += 1;
                    }
                    _ => {}
                }
            }
        }
        assert!(changed > 0);
        rejected_candidate(
            [
                "wrong-slot-cleanup",
                "missing-refill-before-move",
                "copy-load-owned-array",
            ][variant],
            forged,
            plan.clone(),
            ResultCategory::SafeReject,
        );
    }
}

#[test]
fn raw_and_wire_check_static_bounds_and_charge_declared_array_size() {
    use zil_ir::{InstructionKind, Type};
    let source = format!("{PACKAGE} fn main()->i32{{let a=[make(65),make(66)];return a[0].n-65}}");
    let (original, plan) = candidate(&source);
    for variant in 0..3 {
        let mut forged = original.clone();
        let mut changed = 0;
        for function in &mut forged.functions {
            if variant == 2 {
                for ty in function
                    .locals
                    .iter_mut()
                    .map(|l| &mut l.ty)
                    .chain(&mut function.values)
                {
                    if let Type::Array { length, .. } = ty {
                        *length = u32::MAX;
                        changed += 1;
                    }
                }
            } else {
                for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                    if let InstructionKind::ProjectOwnedArrayField { index, field, .. } =
                        &mut instruction.kind
                    {
                        if variant == 0 {
                            *index = u32::MAX;
                        } else {
                            *field = u32::MAX;
                        }
                        changed += 1;
                    }
                }
            }
        }
        assert!(changed > 0);
        rejected_candidate(
            [
                "index-outside-fixed-array",
                "field-outside-package",
                "oversized-array-metadata",
            ][variant],
            forged,
            plan.clone(),
            if variant == 2 {
                ResultCategory::ResourceLimit
            } else {
                ResultCategory::SafeReject
            },
        );
    }
}

#[test]
fn raw_and_wire_cannot_relabel_owned_operations_as_copy_aggregate_operations() {
    use zil_ir::InstructionKind;
    let source = format!(
        "{PACKAGE} fn main()->i32{{let a=[make(65),make(66)];let first=a[0];return first.n-65}}"
    );
    let (original, plan) = candidate(&source);
    for extract in [false, true] {
        let mut forged = original.clone();
        let mut changed = 0;
        for function in &mut forged.functions {
            let array = function
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| {
                    matches!(i.kind, InstructionKind::MakeOwnedArray { .. })
                        .then_some(i.result)
                        .flatten()
                });
            let Some(array) = array else { continue };
            for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                match &instruction.kind {
                    InstructionKind::MakeOwnedArray { elements } if !extract => {
                        instruction.kind = InstructionKind::MakeAggregate {
                            fields: elements.clone(),
                        };
                        changed += 1;
                    }
                    InstructionKind::TakeOwnedArrayElement { index, .. } if extract => {
                        instruction.kind = InstructionKind::ExtractField {
                            aggregate: array,
                            index: *index,
                        };
                        changed += 1;
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(changed, 1);
        rejected_candidate(
            if extract {
                "copy-field-from-owned-array"
            } else {
                "copy-construct-owned-array"
            },
            forged,
            plan.clone(),
            ResultCategory::SafeReject,
        );
    }
}

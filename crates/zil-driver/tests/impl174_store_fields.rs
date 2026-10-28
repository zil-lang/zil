//! Store fields retain their initialized prefixes through real inline Package ownership.
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
        "store-fields174-{}-{}",
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
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw174.zil", source);
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
resource R { code:i32 }
impl Drop for R { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Item { manages item:R; n:i32 }
package State { manages tag:R; manages items:Store<Item>; n:i32 }
fn item(n:i32)->Item => Item { item:R{code:n}, n:n }
fn empty()->Store<Item> contract{result(create)} { return store_new() }
fn state(tag:i32,a:i32,b:i32)->State {
    let store=empty(); store_reserve(store,2);
    store_push(store,item(a)); store_push(store,item(b));
    return State {tag:R{code:tag},items:give store,n:4}
}
fn relay(state:State)->State => state
"#;

#[test]
fn complete_store_fields_construct_move_and_drop_prefix_in_field_order() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn main()->i32{{let s=relay(state(90,65,66));return i32(store_len(s.items))-2}}"
            ),
        )],
        b"ZAB",
        0,
    );
}

#[test]
fn whole_field_take_refill_and_state_transfer_have_one_responsibility() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn main()->i32{{var p=state(90,65,66);let items=give p.items;let next=state(89,67,68);p.items=give next.items;let restored=relay(p);return i32(store_len(restored.items))-2}}"
            ),
        )],
        b"ZCDYAB",
        0,
    );
}

#[test]
fn replacing_store_field_drops_old_prefix_before_new_publication() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn main()->i32{{var p=state(90,65,66);let next=state(89,67,68);p.items=give next.items;return i32(store_len(p.items))-2}}"
            ),
        )],
        b"ABYZCD",
        0,
    );
}

#[test]
fn taken_store_field_early_error_cleans_only_live_shell_fields() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn main()->void!i32{{let p=state(90,65,66);let items=give p.items;return ()}}"
            ),
        )],
        b"ABZ",
        1,
    );
}

#[test]
fn store_state_in_owned_error_and_fixed_array_recursively_cleans_prefix() {
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn fail()->State!i32{{return state(90,65,66)}} fn main()->State!i32{{fail()?;return 0}}"
            ),
        )],
        b"ZAB",
        1,
    );
    native(
        &[(
            "main",
            &format!(
                "{MODEL} fn main()->i32{{let states=[state(90,65,66),state(89,67,68)];return 0}}"
            ),
        )],
        b"ZABYCD",
        0,
    );
}

#[test]
fn sealed_definitions_are_discovered_from_state_only_signatures() {
    for signature in ["State", "[State;2]", "State!i32"] {
        let source = format!(
            r#"
            resource R{{code:i32}}
            package Item{{manages item:R;manages data:ByteBuffer;n:i32}}
            package State{{manages items:Store<Item>}}
            fn sink(value:{signature})->i32=>0
            fn main()->i32=>0
        "#
        );
        native(&[("main", &source)], b"", 0);
        let (module, _) = candidate(&source);
        assert!(
            module
                .resources
                .iter()
                .any(|r| matches!(r.payload, zil_ir::Type::OwnedStore { .. }))
        );
        assert!(
            module
                .resources
                .iter()
                .any(|r| r.payload == zil_ir::Type::ByteBuffer)
        );
    }
}

#[test]
fn cross_module_state_owns_runtime_replacement_and_partial_exit() {
    let model = include_str!("../../../examples/mainline/impl174/model.zil");
    let storage = include_str!("../../../examples/mainline/impl174/storage.zil");
    let state = include_str!("../../../examples/mainline/impl174/state.zil");
    for (entry, input, output, exit) in [
        (
            include_str!("../../../examples/mainline/impl174/main.zil"),
            b"3".as_slice(),
            b"ABZCDE".as_slice(),
            0,
        ),
        (
            include_str!("../../../examples/mainline/impl174/take_exit.zil"),
            b"".as_slice(),
            b"ABZ".as_slice(),
            1,
        ),
        (
            include_str!("../../../examples/mainline/impl174/reserve_recovery.zil"),
            b"".as_slice(),
            b"ZAB".as_slice(),
            0,
        ),
    ] {
        native_input(
            &[
                ("main", entry),
                ("model", model),
                ("storage", storage),
                ("state", state),
            ],
            input,
            output,
            exit,
        );
    }
}

#[test]
fn source_rejects_borrowed_store_fields_and_unsupported_nested_storage() {
    use zil_base::Phase;
    for (source, category, code, message) in [
        (
            format!("{MODEL} package Bad{{manages tag:R;items:Store<Item>}} fn main()->i32=>0"),
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
            "explicit package manages",
        ),
        (
            format!("{MODEL} package Bad{{manages items:Ref<Store<Item>>}} fn main()->i32=>0"),
            ResultCategory::SafeReject,
            "E-RESOURCE-TYPE",
            "manages requires",
        ),
        (
            format!("{MODEL} package Bad{{manages tag:R;states:[State;2]}} fn main()->i32=>0"),
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
            "outside owning array fields",
        ),
        (
            format!("{MODEL} fn main()->i32{{let s:Store<State>=store_new();return 0}}"),
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
            "nonzero complete direct-Managed",
        ),
        (
            format!(
                "{MODEL} fn main()->i32{{let s=state(90,65,66);let d=s.items;let moved=give d;return 0}}"
            ),
            ResultCategory::SafeReject,
            "E-GIVE-DESCRIPTOR",
            "does not own",
        ),
    ] {
        let report = check(&[("main", &source)]);
        assert_eq!(report.status, category, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == Phase::Types && d.code == code && d.message.contains(message)),
            "{}",
            report.to_json()
        );
        assert!(report.commands.is_empty());
    }
}

#[test]
fn original_complete_store_state_capture_is_supported_since_204() {
    let source = format!(
        "{MODEL} fn main()->i32{{let s=state(90,65,66);let f=move ()=>i32(store_len(s.items));return f()}}"
    );
    let report = check(&[("main", &source)]);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Verify);
    assert_eq!(report.verification_runs, 1);
    // Preserve the original return value rather than rewriting its source.
    native(&[("main", &source)], b"ZAB", 2);
}

#[test]
fn source_rejects_missing_field_double_take_and_old_description_retargeting() {
    use zil_base::Phase;
    for (tail, message) in [
        (
            "fn main()->i32{let p=state(90,65,66);let a=give p.items;let b=give p.items;return 0}",
            "field take requires that field live",
        ),
        (
            "fn main()->i32{let p=state(90,65,66);let a=give p.items;let q=relay(p);return 0}",
            "whole aggregate move requires every owned",
        ),
        (
            "fn main()->i32{var p=state(90,65,66);let old=p.items;p.items=empty();return i32(store_len(old))}",
            "invalidates a description required later",
        ),
        (
            "fn main()->i32{let p=state(90,65,66);let old=p.items;let q=relay(p);return i32(store_len(old))}",
            "invalidates a description required later",
        ),
    ] {
        let source = format!("{MODEL}{tail}");
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.phase == Phase::Analyze
                && d.code == "E-ANALYZE-IR"
                && d.message.contains(message)),
            "{}",
            report.to_json()
        );
        assert!(report.commands.is_empty());
    }
}

fn locate(
    module: &zil_ir::TypedModule,
    predicate: impl Fn(&zil_ir::Instruction) -> bool,
) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if predicate(instruction) {
                    return (f, b, i);
                }
            }
        }
    }
    panic!("mutation target absent")
}

#[test]
fn raw_wire_reconstruct_sealed_store_definition_even_for_state_only_signature() {
    use zil_ir::Type;
    let source = "resource R{n:i32} package Item{manages r:R;n:i32} package State{manages items:Store<Item>} fn sink(s:State)->i32=>0; fn main()->i32=>0";
    let (original, plan) = candidate(source);
    let index = original
        .resources
        .iter()
        .position(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap();
    let mut wrong = original.clone();
    wrong.resources.remove(index);
    rejected_candidate(
        "state-only-missing-definition",
        wrong,
        plan.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-OWNED-STORE",
        "canonical sealed definition",
    );
    let mut wrong = original.clone();
    wrong.resources[index].name = "@builtin:OwnedStore<wrong>".into();
    rejected_candidate(
        "state-only-wrong-definition",
        wrong,
        plan.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-OWNED-STORE",
        "canonical element identity",
    );
    let mut wrong = original;
    wrong.resources[index].payload = Type::OwnedStore {
        element: Box::new(Type::I32),
    };
    rejected_candidate(
        "state-only-invalid-element",
        wrong,
        plan,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
        "complete direct-Managed Package",
    );
}

#[test]
fn raw_wire_reject_double_take_unpaid_replace_and_wrong_field_projection() {
    use zil_ir::{InstructionKind as I, ValueId};
    let source = format!(
        "{MODEL} fn main()->i32{{var p=state(90,65,66);let taken=give p.items;p.items=empty();p.items=empty();return i32(store_len(p.items))}}"
    );
    let (original, plan) = candidate(&source);
    let (f, b, i) = locate(&original, |i| matches!(i.kind, I::TakePackageField { .. }));
    let mut wrong = original.clone();
    let function = &mut wrong.functions[f];
    let mut duplicate = function.blocks[b].instructions[i].clone();
    let old = duplicate.result.unwrap();
    let value = ValueId(function.values.len() as u32);
    function
        .values
        .push(function.values[old.0 as usize].clone());
    duplicate.result = Some(value);
    function.blocks[b].instructions.insert(i + 1, duplicate);
    let mut wrong_plan = plan.clone();
    let mut hold = wrong_plan.functions[f]
        .holds
        .iter()
        .find(|h| h.value == old)
        .unwrap()
        .clone();
    hold.value = value;
    wrong_plan.functions[f].holds.push(hold);
    rejected_candidate(
        "double-store-field-take",
        wrong,
        wrong_plan,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
        "live field",
    );
    // Delete the second field cleanup, where a full replacement is actually owed.
    let (f, b, i) = locate(&original, |i| matches!(i.kind, I::DropPackageField { .. }));
    let mut wrong = original.clone();
    let position = wrong.functions[f].blocks[b]
        .instructions
        .iter()
        .enumerate()
        .skip(i + 1)
        .find(|(_, i)| matches!(i.kind, I::DropPackageField { .. }))
        .map(|(i, _)| i)
        .unwrap();
    wrong.functions[f].blocks[b].instructions.remove(position);
    rejected_candidate(
        "unpaid-store-field-replace",
        wrong,
        plan.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
        "outstanding responsibility",
    );
    let (f, b, i) = locate(&original, |i| {
        matches!(i.kind, I::DescribePackageField { .. })
    });
    let mut wrong = original;
    if let I::DescribePackageField { index, .. } =
        &mut wrong.functions[f].blocks[b].instructions[i].kind
    {
        *index = 0;
    }
    rejected_candidate(
        "wrong-store-field-description",
        wrong,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-TYPE",
        "instruction result presence or type",
    );
}

#[test]
fn raw_wire_store_capture_metadata_cannot_replace_the_actual_input() {
    use zil_ir::Type;
    let source = format!(
        "{MODEL} fn main()->i32{{let p=state(90,65,66);let item=item(67);let f=move ()=>item.n;return f()-67}}"
    );
    let (mut module, plan) = candidate(&source);
    let state = module
        .functions
        .iter()
        .flat_map(|f| f.values.iter())
        .find(|t| matches!(t,Type::Package{name,..}if name.ends_with(":State")))
        .unwrap()
        .clone();
    module.closures[0].captures[0] = state;
    rejected_candidate(
        "store-state-capture",
        module,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-CAPTURE",
        "capture operand type differs from the environment field",
    );
}

#[test]
fn store_field_raw_and_wire_budget_has_an_exact_boundary() {
    let source = format!(
        "{MODEL} fn main()->i32{{var p=state(90,65,66);p.items=empty();return i32(store_len(p.items))}}"
    );
    let (module, plan) = candidate(&source);
    let target = TargetProfile::host_llvm();
    let default = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &default).unwrap();
    let mut lo = 0;
    let mut hi = default.max_analysis_steps;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        let limits = Limits {
            max_analysis_steps: mid,
            ..default.clone()
        };
        if zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).is_ok() {
            hi = mid
        } else {
            lo = mid
        }
    }
    for work in [hi, hi - 1] {
        let limits = Limits {
            max_analysis_steps: work,
            ..default.clone()
        };
        for result in [
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits),
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ] {
            if work == hi {
                result.unwrap();
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit)
                );
            }
        }
    }
    fs::write(
        directory().join("verify-budget.txt"),
        format!("N={hi}; N-1={}\n", hi - 1),
    )
    .unwrap();
}

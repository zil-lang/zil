//! Real source, native cleanup and hostile candidates for dynamic owning prefixes.
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
        "owned-store172-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(source: &str) -> CompileReport {
    let dir = directory();
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

fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) -> Vec<PathBuf> {
    native_input(files, b"", stdout, exit)
}

fn native_input(files: &[(&str, &str)], input: &[u8], stdout: &[u8], exit: i32) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
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
        fs::write(
            dir.join("exit.txt"),
            format!("{:?}\n", output.status.code()),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
        dirs.push(dir);
    }
    dirs
}

const PACKAGE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
fn make(code:i32)->P=>P{r:R{code:code},n:code}
fn empty()->Store<P> contract{result(create)}{return store_new()}
"#;

#[test]
fn empty_context_and_factory_own_a_real_allocation_free_head() {
    let source = format!(
        "{PACKAGE} fn main()->i32{{let a:Store<P>=store_new();let b=empty();return i32(store_len(a)+store_capacity(b))}}"
    );
    native(&[("main", &source)], b"", 0);
}

#[test]
fn reserved_prefix_push_pop_and_drop_transfer_each_package_once() {
    let source = format!(
        "{PACKAGE} fn main()->void!i32{{let s=empty();if !store_reserve(s,2){{return ()}};store_push(s,make(65));store_push(s,make(66));let p=store_pop(s)?;return p.n-66}}"
    );
    native(&[("main", &source)], b"BA", 0);
}

#[test]
fn full_push_returns_the_uninserted_package_to_native_error_cleanup() {
    let source =
        format!("{PACKAGE} fn main()->P!i32{{let s=empty();store_push(s,make(88))?;return 0}}");
    native(&[("main", &source)], b"X", 1);
}

#[test]
fn whole_give_roundtrip_preserves_ascending_prefix_cleanup() {
    let source = format!(
        "{PACKAGE} fn relay(give s:Store<P>)->Store<P> contract{{result(give)}}{{return give s}};fn main()->i32{{let s=empty();if !store_reserve(s,2){{return 7}};store_push(s,make(65));store_push(s,make(66));let moved=relay(give s);return i32(store_len(moved))-2}}"
    );
    native(&[("main", &source)], b"AB", 0);
}

#[test]
fn empty_pop_propagates_unit_error_without_an_element_obligation() {
    let source =
        format!("{PACKAGE} fn main()->void!i32{{let s=empty();let p=store_pop(s)?;return p.n}}");
    native(&[("main", &source)], b"", 1);
}

const MODEL: &str = include_str!("../../../examples/mainline/impl172/model.zil");
const STORAGE: &str = include_str!("../../../examples/mainline/impl172/storage.zil");
const WORK: &str = include_str!("../../../examples/mainline/impl172/work.zil");
const MAIN: &str = include_str!("../../../examples/mainline/impl172/main.zil");

#[test]
fn runtime_count_crosses_four_modules_then_drains_real_prefix() {
    for (input, output) in [
        (b"0".as_slice(), b"".as_slice()),
        (b"1", b"A"),
        (b"4", b"DCBA"),
        (b"8", b"HGFEDCBA"),
    ] {
        native_input(
            &[
                ("main", MAIN),
                ("model", MODEL),
                ("storage", STORAGE),
                ("work", WORK),
            ],
            input,
            output,
            0,
        );
    }
}

#[test]
fn negative_and_overflow_capacity_requests_preserve_populated_prefix() {
    let source = format!(
        "{PACKAGE} fn main()->void!i32{{let s=empty();if !store_reserve(s,1){{return ()}};store_push(s,make(65));if store_reserve(s,-1)||store_reserve(s,9223372036854775807){{return ()}};if store_len(s)!=1||store_capacity(s)!=1{{return ()}};let p=store_pop(s)?;return p.n-65}}"
    );
    native(&[("main", &source)], b"A", 0);
}

#[test]
fn source_guards_reject_missing_context_store_fields_and_capture() {
    for (source,category,needle) in [
  (format!("{PACKAGE} fn main()->i32{{let s=store_new();return 0}}"),ResultCategory::SafeReject,"store_new requires"),
  (format!("{PACKAGE} package Bad{{manages children:Store<P>}} fn main()->i32=>0"),ResultCategory::Unsupported,"Store fields require"),
  (format!("{PACKAGE} fn main()->i32{{let s=empty();let f=move ()=>i32(store_len(s));return f()}}"),ResultCategory::Unsupported,"Store capture requires"),
  (format!("{PACKAGE} fn main()->i32{{let s=empty();let f=()=>i32(store_len(s));return f()}}"),ResultCategory::Unsupported,"Store capture requires"),
  ("resource Z{} package Empty{manages z:Z} fn main()->i32{let s:Store<Empty>=store_new();return 0}".into(),ResultCategory::Unsupported,"nonzero complete direct-Managed"),
  ("fn main()->i32{let s:Store<i32>=store_new();return 0}".into(),ResultCategory::Unsupported,"nonzero complete direct-Managed"),
 ] {
  let result=check(&source);assert_eq!(result.status,category,"{}",result.to_json());assert!(result.diagnostics.iter().any(|d|d.message.contains(needle)),"{}",result.to_json());
 }
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw172.zil", source);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &target, &limits).unwrap();
    (original, plan)
}

fn reject_candidate(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    code: &str,
    message: &str,
) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    // Fingerprints and empty candidate Hold rows are untrusted claims. Never
    // call Analyze on a mutation to manufacture a replacement proof.
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory();
    fs::write(dir.join("label"), label).unwrap();
    fs::write(dir.join("candidate.bin"), &bytes).unwrap();
    for (route, errors) in [
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
            dir.join(format!("{route}-errors.txt")),
            format!("{errors:#?}\n"),
        )
        .unwrap();
        assert!(
            errors.iter().all(|d| d.phase == zil_base::Phase::Verify),
            "{label} {route}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|d| d.code == code && d.message.contains(message)),
            "{label} {route}: {errors:?}"
        );
    }
}

fn locate(
    module: &zil_ir::TypedModule,
    predicate: impl Fn(&zil_ir::TypedFunction, &zil_ir::Instruction) -> bool,
) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if predicate(function, instruction) {
                    return (f, b, i);
                }
            }
        }
    }
    panic!("mutation target absent");
}

fn duplicate_result(
    module: &mut zil_ir::TypedModule,
    plan: &mut zil_ir::CandidatePlan,
    position: (usize, usize, usize),
) {
    let (f, b, i) = position;
    let function = &mut module.functions[f];
    let mut duplicate = function.blocks[b].instructions[i].clone();
    let old = duplicate.result.unwrap();
    let value = zil_ir::ValueId(function.values.len() as u32);
    function
        .values
        .push(function.values[old.0 as usize].clone());
    duplicate.result = Some(value);
    function.blocks[b].instructions.insert(i + 1, duplicate);
    let mut hold = plan.functions[f]
        .holds
        .iter()
        .find(|h| h.value == old)
        .unwrap()
        .clone();
    hold.value = value;
    plan.functions[f].holds.push(hold);
}

#[test]
fn raw_wire_reject_forged_heads_bad_elements_and_double_transfer() {
    use zil_ir::{InstructionKind as I, Type};
    let source = format!(
        "{PACKAGE} fn main()->P!i32{{let s=empty();store_reserve(s,2);store_push(s,make(65))?;return 0}}"
    );
    let (original, plan) = candidate(&source);
    let new = locate(&original, |_, i| matches!(i.kind, I::OwnedStoreNew { .. }));
    let push = locate(&original, |_, i| matches!(i.kind, I::OwnedStorePush { .. }));
    let reserve = locate(&original, |_, i| {
        matches!(i.kind, I::OwnedStoreReserve { .. })
    });
    let store_definition = original
        .resources
        .iter()
        .position(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap();
    let mut mutated = original.clone();
    mutated.functions[new.0].blocks[new.1].instructions[new.2].kind = I::OwnedStoreNew {
        element: Type::Unit,
    };
    reject_candidate(
        "wrong-new-element",
        mutated,
        plan.clone(),
        "E-UNSUPPORTED",
        "complete direct-Managed Package",
    );
    let mut mutated = original.clone();
    mutated.functions[new.0].blocks[new.1].instructions[new.2].kind =
        I::MakeManaged { fields: vec![] };
    reject_candidate(
        "forge-managed-head",
        mutated,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "cannot be assembled",
    );
    let mut mutated = original.clone();
    mutated.resources[store_definition].name = "@builtin:OwnedStore<forged>".into();
    reject_candidate(
        "wrong-canonical-name",
        mutated,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "canonical element identity",
    );
    let mut mutated = original.clone();
    mutated.resources[store_definition].teardown =
        original.resources.iter().find_map(|r| r.teardown);
    assert!(mutated.resources[store_definition].teardown.is_some());
    reject_candidate(
        "forged-user-teardown",
        mutated,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "no user teardown",
    );
    let mut mutated = original.clone();
    mutated.resources.remove(store_definition);
    reject_candidate(
        "missing-definition",
        mutated,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "canonical sealed definition",
    );
    let mut mutated = original.clone();
    let I::OwnedStoreReserve { store, .. } =
        mutated.functions[reserve.0].blocks[reserve.1].instructions[reserve.2].kind
    else {
        unreachable!()
    };
    mutated.functions[reserve.0].blocks[reserve.1].instructions[reserve.2].kind =
        I::OwnedStoreReserve {
            store,
            capacity: store,
        };
    reject_candidate(
        "capacity-is-a-description",
        mutated,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "capacity must be i64",
    );
    let mut mutated = original.clone();
    let I::OwnedStorePush { store, .. } =
        mutated.functions[push.0].blocks[push.1].instructions[push.2].kind
    else {
        unreachable!()
    };
    mutated.functions[push.0].blocks[push.1].instructions[push.2].kind = I::OwnedStorePush {
        store,
        value: store,
    };
    reject_candidate(
        "description-is-not-an-element",
        mutated,
        plan.clone(),
        "E-VERIFY-OWNED-STORE",
        "complete nominal Package element",
    );
    let mut mutated = original.clone();
    let mut certificate = plan.clone();
    duplicate_result(&mut mutated, &mut certificate, push);
    reject_candidate(
        "push-same-ssa-twice",
        mutated,
        certificate,
        "E-VERIFY-RESOURCE",
        "exactly once",
    );
    let mut mutated = original.clone();
    let drop = locate(
        &mutated,
        |f, i| matches!(i.kind,I::DropManaged{local} if matches!(&f.locals[local.0 as usize].ty,Type::Managed(p) if matches!(p.as_ref(),Type::OwnedStore{..}))),
    );
    mutated.functions[drop.0].blocks[drop.1]
        .instructions
        .remove(drop.2);
    reject_candidate(
        "missing-prefix-cleanup",
        mutated,
        plan.clone(),
        "E-VERIFY-RESOURCE",
        "outstanding",
    );
    let mut mutated = original.clone();
    let output = mutated.functions[new.0].blocks[new.1].instructions[new.2]
        .result
        .unwrap();
    mutated.functions[new.0].values[output.0 as usize] = Type::Unit;
    reject_candidate(
        "forged-new-result-type",
        mutated,
        plan,
        "E-VERIFY-TYPE",
        "presence or type",
    );
}

#[test]
fn exact_verify_budget_boundary_and_untrusted_constructor_metadata_are_bounded() {
    use zil_ir::{InstructionKind as I, Type};
    let source = format!(
        "{PACKAGE} fn main()->i32{{let s=empty();store_reserve(s,2);store_push(s,make(65));return 0}}"
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
    let dir = directory();
    fs::write(
        dir.join("verify-budget.txt"),
        format!("N={hi}, N-1={}\n", hi - 1),
    )
    .unwrap();
    let mut mutated = module;
    let (f, b, i) = locate(&mutated, |_, i| matches!(i.kind, I::OwnedStoreNew { .. }));
    let constrained = Limits {
        max_analysis_steps: hi * 2,
        ..default.clone()
    };
    let element = Type::Tuple(vec![Type::I32; constrained.max_analysis_steps + 1]);
    mutated.functions[f].blocks[b].instructions[i].kind = I::OwnedStoreNew { element };
    let result = zil_verify::verify_module(mutated, plan, &target, &constrained).unwrap_err();
    assert!(
        result
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{result:?}"
    );
}

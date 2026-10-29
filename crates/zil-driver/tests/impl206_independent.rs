//! Independent 206 contract fixtures, fixed before implementation integration.
//! Source, raw TypedIR/Wire and actual native behavior are distinct evidence.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
use zil_ir::*;

macro_rules! source {
    ($name:literal) => {
        include_str!(concat!("impl206_independent_fixtures/", $name, ".zil"))
    };
}
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let dir = root
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "independent206-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir, root.is_some())
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn unit(source: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "independent206.zil", source)
}
fn compile(case: &Case, source: &str, optimization: Option<Optimization>) -> CompileReport {
    case.write("source.zil", source);
    let report = compile_source(
        &unit(source),
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn accepted(label: &str, source: &str) {
    let case = Case::new(label);
    let r = compile(&case, source, None);
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert_eq!(r.verification_runs, 1, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
fn rejected(
    label: &str,
    source: &str,
    phase: Phase,
    category: ResultCategory,
    code: Option<&str>,
    text: Option<&str>,
) {
    let case = Case::new(label);
    let r = compile(&case, source, None);
    assert_eq!(r.phase, phase, "{}", r.to_json());
    assert_eq!(r.status, category, "{}", r.to_json());
    if let Some(code) = code {
        assert!(
            r.diagnostics.iter().any(|d| d.code == code),
            "{}",
            r.to_json()
        );
    }
    if let Some(text) = text {
        assert!(
            r.diagnostics.iter().any(|d| d.message.contains(text)),
            "{}",
            r.to_json()
        );
    }
    assert!(!r.artifacts.contains_key("executable"));
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let r = compile(&case, source, Some(optimization));
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1, "{}", r.to_json());
        let actual = Command::new(&r.artifacts["executable"]).output().unwrap();
        case.write("stdout.log", &actual.stdout);
        case.write("stderr.log", &actual.stderr);
        case.write("native.txt", format!("optimization={optimization:?}\nactual_exit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n", actual.status.code()));
        assert_eq!(actual.status.code(), Some(exit), "{label}: {actual:?}");
        assert_eq!(actual.stdout, stdout.as_bytes(), "{label}: {actual:?}");
        assert!(actual.stderr.is_empty(), "{label}: {actual:?}");
    }
}
fn expanded(case: &Case, source: &str) -> CoreModule {
    case.write("source.zil", source);
    let source = unit(source);
    let syntax = zil_syntax::parse(&source).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&source, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    core.unwrap()
}
fn verify_pair(label: &str, module: &TypedModule, candidate: &CandidatePlan, accept: bool) {
    let case = Case::new(label);
    let limits = Limits::default();
    case.write("typed.txt", format!("{module:#?}"));
    case.write("candidate.txt", format!("{candidate:#?}"));
    let wire = zil_ir::wire::encode_module_candidate(module, candidate, &limits).unwrap();
    case.write("candidate.wire", &wire);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
        (module.clone(), candidate.clone())
    );
    let raw = zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits);
    let bytes = zil_verify::verify_module_bytes(&wire, &module.target, &limits);
    case.write("verify.txt", format!("raw={raw:#?}\nwire={bytes:#?}"));
    for result in [raw, bytes] {
        if accept {
            result.unwrap();
        } else {
            let errors = result.unwrap_err();
            assert!(
                !errors.iter().any(|d| d.code == "E-VERIFY-PLAN-IDENTITY"),
                "identity alone is not semantic rejection: {errors:#?}"
            );
            assert!(
                errors.iter().any(|d| d.phase == Phase::Verify
                    && matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )),
                "{errors:#?}"
            );
            assert!(
                !errors
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit),
                "{errors:#?}"
            );
        }
    }
}
fn control(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let case = Case::new(label);
    let core = expanded(&case, source);
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default());
    case.write("types.txt", format!("{typed:#?}"));
    let typed = typed.unwrap();
    let candidate = zil_analyze::analyze(&typed, &Limits::default());
    case.write("analyze.txt", format!("{candidate:#?}"));
    let candidate = candidate.unwrap();
    verify_pair(
        &format!("{label}-raw-wire-control"),
        &typed,
        &candidate,
        true,
    );
    (typed, candidate)
}
fn attack(label: &str, module: &TypedModule, original: &CandidatePlan, analyze_must_reject: bool) {
    let case = Case::new(label);
    case.write("typed.txt", format!("{module:#?}"));
    let actual = zil_analyze::analyze(module, &Limits::default());
    case.write("analyze.txt", format!("{actual:#?}"));
    if let Ok(candidate) = &actual {
        verify_pair(
            &format!("{label}-actual-analyze-candidate"),
            module,
            candidate,
            false,
        );
    }
    let mut forged = original.clone();
    forged.module_fingerprint = module.fingerprint();
    verify_pair(&format!("{label}-forged-candidate"), module, &forged, false);
    if analyze_must_reject {
        let errors = actual.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
            "{label}: {errors:#?}"
        );
    }
}
fn named_package(ty: &Type, suffix: &str) -> bool {
    matches!(ty, Type::Package { name, .. } if name.ends_with(suffix))
}

#[test]
fn raw_actual_sibling_and_leaf_positions_keep_distinct_freshness() {
    let (original, plan) = control("twins-fresh-control", source!("03_twins_left_created"));
    for variant in 0..3 {
        let mut module = original.clone();
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "origin")
            .unwrap();
        let mut changed = 0;
        for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            match &mut i.kind {
                InstructionKind::MakePackage { fields }
                    if variant == 0
                        && i.result
                            .is_some_and(|v| named_package(&f.values[v.0 as usize], ":Twins")) =>
                {
                    assert_eq!(fields.len(), 2);
                    fields.swap(0, 1);
                    changed += 1;
                }
                InstructionKind::TakePackageField { local, index }
                    if variant == 1
                        && named_package(&f.locals[local.0 as usize].ty, ":Twins")
                        && *index == 0 =>
                {
                    *index = 1;
                    changed += 1;
                }
                InstructionKind::TakePackageField { local, index }
                    if variant == 2
                        && named_package(&f.locals[local.0 as usize].ty, ":Leaf")
                        && *index == 2 =>
                {
                    *index = 1;
                    changed += 1;
                }
                _ => {}
            }
        }
        assert_eq!(changed, 1);
        attack(
            &format!("twins-origin-position-{variant}"),
            &module,
            &plan,
            true,
        );
    }
}

#[test]
fn raw_complete_child_operands_are_affine_and_wrong_sibling_drop_is_not_refill() {
    let (original, plan) = control("child-affine-control", source!("34_replace_complete_child"));
    for variant in 0..3 {
        let mut module = original.clone();
        let mut changed = 0;
        for f in &mut module.functions {
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if changed != 0 {
                    continue;
                }
                match &mut i.kind {
                    InstructionKind::MakePackage { fields }
                        if variant == 0
                            && i.result.is_some_and(|v| {
                                named_package(&f.values[v.0 as usize], ":Twins")
                            }) =>
                    {
                        assert_eq!(fields.len(), 3);
                        fields[2] = fields[0];
                        changed += 1;
                    }
                    InstructionKind::TakeLocal { local }
                        if variant == 1
                            && named_package(&f.locals[local.0 as usize].ty, ":Root") =>
                    {
                        i.kind = InstructionKind::Load(*local);
                        changed += 1;
                    }
                    InstructionKind::DropPackageField { local, index }
                        if variant == 2
                            && named_package(&f.locals[local.0 as usize].ty, ":Root")
                            && *index == 0 =>
                    {
                        *index = 1;
                        changed += 1;
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(changed, 1);
        attack(
            &format!("complete-child-affine-{variant}"),
            &module,
            &plan,
            variant != 1,
        );
    }
}

#[test]
fn raw_ordinary_call_and_pure_input_cannot_forge_create() {
    for (label, source, function) in [
        ("unknown-call", source!("12_unknown_call_give"), "origin"),
        (
            "pure-incoming",
            source!("01_incoming_child_only"),
            "extract",
        ),
    ] {
        let (mut module, plan) = control(label, source);
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == function)
            .unwrap();
        assert_eq!(f.result_mode, PassingMode::Give);
        f.result_mode = PassingMode::Create;
        attack(&format!("{label}-forged-create"), &module, &plan, true);
    }
}

#[test]
fn raw_unused_nested_metadata_does_not_gain_ordinary_or_legacy_authority() {
    let (mut original, mut plan) =
        control("unused-types-control", source!("38_raw_metadata_control"));
    let root = original
        .functions
        .iter()
        .flat_map(|f| &f.locals)
        .find(|l| named_package(&l.ty, ":Root"))
        .unwrap()
        .ty
        .clone();
    let Type::Package { fields, .. } = &root else {
        unreachable!()
    };
    let leaf = fields[0].1.clone();
    let closure = original.closures.first().unwrap().id;
    let f = original
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let extra = f.locals.len();
    f.locals.push(LocalDecl {
        id: LocalId(extra as u32),
        name: "unused_review_metadata".into(),
        ty: root.clone(),
        mutable: false,
        span: f.span,
    });
    plan.module_fingerprint = original.fingerprint();
    verify_pair("unused-valid-nested-local", &original, &plan, true);
    for variant in 0..8 {
        let mut module = original.clone();
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let Type::Package { fields, .. } = &mut f.locals[extra].ty else {
            unreachable!()
        };
        match variant {
            0 => {
                let Type::Package { name, .. } = &mut fields[0].1 else {
                    unreachable!()
                };
                name.clear();
            }
            1 => {
                let Type::Package { fields, .. } = &mut fields[0].1 else {
                    unreachable!()
                };
                fields[1].0 = fields[0].0.clone();
            }
            2 => {
                let Type::Package { fields, .. } = &mut fields[0].1 else {
                    unreachable!()
                };
                for (_, ty) in fields {
                    *ty = Type::I32;
                }
            }
            3 => fields.push((
                "borrowed".into(),
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
            )),
            4 => fields.push((
                "owned_array".into(),
                Type::Array {
                    element: Box::new(leaf.clone()),
                    length: 2,
                },
            )),
            5 => fields.push((
                "owned_sum".into(),
                Type::ErrorUnion {
                    error: Box::new(Type::Enum {
                        name: "review206:E".into(),
                        variants: vec![("Fail".into(), vec![])],
                    }),
                    result: Box::new(leaf.clone()),
                },
            )),
            6 => fields.push(("callback".into(), Type::Closure(closure))),
            7 => {
                let Type::Package { fields, .. } = &mut fields[0].1 else {
                    unreachable!()
                };
                fields[0].1 = Type::Managed(Box::new(Type::Record {
                    name: "review206:UnregisteredResource".into(),
                    fields: vec![("n".into(), Type::I32)],
                }));
            }
            _ => unreachable!(),
        }
        attack(
            &format!("unused-nested-metadata-{variant}"),
            &module,
            &plan,
            false,
        );
    }
}

#[test]
fn raw_late_desc_read_cannot_revive_after_child_refill_and_embedding() {
    let (mut module, plan) = control("desc-last-use-control", source!("19_desc_last_use"));
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut changed = 0;
    for block in &mut function.blocks {
        let Some(read_index) = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
        else {
            continue;
        };
        let result = block.instructions[read_index].result.unwrap();
        let store_index = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::Store { value, .. } if value == result))
            .unwrap();
        assert!(store_index > read_index);
        let store = block.instructions.remove(store_index);
        let read = block.instructions.remove(read_index);
        let last_refill = block
            .instructions
            .iter()
            .rposition(|i| matches!(i.kind, InstructionKind::StorePackageField { .. }))
            .unwrap();
        block.instructions.insert(last_refill + 1, read);
        block.instructions.insert(last_refill + 2, store);
        changed += 1;
    }
    assert_eq!(changed, 1);
    // Both ReadDesc and its scalar Store move together; SSA definition order
    // remains valid. The same actual description is now read after invalidation.
    attack("desc-late-after-refill-and-embed", &module, &plan, true);
}

#[test]
fn raw_unused_resource_head_cannot_hide_an_ordinary_product() {
    let (mut module, mut plan) = control(
        "unused-resource-control",
        source!("38_raw_metadata_control"),
    );
    let product = module
        .functions
        .iter()
        .flat_map(|f| &f.locals)
        .find(|l| named_package(&l.ty, ":Root"))
        .unwrap()
        .ty
        .clone();
    let name = "review206:UnusedHead".to_string();
    module.resources.push(ResourceDefinition {
        name: name.clone(),
        payload: Type::Record {
            name,
            fields: vec![("copy".into(), Type::I32)],
        },
        teardown: None,
    });
    plan.module_fingerprint = module.fingerprint();
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    verify_pair("unused-copy-resource-head", &module, &plan, true);
    let Type::Record { fields, .. } = &mut module.resources.last_mut().unwrap().payload else {
        unreachable!()
    };
    fields.push(("hidden_product".into(), product));
    attack("unused-resource-product-head", &module, &plan, false);
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::Unsupported
            && d.message.contains("resource payload")),
        "{errors:#?}"
    );
}

fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>) -> usize {
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    run(high).unwrap();
    while low + 1 < high {
        let n = low + (high - low) / 2;
        match run(n) {
            Ok(()) => high = n,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:#?}"
                );
                low = n;
            }
        }
    }
    run(high).unwrap();
    let ds = run(high - 1).unwrap_err();
    assert!(
        ds.iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{ds:#?}"
    );
    run(high).unwrap();
    high
}

#[test]
fn nested_actual_leaf_paths_have_exact_fresh_phase_work_boundaries() {
    let source = source!("03_twins_left_created");
    let (typed, plan) = control("nested-phase-budget", source);
    let case = Case::new("nested-phase-boundaries");
    let input = unit(source);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    let limits = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let expand = boundary(|n| {
        zil_expand::expand_with_map_limits(&input, &syntax, &limits(n))
            .map(|m| assert_eq!(m.core, core))
    });
    let types = boundary(|n| {
        zil_types::check(&core, &typed.target, &limits(n)).map(|m| assert_eq!(m, typed))
    });
    let analyze =
        boundary(|n| zil_analyze::analyze(&typed, &limits(n)).map(|p| assert_eq!(p, plan)));
    let verify = boundary(|n| {
        zil_verify::verify_module(typed.clone(), plan.clone(), &typed.target, &limits(n))
            .map(|_| ())
    });
    let verify_wire =
        boundary(|n| zil_verify::verify_module_bytes(&wire, &typed.target, &limits(n)).map(|_| ()));
    case.write("source.zil", source);
    case.write("boundaries.json", format!("{{\"expand\":{expand},\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"wire\":{verify_wire},\"method\":\"measured N; N-1 ResourceLimit; fresh N; exact Core/Typed/Candidate equality; unchanged defaults\"}}\n"));
}

#[test]
fn source_00_original120() {
    accepted("00_original120", source!("00_original120"));
}

#[test]
fn source_01_incoming_child_only() {
    accepted("01_incoming_child_only", source!("01_incoming_child_only"));
}

#[test]
fn source_02_incoming_forged_create() {
    rejected(
        "02_incoming_forged_create",
        source!("02_incoming_forged_create"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn source_03_twins_left_created() {
    accepted("03_twins_left_created", source!("03_twins_left_created"));
}

#[test]
fn source_04_twins_right_created() {
    accepted("04_twins_right_created", source!("04_twins_right_created"));
}

#[test]
fn source_05_twins_left_incoming() {
    rejected(
        "05_twins_left_incoming",
        source!("05_twins_left_incoming"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn source_06_twins_right_incoming() {
    rejected(
        "06_twins_right_incoming",
        source!("06_twins_right_incoming"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn source_07_refill_actual_created() {
    accepted(
        "07_refill_actual_created",
        source!("07_refill_actual_created"),
    );
}

#[test]
fn source_08_refill_other_stays_incoming() {
    rejected(
        "08_refill_other_stays_incoming",
        source!("08_refill_other_stays_incoming"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn source_09_join_created() {
    accepted("09_join_created", source!("09_join_created"));
}

#[test]
fn source_10_join_mixed() {
    rejected(
        "10_join_mixed",
        source!("10_join_mixed"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn source_11_unknown_call_not_create() {
    rejected(
        "11_unknown_call_not_create",
        source!("11_unknown_call_not_create"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn source_12_unknown_call_give() {
    accepted("12_unknown_call_give", source!("12_unknown_call_give"));
}

#[test]
fn source_13_managed_create_contract() {
    accepted(
        "13_managed_create_contract",
        source!("13_managed_create_contract"),
    );
}

#[test]
fn source_14_zero_iteration_incoming() {
    rejected(
        "14_zero_iteration_incoming",
        source!("14_zero_iteration_incoming"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("create result forwards an incoming responsibility"),
    );
}

#[test]
fn source_15_partial_child_embed() {
    rejected(
        "15_partial_child_embed",
        source!("15_partial_child_embed"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("whole aggregate move requires every owned element or field live"),
    );
}

#[test]
fn source_16_partial_parent_return() {
    rejected(
        "16_partial_parent_return",
        source!("16_partial_parent_return"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("whole aggregate move requires every owned element or field live"),
    );
}

#[test]
fn source_17_child_take_twice() {
    rejected(
        "17_child_take_twice",
        source!("17_child_take_twice"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("package field take requires that field live"),
    );
}

#[test]
fn source_18_child_value_reused_after_refill() {
    rejected(
        "18_child_value_reused_after_refill",
        source!("18_child_value_reused_after_refill"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-UNINITIALIZED"),
        Some("is not initialized on every incoming path"),
    );
}

#[test]
fn source_19_desc_last_use() {
    accepted("19_desc_last_use", source!("19_desc_last_use"));
}

#[test]
fn source_20_desc_does_not_revive() {
    rejected(
        "20_desc_does_not_revive",
        source!("20_desc_does_not_revive"),
        Phase::Analyze,
        ResultCategory::SafeReject,
        Some("E-ANALYZE-IR"),
        Some("head transition invalidates a description required later"),
    );
}

#[test]
fn source_21_copy_companions() {
    accepted("21_copy_companions", source!("21_copy_companions"));
}

#[test]
fn source_22_inner_store() {
    rejected(
        "22_inner_store",
        source!("22_inner_store"),
        Phase::Types,
        ResultCategory::Unsupported,
        None,
        None,
    );
}

#[test]
fn source_23_store_parent() {
    rejected(
        "23_store_parent",
        source!("23_store_parent"),
        Phase::Types,
        ResultCategory::Unsupported,
        None,
        None,
    );
}

#[test]
fn source_24_array_parent() {
    rejected(
        "24_array_parent",
        source!("24_array_parent"),
        Phase::Types,
        ResultCategory::Unsupported,
        None,
        None,
    );
}

#[test]
fn source_25_sum_parent() {
    rejected(
        "25_sum_parent",
        source!("25_sum_parent"),
        Phase::Types,
        ResultCategory::Unsupported,
        None,
        None,
    );
}

#[test]
fn source_26_borrowed_companion() {
    rejected(
        "26_borrowed_companion",
        source!("26_borrowed_companion"),
        Phase::Types,
        ResultCategory::Unsupported,
        None,
        None,
    );
}

#[test]
fn source_27_inner_owning_array() {
    rejected(
        "27_inner_owning_array",
        source!("27_inner_owning_array"),
        Phase::Types,
        ResultCategory::Unsupported,
        None,
        None,
    );
}

#[test]
fn source_28_inner_owning_sum() {
    rejected(
        "28_inner_owning_sum",
        source!("28_inner_owning_sum"),
        Phase::Types,
        ResultCategory::Unsupported,
        None,
        None,
    );
}

#[test]
fn source_29_product_capture_admitted_after_214_216() {
    // Frozen 206 source bytes remain unchanged. Tasks214-216 intentionally
    // replace the old Types Unsupported gate with complete owning capture.
    accepted("29_product_capture", source!("29_product_capture"));
}

#[test]
fn source_30_inner_concrete_closure() {
    rejected(
        "30_inner_concrete_closure",
        source!("30_inner_concrete_closure"),
        Phase::Types,
        ResultCategory::Unsupported,
        None,
        None,
    );
}

#[test]
fn source_31_wrong_nominal_child() {
    rejected(
        "31_wrong_nominal_child",
        source!("31_wrong_nominal_child"),
        Phase::Types,
        ResultCategory::SafeReject,
        Some("E-TYPE-MISMATCH"),
        None,
    );
}

#[test]
fn source_32_recursive_drop() {
    native(
        "32_recursive_drop",
        source!("32_recursive_drop"),
        "ABCDZ",
        0,
    );
}

#[test]
fn source_33_partial_refill() {
    native(
        "33_partial_refill",
        source!("33_partial_refill"),
        "AEBCDZ",
        0,
    );
}

#[test]
fn source_34_replace_complete_child() {
    native(
        "34_replace_complete_child",
        source!("34_replace_complete_child"),
        "ABCDEFGHZ",
        0,
    );
}

#[test]
fn source_35_failed_rhs_keeps_old() {
    native(
        "35_failed_rhs_keeps_old",
        source!("35_failed_rhs_keeps_old"),
        "EFABCDZ",
        1,
    );
}

#[test]
fn source_36_failed_rhs_no_rollback() {
    native(
        "36_failed_rhs_no_rollback",
        source!("36_failed_rhs_no_rollback"),
        "ABCDZ",
        1,
    );
}

#[test]
fn source_37_layout_actual_values() {
    native(
        "37_layout_actual_values",
        source!("37_layout_actual_values"),
        "CDABZ",
        0,
    );
}

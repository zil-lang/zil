//! Independent Verify boundary: complete Product source, candidate/Wire and
//! per-actual-leaf create evidence. Negative programs never reach execution.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, PassingMode, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = base.join(format!(
        "product206-verify-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}

fn typed(label: &str, text: &str) -> TypedModule {
    let out = directory(label);
    fs::write(out.join("source.zil"), text).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "verify206.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
    module
}

fn raw_wire(
    label: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    expected: Option<(&str, &str)>,
) {
    plan.module_fingerprint = module.fingerprint();
    let out = directory(label);
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    fs::write(out.join("input.wire"), &wire).unwrap();
    fs::write(out.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &module.target,
                &Limits::default(),
            ),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()),
        ),
    ] {
        fs::write(out.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
        if let Some((code, message)) = expected {
            let errors = result.expect_err(label);
            assert!(
                errors.iter().any(|d| d.phase == Phase::Verify
                    && d.category == ResultCategory::SafeReject
                    && d.code == code
                    && d.message.contains(message)),
                "{label}/{route}: {errors:#?}"
            );
        } else {
            result.unwrap_or_else(|e| panic!("{label}/{route}: {e:#?}"));
        }
    }
}

const POSITIVES: &[(&str, &str)] = &[
    (
        "original120",
        include_str!("impl206_fixtures/00_original120.zil"),
    ),
    (
        "child-only",
        include_str!("impl206_fixtures/00_child_only.zil"),
    ),
    (
        "created",
        include_str!("impl206_fixtures/01_mixed_child_created.zil"),
    ),
    (
        "two-whole-transfers",
        include_str!("impl206_fixtures/02_whole_transfer_twice_created.zil"),
    ),
    (
        "three-wrappers",
        include_str!("impl206_fixtures/03_child_only_wrappers_created.zil"),
    ),
];

#[test]
fn exact_frozen_complete_products_reach_independent_raw_and_wire_verify() {
    for &(label, text) in POSITIVES {
        let module = typed(label, text);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        raw_wire(label, module, plan, None);
    }
}

#[test]
fn incoming_and_unknown_call_leaves_cannot_forge_create_with_a_candidate() {
    for (label, text) in [
        (
            "incoming",
            include_str!("impl206_fixtures/01_mixed_child_incoming.zil"),
        ),
        (
            "incoming-after-two-transfers",
            include_str!("impl206_fixtures/02_whole_transfer_twice_incoming.zil"),
        ),
        (
            "incoming-after-three-wrappers",
            include_str!("impl206_fixtures/03_child_only_wrappers_incoming.zil"),
        ),
        (
            "unknown-call-leaves",
            include_str!("impl206_fixtures/04_unknown_ordinary_calls_do_not_create.zil"),
        ),
    ] {
        let module = typed(label, text);
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert!(errors.iter().any(|d| {
            d.phase == Phase::Analyze
                && d.message
                    .contains("create result forwards an incoming responsibility")
        }));
        // Obtain ordinary control/loan claims with Give, then deliberately
        // submit the original Create module. Candidate claims cannot certify it.
        let mut ordinary = module.clone();
        let mut changed = 0;
        for function in &mut ordinary.functions {
            if function.name == "origin" && function.result_mode == PassingMode::Create {
                function.result_mode = PassingMode::Give;
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
        let plan = zil_analyze::analyze(&ordinary, &Limits::default()).unwrap();
        raw_wire(
            label,
            module,
            plan,
            Some((
                "E-VERIFY-RESOURCE",
                "create result is not independently proven fresh",
            )),
        );
    }
}

#[test]
fn unused_nested_metadata_has_no_source_or_candidate_exemption() {
    let base = typed("unused-base", POSITIVES[0].1);
    let plan = zil_analyze::analyze(&base, &Limits::default()).unwrap();
    let product = base.functions.iter().flat_map(|f| &f.values).find(|ty| matches!(ty, Type::Package {fields, ..} if fields.iter().any(|(_, f)|matches!(f, Type::Package {..})))).unwrap().clone();
    for mode in 0..3 {
        let mut malformed = product.clone();
        if let Type::Package { name, fields } = &mut malformed {
            match mode {
                0 => name.clear(),
                1 => fields[1].0 = fields[0].0.clone(),
                _ => {
                    let child = fields
                        .iter_mut()
                        .find_map(|(_, ty)| {
                            if let Type::Package { fields, .. } = ty {
                                Some(fields)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    child[0].0.clear();
                }
            }
        }
        let mut module = base.clone();
        module.functions[0].values.push(malformed);
        // The new value is unused and undefined. Type identity must reject it
        // before executable SSA or candidate-plan checks can hide the defect.
        raw_wire(
            &format!("unused-malformed-{mode}"),
            module,
            plan.clone(),
            Some((
                "E-VERIFY-PACKAGE",
                if mode == 0 {
                    "empty package identity"
                } else {
                    "unique nonempty names"
                },
            )),
        );
    }
}

#[test]
fn real_product_verify_work_has_exact_n_minus_one_and_fresh_boundaries() {
    for &(label, source) in &POSITIVES[2..] {
        let module = typed(label, source);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let run = |work| {
            zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &module.target,
                &Limits {
                    max_analysis_steps: work,
                    ..Limits::default()
                },
            )
            .map(|verified| {
                (
                    verified.module().clone(),
                    verified.plan().clone(),
                    verified.fingerprint(),
                )
            })
        };
        let expected = run(Limits::default().max_analysis_steps).unwrap();
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(
                        errors.iter().all(|d| d.phase == Phase::Verify
                            && d.category == ResultCategory::ResourceLimit),
                        "{errors:#?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        let below = run(high - 1).unwrap_err();
        assert!(
            below
                .iter()
                .all(|d| d.phase == Phase::Verify && d.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap(), expected);
        let out = directory(&format!("boundary-{label}"));
        fs::write(out.join("source.zil"), source).unwrap();
        fs::write(out.join("boundary.json"),format!("{{\"verify_work\":{high},\"checks\":\"default == N == fresh N; N-1 Verify ResourceLimit\"}}\n")).unwrap();
        fs::write(out.join("below.txt"), format!("{below:#?}")).unwrap();
        eprintln!("{label}: verify_work={high}");
    }
}

#[test]
fn original_flat186_verify_boundary_stays_10398_without_product_leaf_table() {
    // Exact source built by review186_field_origins::baseline, unchanged in
    // accepted205@2db3. Its original workspace02 boundaries.txt records
    // Analyze8673 / Verify10398; this is a fixed historical control.
    let source = "resource R{n:i32}package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let q=p;return give q.fresh}fn main(){}";
    let module = typed("flat186", source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let run = |n| {
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|v| (v.module().clone(), v.plan().clone(), v.fingerprint()))
    };
    let expected = run(Limits::default().max_analysis_steps).unwrap();
    assert_eq!(run(10398).unwrap(), expected);
    let below = run(10397).unwrap_err();
    assert!(
        below
            .iter()
            .all(|d| d.phase == Phase::Verify && d.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(run(10398).unwrap(), expected);
    let out = directory("old-flat186-boundary");
    fs::write(out.join("source.zil"), source).unwrap();
    fs::write(out.join("boundary.json"),"{\"verify_work\":10398,\"old_verify_work\":10398,\"checks\":\"default == original N == fresh N; N-1 Verify ResourceLimit\"}\n").unwrap();
    fs::write(out.join("below.txt"), format!("{below:#?}")).unwrap();
}

//! Additional independent 339 attacks. The earlier prepared harness remains
//! byte-identical to the exact 337 review; its two 339 tests are reused once.
#[path = "raw.rs"]
mod prepared;

use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{InstructionKind, Type};

#[test]
fn review339_new_capture_work_has_exact_fresh_budget() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "review-budget.zil",
        include_str!("thread-raw-helper-return-seed.zil"),
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let defaults = Limits::default();
    let target = TargetProfile::default();
    let core = zil_types::infer_source_closures(&core, &defaults).unwrap();
    let core = zil_ct::evaluate_source_constants(&core, &target, &defaults).unwrap();
    let module = zil_types::check(&core, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    for phase in 0..3 {
        let run = |n| {
            let selected = Limits {
                max_analysis_steps: n,
                ..defaults.clone()
            };
            match phase {
                0 => zil_types::check(&core, &target, &selected).map(|actual| {
                    assert_eq!(actual, module);
                }),
                1 => zil_analyze::analyze(&module, &selected).map(|actual| {
                    assert_eq!(actual, plan);
                }),
                _ => zil_verify::verify_module(module.clone(), plan.clone(), &target, &selected)
                    .map(|_| ()),
            }
        };
        let (mut low, mut high) = (0, defaults.max_analysis_steps);
        while low + 1 < high {
            let selected = low + (high - low) / 2;
            match run(selected) {
                Ok(()) => high = selected,
                Err(errors) => {
                    assert_eq!(
                        errors[0].category,
                        ResultCategory::ResourceLimit,
                        "{errors:?}"
                    );
                    low = selected;
                }
            }
        }
        run(high).unwrap();
        let errors = run(high - 1).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "{errors:?}"
        );
        run(high).unwrap();
        println!(
            "{} N={high}; N-1 ResourceLimit; fresh N Success",
            ["Types", "Analyze", "Verify"][phase]
        );
    }
}

#[test]
fn review339_returned_mutable_capture_retains_its_actual_exclusive_root() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "review-helper-return.zil",
        include_str!("thread-raw-helper-return-seed.zil"),
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let limits = Limits::default();
    let core = zil_types::infer_source_closures(&core, &limits).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &limits).unwrap();
    let module = zil_types::check(&core, &TargetProfile::default(), &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let positive_wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&positive_wire, &module.target, &limits).unwrap();
    let mut changed = module.clone();
    let caller = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let y = caller.locals.iter().find(|l| l.name == "y").unwrap().id;
    let poison = caller
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, InstructionKind::BorrowLocal { local } if local == y)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    assert_eq!(
        caller.values[poison.0 as usize],
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32)
        }
    );
    assert!(
        caller
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::Call { .. })
                && i.result.is_some_and(|v| matches!(
                    caller.values[v.0 as usize],
                    Type::Address { mutable: true, .. }
                )))
    );
    let scope = caller
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
        .unwrap();
    let InstructionKind::WithScopedThread { argument, .. } = &mut scope.kind else {
        unreachable!()
    };
    assert_ne!(*argument, poison);
    *argument = poison;
    let errors = zil_analyze::analyze(&changed, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::SafeReject
            && d.code == "E-BORROW-CONFLICT"),
        "{errors:?}"
    );
    println!("Analyze: E-BORROW-CONFLICT for helper-returned mutable capture");
    let mut claim = plan;
    claim.module_fingerprint = changed.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&changed, &claim, &limits).unwrap();
    for (form, result) in [
        (
            "raw",
            zil_verify::verify_module(changed.clone(), claim, &changed.target, &limits),
        ),
        (
            "Wire",
            zil_verify::verify_module_bytes(&wire, &changed.target, &limits),
        ),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == "E-BORROW-CONFLICT"),
            "{form}: {errors:?}"
        );
        println!("{form}: E-BORROW-CONFLICT for helper-returned mutable capture");
    }
}

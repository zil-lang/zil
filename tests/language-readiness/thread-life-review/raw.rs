//! Independent source-derived raw/Wire attacks. Build standalone against the
//! exact candidate's existing rlibs; no new Cargo target or implementation code.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, InstructionKind, Type, TypedModule};

fn typed(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "review.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

fn positive(module: &TypedModule) -> CandidatePlan {
    let limits = Limits::default();
    let plan = zil_analyze::analyze(module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    plan
}

fn verify_reject(module: TypedModule, mut claim: CandidatePlan, code: &str) {
    let limits = Limits::default();
    claim.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &claim, &limits).unwrap();
    for (form, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), claim, &module.target, &limits),
        ),
        (
            "Wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == code),
            "{form}: {errors:?}"
        );
        println!("{form}: {code}");
    }
}

fn both_reject(module: TypedModule, claim: CandidatePlan, analyze: &str, verify: &str) {
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::SafeReject
            && d.code == analyze),
        "{errors:?}"
    );
    println!("Analyze: {analyze}");
    verify_reject(module, claim, verify);
}

#[test]
fn mixed_upper_bound_cannot_replace_actual_return_origin() {
    let module = typed(include_str!("life-mixed-upper-bound.zil"));
    let plan = positive(&module);
    let mut changed = module.clone();
    let function = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "choose")
        .unwrap();
    assert_eq!(function.reference_return_bound, Some(vec![0, 1]));
    function.reference_return_bound = Some(vec![0]);
    both_reject(changed, plan, "E-LIFE-RETURN", "E-VERIFY-LIFE");
}

#[test]
fn actual_shared_input_cannot_ground_mutable_return() {
    let module = typed(include_str!("life-raw-mode-seed.zil"));
    let plan = positive(&module);
    let mut changed = module.clone();
    let function = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "reborrow")
        .unwrap();
    let parameter = function.parameters[0];
    let shared = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    function.locals[parameter.0 as usize].ty = shared.clone();
    let mut changed_loads = 0;
    for block in &function.blocks {
        for instruction in &block.instructions {
            if matches!(instruction.kind, InstructionKind::Load(local) if local == parameter) {
                function.values[instruction.result.unwrap().0 as usize] = shared.clone();
                changed_loads += 1;
            }
        }
    }
    assert!(changed_loads > 0);
    assert!(
        function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                InstructionKind::ReborrowAddress { mutable: true, .. }
            ))
    );
    both_reject(changed, plan, "E-LIFE-RETURN", "E-VERIFY-ADDRESS");
}

#[test]
fn declared_result_mode_is_verified_independently_of_candidate_analysis() {
    let module = typed(include_str!("life-raw-mode-seed.zil"));
    let plan = positive(&module);
    let mut changed = module.clone();
    changed
        .functions
        .iter_mut()
        .find(|f| f.name == "reborrow")
        .unwrap()
        .return_type = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    // Analyze is not a complete raw instruction type validator. Its successful
    // candidate must still fail the independent return-mode verifier.
    zil_analyze::analyze(&changed, &Limits::default()).unwrap();
    verify_reject(changed, plan, "E-VERIFY-RETURN");
}

#[test]
fn single_scope_argument_substitution_exposes_final_use_alias() {
    let module = typed(include_str!("thread-raw-disjoint-seed.zil"));
    let plan = positive(&module);
    let mut changed = module.clone();
    let function = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let y = function.locals.iter().find(|l| l.name == "y").unwrap().id;
    let poison = function
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
        function.values[poison.0 as usize],
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32)
        }
    );
    let scope = function
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
    // No source, capture, signature, Hold row or producer was fabricated. The
    // sole changed operand makes existing immutable payload and mutable body
    // captures alias while both are at their final use.
    both_reject(changed, plan, "E-BORROW-CONFLICT", "E-BORROW-CONFLICT");
}

#[test]
fn same_signature_worker_substitution_rechecks_reachable_foreign_dependency() {
    let module = typed(include_str!("thread-raw-worker-seed.zil"));
    let plan = positive(&module);
    let mut changed = module.clone();
    let other = changed
        .functions
        .iter()
        .find(|f| f.name == "unsafe_worker")
        .unwrap()
        .id;
    let caller = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let scope = caller
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
        .unwrap();
    let InstructionKind::WithScopedThread { worker, .. } = &mut scope.kind else {
        unreachable!()
    };
    *worker = other;
    both_reject(changed, plan, "E-THREAD-WORKER", "E-THREAD-WORKER");
}

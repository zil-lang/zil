//! Raw executable/Wire controls for the first actual physical stored-leaf proof.
//! Source/Types/native coverage is recorded separately; this fixture is hand IR.
use super::*;
use zil_ir::{CandidatePlan, Terminator, Type, ValueId};
mod fixture {
    include!("stored_flow_fixture.rs");
}

#[test]
fn terminal342_ordinary_stored_view_source_has_an_inline_pointer_and_native_load() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/terminal/342/stored_view.zil");
    let mut backends = vec![zil_driver::NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(zil_driver::NativeBackend::LlvmApi);
    }
    let temp = Temp::new();
    for backend in backends {
        for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
            let report = zil_driver::compile_path(
                &path,
                &CompileOptions {
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    backend,
                    optimization,
                    output: Some(temp.0.join("stored-view")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            execute(&report.artifacts["executable"], 42);
            let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
            if let Some(retained) = std::env::var_os("ZIL342_RETAIN_NATIVE") {
                let retained = PathBuf::from(retained);
                fs::create_dir_all(&retained).unwrap();
                let stem = format!("view-{backend:?}-{optimization:?}");
                fs::write(retained.join(format!("{stem}.ll")), &llvm).unwrap();
                fs::copy(
                    &report.artifacts["executable"],
                    retained.join(format!("{stem}.bin")),
                )
                .unwrap();
                fs::write(retained.join(format!("{stem}.json")), report.to_json()).unwrap();
                for key in ["typed", "core", "lir", "typed_ir", "core_ir", "lir_ir"] {
                    if let Some(artifact) = report.artifacts.get(key) {
                        fs::copy(artifact, retained.join(format!("{stem}-{key}.txt"))).unwrap();
                    }
                }
            }
            assert!(
                llvm.contains("{ ptr }")
                    && llvm.contains("getelementptr")
                    && llvm.contains("load ptr, ptr"),
                "{llvm}"
            );
        }
    }
}

fn candidate(module: &TypedModule) -> CandidatePlan {
    zil_analyze::analyze(module, &Limits::default()).unwrap()
}

#[test]
fn terminal342_phantom_nominal_classes_preserve_metadata_without_target_roots() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/language-readiness/terminal/342");
    let positive = dir.join("phantom_stored_metadata.zil");
    let negative = dir.join("phantom_stored_metadata_escape.zil");
    let report = zil_driver::compile_path(&positive, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let error = zil_driver::compile_path(&negative, &CompileOptions::default());
    assert_eq!(
        error.status,
        ResultCategory::SafeReject,
        "{}",
        error.to_json()
    );
    assert!(
        error
            .diagnostics
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.message.contains("activation-local")),
        "{}",
        error.to_json()
    );
    let raw = fixture::phantom();
    let plan = accept(&raw);
    assert!(plan.functions[0].holds[1].roots.is_empty());
    let mut forged = plan.clone();
    forged.functions[0].holds[1]
        .roots
        .push(HoldRoot::Parameter(0));
    let wire = zil_ir::wire::encode_module_candidate(&raw, &forged, &Limits::default()).unwrap();
    assert!(
        zil_verify::verify_module(raw.clone(), forged, &raw.target, &Limits::default()).is_err()
    );
    assert!(zil_verify::verify_module_bytes(&wire, &raw.target, &Limits::default()).is_err());
    let mut escaped = raw.clone();
    escaped.functions[0].return_type = fixture::address(Type::I32, false);
    escaped.functions[0].reference_return_bound = Some(vec![0]);
    escaped.functions[0].values[4] = fixture::address(Type::I32, false);
    escaped.functions[0].blocks[0].instructions[5].kind = InstructionKind::ReborrowAddress {
        address: ValueId(3),
        mutable: false,
    };
    escaped.functions[1].values[2] = fixture::address(Type::I32, false);
    escaped.functions[1].values.push(Type::I32);
    escaped.functions[1].blocks[0]
        .instructions
        .push(fixture::instruction(
            Some(3),
            InstructionKind::ReadAddress {
                address: ValueId(2),
            },
        ));
    escaped.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
    let mut claim = plan;
    claim.functions[1].holds.push(zil_ir::ValueHold {
        value: ValueId(3),
        roots: vec![],
    });
    reject(
        &escaped,
        claim,
        "nominal phantom class cannot turn its activation-local field into input target permission",
    );
}
fn accept(module: &TypedModule) -> CandidatePlan {
    let plan = candidate(module);
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &Limits::default()).unwrap();
    let (decoded, decoded_plan) =
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    assert_eq!(&decoded, module);
    assert_eq!(decoded_plan, plan);
    assert_eq!(
        zil_analyze::analyze(&decoded, &Limits::default()).unwrap(),
        plan
    );
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    plan
}
fn reject(module: &TypedModule, mut plan: CandidatePlan, text: &str) {
    let errors = zil_analyze::analyze(module, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().all(|d| d.phase == Phase::Analyze),
        "{text}: {errors:?}"
    );
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &Limits::default()).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    assert!(
        zil_analyze::analyze(&decoded, &Limits::default()).is_err(),
        "{text}"
    );
    for errors in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default())
            .unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors.iter().all(|d| d.phase == Phase::Verify),
            "{text}: {errors:?}"
        );
    }
}

#[test]
fn terminal342_physical_shared_targets_survive_carrier_end_and_forward_calls() {
    let module = fixture::module();
    let plan = accept(&module);
    let verified = zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
    assert!(llvm.contains("getelementptr") && llvm.contains("load ptr, ptr"));
    let mut erased = lir.clone();
    erased.functions[1].storage_life = None;
    assert!(
        zil_backend_llvm::emit(&verified, &erased)
            .unwrap_err()
            .iter()
            .any(|d| d.code == "E-CODEGEN-UNBOUND-LIR")
    );
    let mut replaced = lir;
    replaced.functions[1].blocks[0].instructions[1].kind = InstructionKind::LoadStoredBorrow {
        storage: ValueId(0),
        path: vec![zil_ir::PlaceProjection::Field(1)],
        mode: zil_ir::StoredBorrowLoad::CopyShared,
    };
    assert!(
        zil_backend_llvm::emit(&verified, &replaced)
            .unwrap_err()
            .iter()
            .any(|d| d.code == "E-CODEGEN-UNBOUND-LIR")
    );
    assert_eq!(
        plan.functions[3].holds[4].roots,
        vec![HoldRoot::Local(LocalId(0))]
    );
    let positive = fixture::exclusive_returns(false);
    let plan = accept(&positive);
    let negative = fixture::exclusive_returns(true);
    reject(
        &negative,
        plan,
        "second call cannot reuse an exclusive parent while its returned child is live",
    );
}

#[test]
fn terminal342_physical_stored_actual_roots_modes_and_complete_inventory_are_independent() {
    let original = fixture::module();
    let plan = accept(&original);
    let mut changed = original.clone();
    changed.life_instances.clear();
    reject(&changed, plan.clone(), "missing complete nominal instance");
    let mut changed = original.clone();
    changed.life_instances[0].arguments[0] = zil_ir::LifeNominalActual::Type(Type::I64);
    reject(
        &changed,
        plan.clone(),
        "actual generic kind differs from full declaration ordinal",
    );
    let mut changed = original.clone();
    changed.life_instances[0].arguments[0] = zil_ir::LifeNominalActual::Lifetime(1);
    reject(
        &changed,
        plan.clone(),
        "nominal class outside actual domain",
    );
    let mut changed = original.clone();
    changed
        .life_declarations
        .push(changed.life_declarations[0].clone());
    reject(
        &changed,
        plan.clone(),
        "duplicate stable declaration identity",
    );
    let mut changed = original.clone();
    changed.functions[1].storage_life.as_mut().unwrap().bindings[2].binder =
        zil_ir::StorageBinder::Invocation(0);
    reject(
        &changed,
        plan.clone(),
        "stored target cannot acquire receiver invocation bound",
    );
    let mut changed = original.clone();
    let f = &mut changed.functions[3];
    f.values.push(fixture::address(Type::I64, true));
    f.blocks[0].instructions.insert(
        8,
        fixture::instruction(
            Some(6),
            InstructionKind::BorrowLocalMut { local: LocalId(0) },
        ),
    );
    let mut forged = plan.clone();
    forged.functions[3].holds.push(zil_ir::ValueHold {
        value: ValueId(6),
        roots: vec![HoldRoot::Local(LocalId(0))],
    });
    reject(
        &changed,
        forged,
        "CopyShared retains the actual target parent after carrier teardown",
    );
    let mut changed = original.clone();
    let f = &mut changed.functions[4];
    f.values.push(fixture::address(Type::I64, false));
    f.blocks[0].instructions.push(fixture::instruction(
        Some(1),
        InstructionKind::Call {
            function: zil_ir::FunctionId(4),
            arguments: vec![ValueId(0)],
        },
    ));
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    let mut forged = plan;
    forged.functions[4].holds.push(zil_ir::ValueHold {
        value: ValueId(1),
        roots: vec![HoldRoot::Parameter(0)],
    });
    reject(
        &changed,
        forged,
        "recursive bottom cannot be claimed as an actual returned parameter",
    );
}

#[test]
fn terminal342_physical_stored_current_cfg_is_reproved_and_work_has_fresh_boundaries() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        phase: Phase,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let expected = run(Limits::default().max_analysis_steps).unwrap();
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(value) => {
                    assert_eq!(value, expected);
                    high = mid
                }
                Err(d) => {
                    assert!(
                        d.iter().all(
                            |x| x.phase == phase && x.category == ResultCategory::ResourceLimit
                        ),
                        "{d:?}"
                    );
                    low = mid
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|x| x.phase == phase && x.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap(), expected);
        high
    }
    let original = fixture::module();
    let plan = accept(&original);
    let n = boundary(Phase::Analyze, |n| {
        zil_analyze::analyze(
            &original,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    });
    let v = boundary(Phase::Verify, |n| {
        zil_verify::verify_module(
            original.clone(),
            plan.clone(),
            &original.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|v| v.fingerprint())
    });
    println!(
        "342 physical shared stored raw N Analyze={n} Verify={v}; N-1 ResourceLimit/fresh N identical"
    );
    let mut session = zil_analyze::AnalysisSession::default();
    let warm = session.analyze(&original, &Limits::default()).unwrap();
    assert_eq!(warm, plan);
    let mut changed = original.clone();
    let f = &mut changed.functions[1];
    f.locals.push(zil_ir::LocalDecl {
        id: LocalId(1),
        name: "activation".into(),
        ty: Type::I64,
        mutable: false,
        span: f.span,
    });
    f.values.push(Type::I64);
    f.blocks[0].instructions.pop();
    f.blocks[0].instructions.extend([
        fixture::instruction(Some(2), InstructionKind::Constant(zil_ir::Constant::I64(7))),
        fixture::instruction(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(2),
            },
        ),
        fixture::instruction(Some(1), InstructionKind::BorrowLocal { local: LocalId(1) }),
    ]);
    assert!(session.analyze(&changed, &Limits::default()).is_err());
    let mut forged = plan;
    forged.functions[1].blocks[0]
        .initialized_out
        .push(LocalId(1));
    forged.functions[1].holds.push(zil_ir::ValueHold {
        value: ValueId(2),
        roots: vec![],
    });
    reject(
        &changed,
        forged,
        "cached symbolic role cannot replace actual activation-local escape proof",
    );
}

#[test]
fn terminal342_unreachable_instructions_cannot_extend_the_stored_native_comparison_profile() {
    let original = fixture::module();
    let mut plan = accept(&original);
    let bad = fixture::unimplemented_unreachable();
    let error = zil_analyze::analyze(&bad, &Limits::default()).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.message.contains("including in unreachable")),
        "{error:?}"
    );
    plan.functions[3].blocks.push(zil_ir::BlockPlan {
        block: zil_ir::BlockId(1),
        initialized_in: vec![],
        initialized_out: vec![],
    });
    for value in [6, 7] {
        plan.functions[3].holds.push(zil_ir::ValueHold {
            value: ValueId(value),
            roots: vec![],
        });
    }
    plan.functions[4].blocks[0].initialized_in = (0..64).map(LocalId).collect();
    plan.functions[4].blocks[0].initialized_out = (0..64).map(LocalId).collect();
    plan.functions[4].holds[0].roots.clear();
    plan.module_fingerprint = bad.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&bad, &plan, &Limits::default()).unwrap();
    for errors in [
        zil_verify::verify_module(bad.clone(), plan, &bad.target, &Limits::default()).unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &bad.target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("including unreachable")),
            "{errors:?}"
        );
    }
}

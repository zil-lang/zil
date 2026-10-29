//! Independent hand-filled candidates. No Types or Analyze supplies this plan.
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;
static NEXT: AtomicU64 = AtomicU64::new(0);
fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn op(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(),
    }
}
fn function(id: u32, result: Type, locals: &[Type], parameters: &[u32]) -> TypedFunction {
    TypedFunction {
        id: FunctionId(id),
        name: format!("raw127_{id}"),
        foreign: None,
        environment: None,
        callable_contract: None,
        callable_return_contract: None,
        parameters: parameters.iter().copied().map(LocalId).collect(),
        return_type: result,
        result_mode: PassingMode::Infer,
        locals: locals
            .iter()
            .enumerate()
            .map(|(i, ty)| LocalDecl {
                id: LocalId(i as u32),
                name: format!("local{i}"),
                ty: ty.clone(),
                mutable: !parameters.contains(&(i as u32)),
                span: span(),
            })
            .collect(),
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Trap,
        }],
        entry: BlockId(0),
        span: span(),
    }
}
fn fixture(parameter_alias: bool) -> TypedModule {
    let env = Type::Closure(ClosureId(0));
    let mut helper = function(0, Type::I32, &[env.clone(), env.clone()], &[0]);
    helper.values = vec![
        Type::I32,
        env.clone(),
        env.clone(),
        Type::I32,
        env.clone(),
        Type::I32,
        Type::I32,
    ];
    helper.blocks[0].instructions = vec![
        op(None, InstructionKind::StorageLive(LocalId(1))),
        op(Some(0), InstructionKind::Constant(Constant::I32(5))),
        op(Some(1), InstructionKind::Load(LocalId(0))),
        op(
            Some(2),
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(0)],
            },
        ),
        op(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(if parameter_alias { 1 } else { 2 }),
            },
        ),
        op(
            Some(3),
            InstructionKind::CallClosure {
                callee: ValueId(1),
                arguments: vec![],
            },
        ),
        op(Some(4), InstructionKind::Load(LocalId(1))),
        op(
            Some(5),
            InstructionKind::CallClosure {
                callee: ValueId(4),
                arguments: vec![],
            },
        ),
        op(
            Some(6),
            InstructionKind::Binary {
                op: BinaryOp::Add,
                left: ValueId(3),
                right: ValueId(5),
            },
        ),
        op(None, InstructionKind::StorageDead(LocalId(1))),
    ];
    helper.blocks[0].terminator = Terminator::Return(Some(ValueId(6)));
    helper.callable_contract = Some(CallableParameterContract {
        roles: vec![CallableRole {
            parameter: 0,
            closure: ClosureId(0),
            kind: CallableKind::Fn,
            hold: vec![],
            call: vec![],
            writes: vec![],
            cleanup: CallableCleanup::Owned,
        }],
        traces: zil_ir::CallableTraces::Finite(vec![CallableTrace {
            calls: vec![
                zil_ir::CallEvent {
                    parameter: 0,
                    kind: CallableKind::Fn
                };
                if parameter_alias { 2 } else { 1 }
            ],
            exit: CallableExit::Return,
        }]),
    });
    let mut factory = function(1, env.clone(), &[Type::I32], &[0]);
    factory.values = vec![Type::I32, env];
    factory.blocks[0].instructions = vec![
        op(Some(0), InstructionKind::Load(LocalId(0))),
        op(
            Some(1),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![ValueId(0)],
            },
        ),
    ];
    factory.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    factory.callable_return_contract = Some(CallableReturnContract {
        lending_captures: Vec::new(),
        closure: ClosureId(0),
        kind: CallableKind::Fn,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
    });
    let mut entry = function(2, Type::I32, &[], &[]);
    entry.environment = Some(ClosureId(0));
    entry.values = vec![Type::I32];
    entry.blocks[0].instructions = vec![op(Some(0), InstructionKind::ReadCapture { index: 0 })];
    entry.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"127 hand-filled local factory instances"),
        target: TargetProfile::host_llvm(),
        functions: vec![helper, factory, entry],
        closures: vec![ClosureDefinition {
            lending_captures: Vec::new(),
            id: ClosureId(0),
            function: FunctionId(2),
            captures: vec![Type::I32],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::Fn,
        }],
        resources: vec![],
        unsupported: vec![],
    }
}
// Fixtures are straight-line and Copy-only. This constructs their literal
// initialization/empty-Hold claims without invoking either production solver.
fn plan(module: &TypedModule) -> CandidatePlan {
    let mut plan = CandidatePlan::empty_for(module);
    for f in &module.functions {
        assert_eq!(f.blocks.len(), 1);
        let mut initialized: BTreeSet<_> = f.parameters.iter().copied().collect();
        for instruction in &f.blocks[0].instructions {
            match instruction.kind {
                InstructionKind::Store { local, .. } => {
                    initialized.insert(local);
                }
                InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                    initialized.remove(&local);
                }
                _ => {}
            }
        }
        plan.functions.push(FunctionPlan {
            function: f.id,
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: f.parameters.clone(),
                initialized_out: initialized.into_iter().collect(),
            }],
            holds: f
                .values
                .iter()
                .enumerate()
                .map(|(i, _)| ValueHold {
                    value: ValueId(i as u32),
                    roots: if f.id == FunctionId(0) && i == 7 {
                        vec![HoldRoot::Local(LocalId(1))]
                    } else {
                        vec![]
                    },
                })
                .collect(),
        });
    }
    plan
}
fn observe(label: &str, module: &TypedModule, expected: Option<(&str, &str)>) {
    let limits = Limits::default();
    let candidate = plan(module);
    let wire = wire::encode_module_candidate(module, &candidate, &limits).unwrap();
    let analyze = zil_analyze::analyze(module, &limits);
    let raw = zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits);
    let decoded = zil_verify::verify_module_bytes(&wire, &module.target, &limits);
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let dir = PathBuf::from(root).join(format!(
            "raw127-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("module.txt"), format!("{module:#?}")).unwrap();
        fs::write(dir.join("plan.txt"), format!("{candidate:#?}")).unwrap();
        fs::write(dir.join("candidate.wire"), wire).unwrap();
        fs::write(
            dir.join("outcomes.txt"),
            format!("analyze={analyze:?}\nraw={raw:?}\nwire={decoded:?}\n"),
        )
        .unwrap();
    }
    match expected {
        None => {
            analyze.unwrap();
            raw.unwrap();
            decoded.unwrap();
        }
        Some((analyze_text, verify_text)) => {
            let (analyze_category, verify_category) = match label {
                "mixed-slot" | "borrowed-local" => {
                    (ResultCategory::Unsupported, ResultCategory::Unsupported)
                }
                "nested-environment" | "external-capture" => {
                    (ResultCategory::SafeReject, ResultCategory::Unsupported)
                }
                _ => (ResultCategory::SafeReject, ResultCategory::SafeReject),
            };
            let a = analyze.unwrap_err();
            assert_eq!(a[0].phase, Phase::Analyze, "{label}: {a:?}");
            assert_eq!(a[0].category, analyze_category, "{label}: {a:?}");
            assert!(a[0].message.contains(analyze_text), "{label}: {a:?}");
            for outcome in [raw, decoded] {
                let errors = outcome.unwrap_err();
                assert_eq!(errors[0].phase, Phase::Verify, "{label}: {errors:?}");
                assert_eq!(errors[0].category, verify_category, "{label}: {errors:?}");
                assert!(
                    errors[0].message.contains(verify_text),
                    "{label}: {errors:?}"
                );
            }
        }
    }
}
#[test]
fn independent_same_id_instances_and_parameter_aliases_have_exact_counts() {
    observe("same-hidden-id", &fixture(false), None);
    observe("parameter-alias", &fixture(true), None);
    for alias in [false, true] {
        let mut m = fixture(alias);
        m.functions[0]
            .callable_contract
            .as_mut()
            .unwrap()
            .traces
            .as_finite_mut()
            .unwrap()[0]
            .calls = vec![
            zil_ir::CallEvent {
                parameter: 0,
                kind: CallableKind::Fn
            };
            if alias { 1 } else { 2 }
        ];
        observe(
            if alias {
                "alias-hidden-call"
            } else {
                "local-counted-as-parameter"
            },
            &m,
            Some(("trace claim differs", "traces differ")),
        );
    }
}
#[test]
fn raw_mixed_origins_cannot_erase_or_invent_parameter_calls() {
    let mut m = fixture(false);
    m.functions[0].blocks[0].instructions.insert(
        5,
        op(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(1),
            },
        ),
    );
    observe(
        "mixed-slot",
        &m,
        Some(("origin is unknown, mixed", "origin is unknown, mixed")),
    );
}
#[test]
fn raw_borrowed_local_and_nested_creation_keep_their_scope_boundary() {
    let mut borrowed = fixture(false);
    borrowed.functions[0].values.push(Type::Address {
        mutable: false,
        inner: Box::new(Type::Closure(ClosureId(0))),
    });
    borrowed.functions[0].blocks[0].instructions.insert(
        7,
        op(Some(7), InstructionKind::BorrowLocal { local: LocalId(1) }),
    );
    borrowed.functions[0].blocks[0].instructions[8].kind = InstructionKind::CallClosureBorrowed {
        callee: ValueId(7),
        kind: CallableKind::Fn,
        arguments: vec![],
    };
    observe(
        "borrowed-local",
        &borrowed,
        Some((
            "borrowed local factory result",
            "borrowed local factory result",
        )),
    );
    let mut nested = fixture(false);
    nested.functions[0].blocks[0].instructions[3].kind = InstructionKind::MakeClosure {
        closure: ClosureId(0),
        captures: vec![ValueId(0)],
    };
    observe(
        "nested-environment",
        &nested,
        Some(("helper-local nested", "helper-local nested")),
    );
}
#[test]
fn raw_factory_public_envelope_is_rechecked_before_local_calls() {
    let mut missing = fixture(false);
    missing.functions[1].callable_return_contract = None;
    observe(
        "missing-return-contract",
        &missing,
        Some((
            "no public callable return contract",
            "requires its independently checkable return contract",
        )),
    );
    let mut external = fixture(false);
    external.functions[1]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .hold
        .push(0);
    observe(
        "forged-external-envelope",
        &external,
        Some((
            "ordinary self-contained factory",
            "cannot advertise external dependencies",
        )),
    );
    let mut cleanup = fixture(false);
    cleanup.functions[1]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .push(0);
    observe(
        "forged-cleanup",
        &cleanup,
        Some(("cleanup differs", "cleanup must name every owned capture")),
    );
    let mut borrowed = fixture(false);
    borrowed.closures[0].captures[0] = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    observe(
        "external-capture",
        &borrowed,
        Some((
            "captures an external address or description",
            "self-contained finite Copy",
        )),
    );
}

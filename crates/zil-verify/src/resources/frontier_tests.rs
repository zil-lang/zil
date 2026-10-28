//! Scheduling equivalence over independent verifier resource transfers. These raw
//! finite IR fixtures are not source/native or whole-language evidence.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn payload() -> Type {
    Type::Record {
        name: "EffectToken".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}
fn desc() -> Type {
    Type::Desc(Box::new(payload()))
}
fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(),
    }
}
fn fixture(edges: &[Vec<(usize, bool)>], writes: &[u8]) -> TypedModule {
    let mut functions = Vec::new();
    for (id, calls) in edges.iter().enumerate() {
        let mut values = vec![desc(), desc(), Type::I32];
        let mut instructions = vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(Some(1), InstructionKind::Load(LocalId(1))),
            instruction(Some(2), InstructionKind::Constant(Constant::I32(1))),
        ];
        for p in 0..2 {
            if writes[id] & (1 << p) != 0 {
                instructions.push(instruction(
                    None,
                    InstructionKind::WriteDesc {
                        desc: ValueId(p),
                        index: 0,
                        value: ValueId(2),
                    },
                ));
            }
        }
        for &(callee, swap) in calls {
            instructions.push(instruction(
                Some(values.len() as u32),
                InstructionKind::Call {
                    function: FunctionId(callee as u32),
                    arguments: if swap {
                        vec![ValueId(1), ValueId(0)]
                    } else {
                        vec![ValueId(0), ValueId(1)]
                    },
                },
            ));
            values.push(Type::I32);
        }
        functions.push(TypedFunction {
            id: FunctionId(id as u32),
            name: format!("f{id}"),
            foreign: None,
            environment: None,
            callable_contract: None,
            callable_return_contract: None,
            parameters: vec![LocalId(0), LocalId(1)],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: (0..2)
                .map(|id| LocalDecl {
                    id: LocalId(id),
                    name: format!("p{id}"),
                    ty: desc(),
                    mutable: false,
                    span: span(),
                })
                .collect(),
            values,
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator: Terminator::Return(Some(ValueId(2))),
            }],
            entry: BlockId(0),
            span: span(),
        });
    }
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions,
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "EffectToken".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn graphs(module: &TypedModule) -> Vec<FunctionGraph> {
    module
        .functions
        .iter()
        .map(|f| FunctionGraph {
            successors: vec![vec![]; f.blocks.len()],
            predecessors: vec![vec![]; f.blocks.len()],
            reachable: vec![true; f.blocks.len()],
            package_views: packages::CaptureViews::default(),
        })
        .collect()
}
fn compare(module: &TypedModule) -> (usize, usize) {
    let graphs = graphs(module);
    let limits = Limits {
        max_analysis_steps: 100_000_000,
        ..Limits::default()
    };
    TEST_EFFECT_SOLVES.with(|value| value.set(0));
    let actual = infer_effects(module, &graphs, &mut Budget::new(&limits)).unwrap();
    let queued = TEST_EFFECT_SOLVES.with(|value| value.get());
    TEST_EFFECT_SOLVES.with(|value| value.set(0));
    let reference = infer_effects_full_scan(module, &graphs, &mut Budget::new(&limits)).unwrap();
    let scanned = TEST_EFFECT_SOLVES.with(|value| value.get());
    assert_eq!(
        actual, reference,
        "complete input/capture effect rows differ"
    );
    for (function, graph) in module.functions.iter().zip(&graphs) {
        let left = run_function(
            module,
            function,
            graph,
            &actual,
            true,
            &mut Budget::new(&limits),
        );
        let right = run_function(
            module,
            function,
            graph,
            &reference,
            true,
            &mut Budget::new(&limits),
        );
        assert_eq!(left, right, "final validate diagnostic differs");
    }
    (queued, scanned)
}

#[test]
fn reverse_chain_revisits_only_affected_predecessors() {
    let n = 48;
    let edges: Vec<_> = (0..n)
        .map(|i| {
            if i + 1 < n {
                vec![(i + 1, false)]
            } else {
                vec![]
            }
        })
        .collect();
    let mut writes = vec![0; n];
    writes[n - 1] = 1;
    let module = fixture(&edges, &writes);
    let (queued, scanned) = compare(&module);
    assert_eq!(queued, 2 * n - 1);
    assert_eq!(scanned, n * (n + 1));
    println!(
        "080 Verify reverse48 complete-summary solves: frontier={queued}, full_scan={scanned}"
    );
}

#[test]
fn forward_star_disconnected_duplicate_and_recursive_effect_graphs() {
    let cases = [
        vec![vec![], vec![(0, false)], vec![(1, false)], vec![(2, false)]],
        vec![
            vec![(3, false), (3, false)],
            vec![(3, true)],
            vec![],
            vec![],
        ],
        vec![vec![(1, false)], vec![(0, true)], vec![], vec![(3, true)]],
        vec![vec![(0, true)], vec![], vec![(3, false)], vec![(2, true)]],
    ];
    for edges in cases {
        compare(&fixture(&edges, &[1, 0, 0, 2]));
    }
}

#[test]
fn all_small_dependency_graphs_match_complete_reference_rows() {
    // Every directed graph on three vertices, including self-edges, with a
    // nontrivial argument permutation. This is 512 actual transfer fixed points.
    for mask in 0usize..512 {
        let edges: Vec<_> = (0..3)
            .map(|a| {
                (0..3)
                    .filter(|&b| mask & (1 << (a * 3 + b)) != 0)
                    .map(|b| (b, (a + b) % 2 == 0))
                    .collect()
            })
            .collect();
        compare(&fixture(&edges, &[1, 0, 2]));
    }
}

#[test]
fn unreachable_call_does_not_create_a_summary_consumer() {
    let mut module = fixture(&[vec![], vec![]], &[0, 1]);
    let mut dead = module.functions[0].blocks[0].clone();
    dead.id = BlockId(1);
    dead.instructions.push(instruction(
        Some(3),
        InstructionKind::Call {
            function: FunctionId(1),
            arguments: vec![ValueId(0), ValueId(1)],
        },
    ));
    module.functions[0].values.push(Type::I32);
    module.functions[0].blocks.push(dead);
    let mut gs = graphs(&module);
    gs[0].reachable[1] = false;
    let consumers = effect_dependents(&module, &gs, &mut Budget::new(&Limits::default())).unwrap();
    assert_eq!(consumers, vec![Vec::<usize>::new(), vec![]]);
    assert_eq!(
        infer_effects(&module, &gs, &mut Budget::new(&Limits::default())).unwrap(),
        infer_effects_full_scan(&module, &gs, &mut Budget::new(&Limits::default())).unwrap()
    );
}

#[test]
fn every_concrete_closure_call_has_its_entry_dependency() {
    let mut module = fixture(&[vec![], vec![]], &[0, 1]);
    module.closures.push(ClosureDefinition {
        lending_captures: Vec::new(),
        id: ClosureId(0),
        function: FunctionId(1),
        kind: CallableKind::FnMut,
        captures: vec![desc()],
        parameters: vec![],
        result: Type::I32,
    });
    let f = &mut module.functions[0];
    f.locals[0].ty = Type::Closure(ClosureId(0));
    f.values[0] = Type::Closure(ClosureId(0));
    f.values[1] = Type::Address {
        mutable: true,
        inner: Box::new(Type::Closure(ClosureId(0))),
    };
    for kind in [
        InstructionKind::CallClosureShared {
            callee: LocalId(0),
            arguments: vec![],
        },
        InstructionKind::CallClosureMut {
            callee: LocalId(0),
            arguments: vec![],
        },
        InstructionKind::CallClosure {
            callee: ValueId(0),
            arguments: vec![],
        },
        InstructionKind::CallClosureOnce {
            callee: ValueId(0),
            arguments: vec![],
        },
        InstructionKind::CallClosureBorrowed {
            callee: ValueId(1),
            kind: CallableKind::FnMut,
            arguments: vec![],
        },
    ] {
        module.functions[0].blocks[0].instructions = vec![instruction(None, kind)];
        let found = effect_dependents(
            &module,
            &graphs(&module),
            &mut Budget::new(&Limits::default()),
        )
        .unwrap();
        assert_eq!(found[1], vec![0]);
    }
}

#[test]
fn module_budget_boundary_and_fresh_request_are_exact() {
    let module = fixture(&[vec![(1, false)], vec![(2, true)], vec![]], &[0, 0, 1]);
    let plan = crate::resource_tests::plan(&module);
    let target = TargetProfile::host_llvm();
    let mut low = 0;
    let mut high = 100_000;
    while low + 1 < high {
        let mid = (low + high) / 2;
        let limits = Limits {
            max_analysis_steps: mid,
            ..Limits::default()
        };
        if verify_module(module.clone(), plan.clone(), &target, &limits).is_ok() {
            high = mid;
        } else {
            low = mid;
        }
    }
    let limits = Limits {
        max_analysis_steps: high,
        ..Limits::default()
    };
    verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let failure = verify_module(
        module.clone(),
        plan.clone(),
        &target,
        &Limits {
            max_analysis_steps: high - 1,
            ..limits.clone()
        },
    )
    .unwrap_err();
    assert_eq!(failure[0].category, zil_base::ResultCategory::ResourceLimit);
    verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    println!("080 Verify complete public request budget N={high}; N-1=ResourceLimit");
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    verify_module_bytes(&bytes, &target, &Limits::default()).unwrap();
    let before = infer_effects(
        &module,
        &graphs(&module),
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    let edited = fixture(&[vec![(1, false)], vec![(2, true)], vec![]], &[0, 0, 2]);
    let after = infer_effects(
        &edited,
        &graphs(&edited),
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    assert_ne!(before, after);
    compare(&edited);
}

#[test]
fn transitive_owned_write_is_rejected_after_convergence_and_borrowed_write_is_legal() {
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        for borrowed in [false, true] {
            let mut module = fixture(&[vec![(1, false)], vec![]], &[0, 1]);
            let entry = &mut module.functions[0];
            entry.environment = Some(ClosureId(0));
            entry.parameters.clear();
            entry.locals.clear();
            for instruction in entry.blocks[0].instructions.iter_mut().take(2) {
                instruction.kind = if borrowed {
                    InstructionKind::ReadCapture { index: 0 }
                } else {
                    InstructionKind::DescribeCapture { index: 0 }
                };
            }
            module.closures.push(ClosureDefinition {
                lending_captures: Vec::new(),
                id: ClosureId(0),
                function: FunctionId(0),
                kind,
                captures: vec![if borrowed {
                    desc()
                } else {
                    Type::Managed(Box::new(payload()))
                }],
                parameters: vec![],
                result: Type::I32,
            });
            compare(&module);
            let gs = graphs(&module);
            let effects =
                infer_effects(&module, &gs, &mut Budget::new(&Limits::default())).unwrap();
            let result = run_function(
                &module,
                &module.functions[0],
                &gs[0],
                &effects,
                true,
                &mut Budget::new(&Limits::default()),
            );
            assert_eq!(result.is_ok(), borrowed || kind == CallableKind::FnMut);
            if let Err(error) = result {
                assert!(error.message.contains("shared Fn"));
            }
        }
    }
}

#[test]
fn public_helper_contract_cuts_the_private_effect_dependency() {
    let mut module = fixture(&[vec![(1, false)], vec![]], &[0, 1]);
    module.functions[1].callable_contract = Some(CallableParameterContract {
        parameter: 0,
        closure: ClosureId(0),
        kind: CallableKind::Fn,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: CallableCleanup::Borrowed,
        traces: vec![CallableTrace {
            calls: vec![],
            exit: CallableExit::Return,
        }],
    });
    let rows = effect_dependents(
        &module,
        &graphs(&module),
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    assert!(rows[1].is_empty());
}

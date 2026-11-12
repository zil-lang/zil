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
            dyn_instance: None,
            retained_return_sources: None,
            storage_life: None,
            call_kind: zil_ir::CallKind::Direct,
            reference_return_bound: None,
            id: FunctionId(id as u32),
            name: format!("f{id}"),
            foreign: None,
            environment: None,
            callable_contract: None,
            protocol_contract: None,
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
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
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
            role_data_uses: Vec::new(),
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
        storage_life: None,
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
                storage_life: None,
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
        roles: vec![CallableRole {
            life: None,
            parameter: 0,
            closure: ClosureId(0),
            kind: CallableKind::Fn,
            hold: vec![],
            call: vec![],
            writes: vec![],
            cleanup: CallableCleanup::Borrowed,
        }],
        traces: zil_ir::CallableTraces::Finite(vec![CallableTrace {
            calls: vec![],
            exit: CallableExit::Return,
        }]),
    });
    let rows = effect_dependents(
        &module,
        &graphs(&module),
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    assert!(rows[1].is_empty());
}

fn cfg_fixture(cycle: bool) -> (TypedModule, FunctionGraph) {
    let mut module = fixture(&[vec![]], &[0]);
    let f = &mut module.functions[0];
    f.locals.push(LocalDecl {
        id: LocalId(2),
        name: "joined".into(),
        ty: desc(),
        mutable: true,
        span: span(),
    });
    f.values.extend([desc(), Type::Bool]);
    let mut entry = f.blocks[0].instructions[..3].to_vec();
    entry.push(instruction(
        Some(4),
        InstructionKind::Constant(Constant::Bool(true)),
    ));
    let store = |value| {
        instruction(
            None,
            InstructionKind::Store {
                local: LocalId(2),
                value: ValueId(value),
            },
        )
    };
    let join = vec![
        instruction(Some(3), InstructionKind::Load(LocalId(2))),
        instruction(
            None,
            InstructionKind::WriteDesc {
                desc: ValueId(3),
                index: 0,
                value: ValueId(2),
            },
        ),
    ];
    let edges = if cycle {
        vec![
            vec![1, 2],
            vec![3],
            vec![5],
            vec![4, 6],
            vec![3],
            vec![3],
            vec![],
        ]
    } else {
        vec![
            vec![1, 2],
            vec![3],
            vec![4],
            vec![],
            vec![5],
            vec![3],
            vec![],
        ]
    };
    f.blocks = edges
        .iter()
        .enumerate()
        .map(|(id, successors)| BasicBlock {
            id: BlockId(id as u32),
            instructions: match id {
                0 => entry.clone(),
                1 => vec![store(0)],
                3 => join.clone(),
                4 if cycle => vec![store(1)],
                5 => vec![store(1)],
                _ => vec![],
            },
            terminator: match successors.as_slice() {
                [] => Terminator::Return(Some(ValueId(2))),
                [next] => Terminator::Jump(BlockId(*next as u32)),
                [left, right] => Terminator::Branch {
                    condition: ValueId(4),
                    then_block: BlockId(*left as u32),
                    else_block: BlockId(*right as u32),
                },
                _ => unreachable!(),
            },
        })
        .collect();
    let mut predecessors = vec![vec![]; edges.len()];
    for (block, successors) in edges.iter().enumerate() {
        for &next in successors {
            predecessors[next].push(block);
        }
    }
    let graph = FunctionGraph {
        successors: edges,
        predecessors,
        reachable: vec![true, true, true, true, true, true, cycle],
        role_data_uses: Vec::new(),
        package_views: packages::CaptureViews::default(),
    };
    (module, graph)
}

#[test]
fn pod_cfg_priority_preserves_complete_resource_and_sum_fixed_points() {
    let limits = Limits {
        max_analysis_steps: 100_000_000,
        ..Limits::default()
    };
    for cycle in [false, true] {
        let (module, graph) = cfg_fixture(cycle);
        let solve = |profile, module: &TypedModule| {
            TEST_RESOURCE_INPUTS.with(|capture| *capture.borrow_mut() = Some(Vec::new()));
            let mut budget = Budget::new(&limits);
            budget.pod_stores = profile == 1;
            budget.store_option_results = profile == 2;
            budget.ordinary_error_places = profile == 2;
            let found = run_function(
                module,
                &module.functions[0],
                &graph,
                &[BTreeSet::new()],
                true,
                &mut budget,
            );
            let inputs = TEST_RESOURCE_INPUTS.with(|capture| capture.borrow_mut().take().unwrap());
            (found, inputs)
        };
        let reference = solve(0, &module);
        let priority = solve(1, &module);
        assert_eq!(
            reference,
            solve(2, &module),
            "ordinary owner/error states differ"
        );
        assert_eq!(
            reference, priority,
            "complete states/writes/diagnostics differ: cycle={cycle}"
        );
        assert_eq!(
            reference.0.unwrap(),
            BTreeSet::from([Root::Parameter(0), Root::Parameter(1)])
        );
        sums::test_schedule_equivalence(&module, &module.functions[0], &graph, &limits);
        // A nearest invalid resource protocol must keep its final rejection,
        // including after a backedge joins additional real source roots.
        let mut bad = module;
        bad.functions[0].locals.push(LocalDecl {
            id: LocalId(3),
            name: "never_initialized".into(),
            ty: Type::Managed(Box::new(payload())),
            mutable: true,
            span: span(),
        });
        bad.functions[0].parameters.push(LocalId(3));
        let reference = solve(0, &bad);
        let priority = solve(1, &bad);
        assert_eq!(
            reference,
            solve(2, &bad),
            "ordinary owner/error rejection differs"
        );
        assert!(reference.0.is_err());
        assert_eq!(reference, priority);
    }
}

#[test]
fn pod_cfg_priority_retains_copy_sum_guards_and_invalidation() {
    let limits = Limits {
        max_analysis_steps: 100_000_000,
        ..Limits::default()
    };
    for case in 0..6 {
        let choice = if case < 3 {
            Type::Enum {
                name: "Choice932".into(),
                variants: vec![("A".into(), vec![Type::I32]), ("B".into(), vec![Type::I32])],
            }
        } else {
            Type::ErrorUnion {
                error: Box::new(Type::Unit),
                result: Box::new(Type::I32),
            }
        };
        let (root, tag_path) = match case % 3 {
            0 => (choice, vec![]),
            1 => (Type::Tuple(vec![choice]), vec![PlaceProjection::Field(0)]),
            _ => (
                Type::Package {
                    name: "TaggedProduct932".into(),
                    fields: vec![
                        ("owner".into(), Type::Managed(Box::new(payload()))),
                        ("choice".into(), choice),
                    ],
                },
                vec![PlaceProjection::Field(1)],
            ),
        };
        let mut payload_path = tag_path.clone();
        payload_path.push(if case < 3 {
            PlaceProjection::Variant(VariantProjection {
                variant: 0,
                index: 0,
            })
        } else {
            PlaceProjection::ErrorArm(false)
        });
        for invalidation in [false, true] {
            let mut module = fixture(&[vec![]], &[0]);
            let f = &mut module.functions[0];
            f.parameters = vec![LocalId(0)];
            f.locals = vec![
                root.clone(),
                Type::I32,
                payload(),
                Type::Array {
                    element: Box::new(Type::I64),
                    length: 16,
                },
                Type::Managed(Box::new(payload())),
            ]
            .into_iter()
            .enumerate()
            .map(|(id, ty)| LocalDecl {
                id: LocalId(id as u32),
                name: format!("l{id}"),
                ty,
                mutable: true,
                span: span(),
            })
            .collect();
            f.values = vec![
                Type::I32,
                Type::I32,
                Type::Bool,
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
            ];
            let invalidate = || {
                instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: ValueId(1),
                    },
                )
            };
            let mut guarded = vec![];
            if invalidation {
                guarded.push(invalidate());
            }
            guarded.push(instruction(
                Some(3),
                InstructionKind::BorrowPlaceLocal {
                    local: LocalId(0),
                    path: payload_path.clone(),
                    mutable: false,
                },
            ));
            f.blocks = vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![
                        instruction(Some(1), InstructionKind::Constant(Constant::I32(0))),
                        // Exercise the indexed invalidation before a fresh guard.
                        // Its changed route/tag Content then reaches successor inputs.
                        invalidate(),
                        instruction(
                            Some(0),
                            InstructionKind::PlaceTagLocal {
                                local: LocalId(0),
                                path: tag_path.clone(),
                            },
                        ),
                        instruction(
                            Some(2),
                            InstructionKind::Binary {
                                op: BinaryOp::Equal,
                                left: ValueId(0),
                                right: ValueId(1),
                            },
                        ),
                    ],
                    terminator: Terminator::Branch {
                        condition: ValueId(2),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: guarded,
                    terminator: Terminator::Return(Some(ValueId(1))),
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![],
                    terminator: Terminator::Return(Some(ValueId(1))),
                },
            ];
            let graph = FunctionGraph {
                successors: vec![vec![1, 2], vec![], vec![]],
                predecessors: vec![vec![], vec![0], vec![0]],
                reachable: vec![true; 3],
                role_data_uses: Vec::new(),
                package_views: packages::CaptureViews::default(),
            };
            assert_eq!(
                sums::test_schedule_equivalence(&module, &module.functions[0], &graph, &limits),
                !invalidation,
                "case={case}"
            );
            if case == 0 && !invalidation {
                sums::test_tagged_invalidation_budget(&module, &module.functions[0], &graph);
            }
        }
    }
}

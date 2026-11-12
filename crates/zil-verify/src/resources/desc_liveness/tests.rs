//! Independent complete bool history and consumer, not a word-to-word mirror.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
include!("reference.rs");
fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn instruction(result: Option<usize>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(|n| ValueId(n as u32)),
        kind,
        span: span(),
    }
}
fn fixture(
    count: usize,
    edges: Vec<Vec<usize>>,
    reachable: Vec<bool>,
) -> (TypedModule, FunctionGraph, Slots) {
    let local_count = count.div_ceil(2);
    let value_count = count - local_count;
    let ty = Type::Desc(Box::new(Type::Record {
        name: "HistoryToken".into(),
        fields: vec![("n".into(), Type::I32)],
    }));
    let mut blocks = Vec::new();
    for id in 0..edges.len() {
        let mut instructions = vec![instruction(
            None,
            InstructionKind::ScopeEnter { scope: ScopeId(0) },
        )];
        if local_count > 0 {
            let local = LocalId((id % local_count) as u32);
            instructions.push(instruction(None, InstructionKind::StorageLive(local)));
            instructions.push(instruction(
                (value_count > 0).then_some(id % value_count.max(1)),
                InstructionKind::Load(local),
            ));
            if value_count > 0 {
                instructions.push(instruction(
                    None,
                    InstructionKind::Store {
                        local,
                        value: ValueId((id % value_count) as u32),
                    },
                ));
                instructions.push(instruction(
                    None,
                    InstructionKind::ReadDesc {
                        desc: ValueId(((id + value_count - 1) % value_count) as u32),
                        index: 0,
                    },
                ));
            }
            instructions.push(instruction(None, InstructionKind::BorrowLocal { local }));
            instructions.push(instruction(None, InstructionKind::BorrowLocalMut { local }));
            instructions.push(instruction(None, InstructionKind::TakeLocal { local }));
            instructions.push(instruction(None, InstructionKind::StorageDead(local)));
        }
        // An actually empty block and an unreachable block still get histories.
        if id == 4 {
            instructions.clear();
        }
        blocks.push(BasicBlock {
            id: BlockId(id as u32),
            instructions,
            terminator: Terminator::Trap,
        });
    }
    let function = TypedFunction {
        dyn_instance: None,
        retained_return_sources: None,
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        reference_return_bound: None,
        id: FunctionId(0),
        name: "history".into(),
        foreign: None,
        environment: None,
        callable_contract: None,
        protocol_contract: None,
        callable_return_contract: None,
        parameters: (0..local_count).map(|n| LocalId(n as u32)).collect(),
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: (0..local_count)
            .map(|n| LocalDecl {
                id: LocalId(n as u32),
                name: format!("d{n}"),
                ty: ty.clone(),
                mutable: true,
                span: span(),
            })
            .collect(),
        values: vec![ty; value_count],
        blocks,
        entry: BlockId(0),
        span: span(),
    };
    let module = TypedModule {
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![function],
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
    };
    let mut predecessors = vec![vec![]; edges.len()];
    for (from, row) in edges.iter().enumerate() {
        for &to in row {
            predecessors[to].push(from);
        }
    }
    let graph = FunctionGraph {
        successors: edges,
        predecessors,
        reachable,
        package_views: packages::CaptureViews::default(),
        role_data_uses: vec![],
    };
    let slots = Slots::new(
        &module,
        &module.functions[0],
        &graph.package_views,
        &mut budget(1_000_000),
    )
    .unwrap();
    assert_eq!(slots.desc_count, count);
    (module, graph, slots)
}
fn dense(history: &History, function: &TypedFunction) -> Vec<Vec<Vec<bool>>> {
    function
        .blocks
        .iter()
        .enumerate()
        .map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let row = history.row(b, i);
                    (0..row.len())
                        .map(|n| row.get(n, &mut budget(1_000_000)).unwrap())
                        .collect()
                })
                .collect()
        })
        .collect()
}
#[test]
fn description_history_all_rows_match_complete_bool_solver() {
    for count in [0, 1, 7, 8, 9, WORD - 1, WORD, WORD + 1, 72, 2 * WORD + 1] {
        let (module, graph, slots) = fixture(
            count,
            vec![vec![1, 3], vec![2], vec![1, 3], vec![4], vec![], vec![5]],
            vec![true, true, true, true, true, false],
        );
        let f = &module.functions[0];
        let reference = old_liveness(f, &graph, &slots, &mut budget(10_000_000)).unwrap();
        let actual = liveness(f, &graph, &slots, &mut budget(10_000_000)).unwrap();
        assert_eq!(
            dense(&actual, f),
            reference,
            "count={count}: complete reachable/unreachable after rows"
        );
        assert!(matches!(actual, History::Dense(_)) == (count <= SMALL));
    }
}
#[test]
fn description_history_word_reverse_matches_every_old_operation_and_tail() {
    let (module, _, slots) = fixture(2 * WORD + 1, vec![vec![]], vec![true]);
    let f = &module.functions[0];
    let last = slots.desc_count - 1;
    let mut instructions = f.blocks[0].instructions.clone();
    // These private reverse unit probes share the old explicit local-coordinate
    // rule; they do not claim callable type admission for a Desc-typed local.
    instructions.push(instruction(
        None,
        InstructionKind::CallClosureShared {
            callee: LocalId(0),
            arguments: vec![],
        },
    ));
    instructions.push(instruction(
        None,
        InstructionKind::CallClosureMut {
            callee: LocalId(0),
            arguments: vec![],
        },
    ));
    for active in [false, true] {
        let mut old = vec![active; slots.desc_count];
        let mut words = vec![0; slots.desc_count.div_ceil(WORD)];
        for (index, value) in old.iter().enumerate() {
            set(&mut words, old.len(), index, *value, &mut budget(100)).unwrap();
        }
        for i in instructions.iter().cycle().take(instructions.len() * 3) {
            old_reverse(&slots, &mut old, i);
            reverse_words(&slots, &mut words, i, &mut budget(1_000_000)).unwrap();
            let row = Row::Packed {
                len: old.len(),
                words: &words,
            };
            for (index, expected) in old.iter().enumerate() {
                assert_eq!(row.get(index, &mut budget(100)).unwrap(), *expected);
            }
            assert_eq!(words.last().unwrap() & !1usize, 0, "unused tail bits");
        }
        let before = words.clone();
        assert!(set(&mut words, old.len(), last + 1, true, &mut budget(100)).is_err());
        assert_eq!(words, before);
    }
}
#[test]
fn description_history_consumer_preserves_full_roots_diagnostics_and_state() {
    for count in [1, 8, 9, 72, WORD + 1] {
        let (module, _, slots) = fixture(count, vec![vec![]], vec![true]);
        let f = &module.functions[0];
        for active in [false, true] {
            for observed in [
                Root::Local(LocalId(0)),
                Root::Local(LocalId(1)),
                Root::PackageField {
                    local: LocalId(0),
                    index: 2,
                },
                Root::Parameter(3),
            ] {
                let mut state = slots.empty(&mut budget(1_000_000)).unwrap();
                state.desc[count - 1].insert(observed);
                state.newer.iter_mut().for_each(|r| {
                    r.insert(0);
                });
                let before = state.clone();
                let bools = vec![active; count];
                let actual = history(
                    f,
                    &slots,
                    std::slice::from_ref(&bools),
                    &mut budget(1_000_000),
                )
                .unwrap();
                // Use a row generated from a no-op instruction, whose after
                // snapshot is exactly this original complete exit inventory.
                let row = actual.row(0, f.blocks[0].instructions.len() - 1);
                let old = old_no_live_description(
                    &state,
                    &bools,
                    Root::Local(LocalId(0)),
                    span(),
                    &mut budget(1_000_000),
                );
                let new = no_live_description(
                    &state,
                    row,
                    Root::Local(LocalId(0)),
                    span(),
                    &mut budget(1_000_000),
                );
                assert_eq!(old, new);
                if active
                    && matches!(
                        observed,
                        Root::Local(LocalId(0))
                            | Root::PackageField {
                                local: LocalId(0),
                                ..
                            }
                    )
                {
                    let d = new.unwrap_err();
                    assert_eq!(d.category, zil_base::ResultCategory::SafeReject);
                    assert!(d.message.contains("invalidate a live description"));
                } else {
                    new.unwrap();
                }
                assert_eq!(state, before, "all proof lanes are read-only");
            }
        }
    }
}
#[test]
fn description_history_budget_failure_does_not_publish_and_retries_fresh() {
    let (module, graph, slots) = fixture(72, vec![vec![1], vec![2], vec![1]], vec![true; 3]);
    let f = &module.functions[0];
    let mut b = budget(1_000_000);
    let success = liveness(f, &graph, &slots, &mut b).unwrap();
    let n = 1_000_000 - b.remaining;
    let failure = liveness(f, &graph, &slots, &mut budget(n - 1))
        .err()
        .unwrap();
    assert_eq!(failure.category, zil_base::ResultCategory::ResourceLimit);
    let exact = liveness(f, &graph, &slots, &mut budget(n)).unwrap();
    let fresh = liveness(f, &graph, &slots, &mut budget(n)).unwrap();
    assert_eq!(dense(&success, f), dense(&exact, f));
    assert_eq!(dense(&fresh, f), dense(&exact, f));
    eprintln!("description history complete CFG N={n} N-1=ResourceLimit freshN=Pass");
    // Actual bit mutation prepays before touching scratch; fail at its last fee.
    let mut row = vec![0; 2];
    let before = row.clone();
    assert_eq!(
        set(&mut row, 72, 71, true, &mut budget(5))
            .unwrap_err()
            .category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(row, before);
    set(&mut row, 72, 71, true, &mut budget(6)).unwrap();
    assert!(
        Row::Packed {
            len: 72,
            words: &row
        }
        .get(71, &mut budget(4))
        .unwrap()
    );
}
#[test]
fn description_history_snapshot_work_reduces_large_rows_and_keeps_tiny_exact() {
    for count in [0, 1, 7, 8, 9, 72, 129] {
        let (mut module, graph, slots) = fixture(count, vec![vec![]], vec![true]);
        for _ in 0..256 {
            module.functions[0].blocks[0].instructions.push(instruction(
                None,
                InstructionKind::ScopeEnter { scope: ScopeId(0) },
            ));
        }
        let f = &module.functions[0];
        let mut current = budget(10_000_000);
        let actual = liveness(f, &graph, &slots, &mut current).unwrap();
        let mut old = budget(10_000_000);
        let expected = old_liveness(f, &graph, &slots, &mut old).unwrap();
        assert_eq!(dense(&actual, f), expected);
        let new_n = 10_000_000 - current.remaining;
        let old_n = 10_000_000 - old.remaining;
        if count <= SMALL {
            assert_eq!(new_n, old_n, "tiny actual bool path");
        } else {
            assert!(new_n < old_n, "count={count}: {new_n} >= {old_n}");
        }
        eprintln!("description history count={count} complete N={new_n} old={old_n}");
    }
}

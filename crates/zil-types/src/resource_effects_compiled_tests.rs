use super::super::worklist_tests::{chain, enough, graph, inputs, minimum, table};
use super::*;

#[test]
fn every_three_vertex_graph_and_seed_matches_two_existing_solvers() {
    let mut compared = 0;
    for mask in 0..512 {
        let edges = (0..3)
            .map(|i| (0..3).filter(|j| mask & (1 << (3 * i + j)) != 0).collect())
            .collect::<Vec<Vec<_>>>();
        for seeds in 0..8 {
            let core = graph(&edges, seeds);
            let signatures = table(&core);
            let list = inputs(&core, &signatures);
            let (actual, stats) = solve(&list, None, &enough()).unwrap();
            assert_eq!(actual, solve_sweeps(&list, None, &enough()).unwrap());
            assert_eq!(actual, solve_worklist(&list, None, &enough()).unwrap());
            assert_eq!(stats.body_evaluations, 3);
            assert_eq!(stats.facts, 6);
            assert!(stats.traversed_edges <= stats.edges);
            assert!(stats.activated <= stats.facts);
            compared += 1;
        }
    }
    assert_eq!(compared, 4096);
}

#[test]
fn budget_exhaustion_never_returns_partial_summaries_and_retry_is_pure() {
    let core = chain(16, true);
    let signatures = table(&core);
    let list = inputs(&core, &signatures);
    let (expected, stats) = solve(&list, None, &enough()).unwrap();
    for n in 0..stats.charged_work {
        let limits = Limits {
            max_analysis_steps: n,
            ..enough()
        };
        let err = solve(&list, None, &limits).unwrap_err();
        assert_eq!(err.category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(err.phase, Phase::Types);
    }
    let limits = Limits {
        max_analysis_steps: stats.charged_work,
        ..enough()
    };
    assert_eq!(solve(&list, None, &limits).unwrap().0, expected);
    assert_eq!(solve(&list, None, &enough()).unwrap().0, expected);
}

#[test]
fn measure_actual_transfer_work_growth_not_a_wall_clock_claim() {
    for n in [16, 32, 64, 128] {
        for reverse in [false, true] {
            let core = chain(n, reverse);
            let signatures = table(&core);
            let list = inputs(&core, &signatures);
            let (compiled, stats) = solve(&list, None, &enough()).unwrap();
            let worklist = minimum(|steps| {
                solve_worklist(
                    &list,
                    None,
                    &Limits {
                        max_analysis_steps: steps,
                        ..enough()
                    },
                )
            });
            let sweeps = minimum(|steps| {
                solve_sweeps(
                    &list,
                    None,
                    &Limits {
                        max_analysis_steps: steps,
                        ..enough()
                    },
                )
            });
            assert_eq!(compiled, solve_sweeps(&list, None, &enough()).unwrap());
            assert_eq!(stats.body_evaluations, n);
            assert_eq!(stats.facts, 2 * n);
            println!(
                "IMPL136 n={n} reverse={reverse} facts={} edges={} compiled={} worklist={worklist} sweeps={sweeps}",
                stats.facts, stats.edges, stats.charged_work
            );
        }
    }
}

#[test]
fn sparse_graph_with_no_effects_does_not_activate_recursive_components() {
    let edges = (0..32).map(|i| vec![(i + 1) % 32]).collect::<Vec<_>>();
    let core = graph(&edges, 0);
    let signatures = table(&core);
    let (writes, stats) = solve(&inputs(&core, &signatures), None, &enough()).unwrap();
    assert!(writes.values().all(BTreeSet::is_empty));
    assert_eq!(stats.activated, 0);
    assert_eq!(stats.traversed_edges, 0);
    assert_eq!(
        solve(
            &[],
            None,
            &Limits {
                max_analysis_steps: 0,
                ..enough()
            }
        )
        .unwrap(),
        (Writes::new(), Stats::default())
    );
}

#[test]
fn wide_recursive_parameter_rotation_removes_interprocedural_rescans() {
    use zil_ir::{CoreExprKind, CoreStatementKind};
    for width in [2, 4, 8, 16] {
        let n = 16;
        let mut core = graph(
            &(0..n).map(|i| vec![(i + 1) % n]).collect::<Vec<_>>(),
            usize::MAX,
        );
        let names = (0..width)
            .map(|i| {
                if i == 0 {
                    "a".to_owned()
                } else if i == 1 {
                    "b".to_owned()
                } else {
                    format!("p{i}")
                }
            })
            .collect::<Vec<_>>();
        for f in &mut core.functions {
            let parameter = f.parameters[0].clone();
            f.parameters = names
                .iter()
                .map(|name| {
                    let mut p = parameter.clone();
                    p.name = name.clone();
                    p
                })
                .collect();
            if let CoreStatementKind::Expression(e) = &mut f.body.statements[0].kind
                && let CoreExprKind::Call { arguments, .. } = &mut e.kind
            {
                let arg = arguments[0].clone();
                *arguments = (0..width)
                    .map(|i| {
                        let mut e = arg.clone();
                        e.kind = CoreExprKind::Name(names[(i + 1) % width].clone());
                        e
                    })
                    .collect();
            }
        }
        let signatures = table(&core);
        let list = inputs(&core, &signatures);
        let (actual, stats) = solve(&list, None, &enough()).unwrap();
        assert_eq!(actual, solve_worklist(&list, None, &enough()).unwrap());
        assert!(actual.values().all(|row| row.len() == width));
        let worklist = minimum(|steps| {
            solve_worklist(
                &list,
                None,
                &Limits {
                    max_analysis_steps: steps,
                    ..enough()
                },
            )
        });
        println!(
            "IMPL136 wide_cycle n={n} width={width} facts={} edges={} compiled={} worklist={worklist}",
            stats.facts, stats.edges, stats.charged_work
        );
        if width >= 4 {
            assert!(stats.charged_work < worklist);
        }
    }
}

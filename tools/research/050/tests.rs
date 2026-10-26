use super::*;
use fixtures::{build, random_model, table};
use model::{Model, Relation, decode};

fn candidate(m: &Model) -> (Answer, Cache) {
    m.validate(Limits::default()).unwrap();
    let cache = Cache::execute(
        m,
        Plan::build(m).unwrap(),
        Limits::default(),
        &mut Stats::default(),
    )
    .unwrap();
    let answer = cache.answer().unwrap();
    check_answer(m, &answer).unwrap();
    (answer, cache)
}

fn agree(m: &Model) -> Answer {
    let (answer, _) = candidate(m);
    let (reference, _) = oracle::enumerate(m, Limits::default()).unwrap();
    let baseline = solve::flat(m, Limits::default(), &mut Stats::default()).unwrap();
    check_answer(m, &baseline).unwrap();
    assert_eq!(answer.relation, reference, "{} candidate", m.name);
    assert_eq!(baseline.relation, reference, "{} baseline", m.name);
    answer
}

fn fixture(line: &str) -> Model {
    build(&Case::parse(line).unwrap()).unwrap()
}

#[test]
fn all_frozen_shapes_equal_independent_oracle() {
    for line in include_str!("../../../tests/perf/impl050/cases.txt")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        agree(&fixture(line));
    }
}

#[test]
fn two_thousand_random_relations_preserve_complete_observations() {
    for seed in 0..2048 {
        agree(&random_model(seed));
    }
}

#[test]
fn nonmonotone_random_edits_match_full_enumeration() {
    for seed in 0..256 {
        let mut m = random_model(seed);
        let (_, mut cache) = candidate(&m);
        if m.factors[0].rows.first() == Some(&0) {
            m.factors[0].rows.remove(0);
        } else {
            m.factors[0].rows.insert(0, 0);
        }
        cache.update(&m, Limits::default()).unwrap();
        let actual = cache.answer().unwrap();
        check_answer(&m, &actual).unwrap();
        assert_eq!(
            actual.relation,
            oracle::enumerate(&m, Limits::default()).unwrap().0
        );
    }
}

fn values(m: &Model, answer: &Answer) -> Vec<Vec<u8>> {
    answer
        .relation
        .rows
        .iter()
        .map(|&row| {
            let mut a = vec![0; m.domains.len()];
            decode(row, &m.observed, &m.domains, &mut a);
            a
        })
        .collect()
}

#[test]
fn equal_capture_marginals_do_not_preserve_alias_correlation() {
    let safe = fixture("safe closure 0 0 1");
    let unsafe_m = fixture("alias closure 1 0 1");
    let a = values(&safe, &agree(&safe));
    let b = values(&unsafe_m, &agree(&unsafe_m));
    for capture in [0, 1] {
        let marginal = |rows: &[Vec<u8>]| {
            rows.iter()
                .map(|r| r[capture])
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(marginal(&a), marginal(&b));
    }
    assert!(
        a.iter()
            .all(|r| r[6] == 1 && r[9] == 1 && r[14] == 0 && r[10] + r[11] == 0)
    );
    assert!(
        b.iter()
            .all(|r| r[6] == 1 && r[9] == 0 && r[14] == 2 && r[10] + r[11] == 1)
    );
    println!(
        "counterexample: equal capture marginals; distinct safe rows={}, alias error rows={}",
        a.len(),
        b.len()
    );
}

#[test]
fn existence_of_normal_path_does_not_mean_safety() {
    let m = fixture("mixed closure 2 0 1");
    let a = values(&m, &agree(&m));
    assert!(a.iter().any(|r| r[14] == 0));
    assert!(a.iter().any(|r| r[14] == 2));
    assert_eq!(a.len(), 4);
}

#[test]
fn owned_creation_calls_and_cleanup_keep_one_responsibility() {
    for (first, second, error, final_drop) in [
        (0, 0, false, 1),
        (1, 1, false, 1),
        (2, 0, false, 0),
        (2, 2, true, 0),
        (2, 1, true, 0),
    ] {
        let m = fixture(&format!("owned owned {first} {second} 1"));
        let a = values(&m, &agree(&m));
        assert_eq!(a.len(), 1);
        assert_eq!(a[0][4] + a[0][7] + a[0][9], 1);
        assert_eq!(a[0][8], 2);
        assert_eq!(a[0][9], final_drop);
        assert_eq!(a[0][10] == 2, error);
    }
}

#[test]
fn edited_preserve_to_consume_replaces_old_summary_and_reuses_sibling() {
    let before = fixture("before owned 1 1 1");
    let after = fixture("after owned 2 1 1");
    let (old, mut cache) = candidate(&before);
    let stats = cache.update(&after, Limits::default()).unwrap();
    let new = cache.answer().unwrap();
    assert_ne!(old.relation, new.relation);
    assert_eq!(new.relation, agree(&after).relation);
    assert!(stats.nodes_reused > 0);
    assert!(stats.nodes_recomputed > 0);
    let old_rows: std::collections::BTreeSet<_> = old.relation.rows.iter().copied().collect();
    assert!(new.relation.rows.iter().all(|r| !old_rows.contains(r)));
    println!(
        "edit Preserve->Consume: recomputed={}, reused={}; old results removed",
        stats.nodes_recomputed, stats.nodes_reused
    );
}

#[test]
fn contextual_change_invalidates_every_node() {
    let mut m = fixture("ctx chain 8 3 1");
    let (_, mut cache) = candidate(&m);
    m.context = [1, 2, 3, 4];
    let stats = cache.update(&m, Limits::default()).unwrap();
    assert_eq!(stats.nodes_reused, 0);
    assert_eq!(
        stats.nodes_recomputed,
        m.domains.len() - m.observed.len() + 1
    );
    assert_eq!(cache.answer().unwrap().relation, agree(&m).relation);
}

#[test]
fn trait_independent_existentials_lose_common_substitution() {
    let m = fixture("shared trait 0 0 1");
    for factor in &m.factors {
        let mut separate = m.clone();
        separate.factors = vec![factor.clone()];
        assert!(!agree(&separate).relation.rows.is_empty());
    }
    assert!(agree(&m).relation.rows.is_empty());
    println!("counterexample: each trait goal is satisfiable, conjunction on shared T is not");
}

#[test]
fn specialization_candidate_and_context_change_invalidates_response() {
    let domains = vec![2, 2];
    let mut m = Model {
        name: "specialization".into(),
        observed: vec![0, 1],
        factors: vec![table(&domains, vec![0, 1], |v| v[1] == 0)],
        domains,
        context: [0; 4],
    };
    let (old, mut cache) = candidate(&m);
    m.factors[0] = table(&m.domains, vec![0, 1], |v| v[1] == v[0]);
    m.context[1] += 1;
    cache.update(&m, Limits::default()).unwrap();
    let new = cache.answer().unwrap();
    assert_ne!(old.relation, new.relation);
    assert_eq!(new.relation, agree(&m).relation);
}

#[test]
fn descriptor_cannot_follow_replaced_static_identity() {
    let domains = vec![2; 3];
    let mut m = Model {
        name: "replace_identity".into(),
        observed: vec![0, 1, 2],
        factors: vec![
            table(&domains, vec![0], |v| v[0] == 0),
            table(&domains, vec![1], |v| v[1] == 0),
            table(&domains, vec![0, 1, 2], |v| v[2] == u8::from(v[0] == v[1])),
        ],
        domains,
        context: [0; 4],
    };
    let (_, mut cache) = candidate(&m);
    m.factors[1] = table(&m.domains, vec![1], |v| v[1] == 1);
    cache.update(&m, Limits::default()).unwrap();
    let new = cache.answer().unwrap();
    assert_eq!(new.relation, agree(&m).relation);
    assert_eq!(values(&m, &new)[0][2], 0);
}

#[test]
fn structural_edit_rebuilds_plan() {
    let mut m = fixture("structure chain 5 2 1");
    let (_, mut cache) = candidate(&m);
    m.observed = vec![1, 3];
    let stats = cache.update(&m, Limits::default()).unwrap();
    assert_eq!(stats.nodes_reused, 0);
    assert_eq!(cache.answer().unwrap().relation, agree(&m).relation);
}

#[test]
fn limits_are_not_empty_relations_and_oracle_can_be_explicit_fallback() {
    let m = fixture("bounded chain 12 3 1");
    let limits = Limits {
        rows: 200,
        ..Limits::default()
    };
    m.validate(limits).unwrap();
    assert!(matches!(
        solve::flat(&m, limits, &mut Stats::default()),
        Err(Error::Limit(_))
    ));
    let fast = Cache::execute(&m, Plan::build(&m).unwrap(), limits, &mut Stats::default()).unwrap();
    assert_eq!(fast.answer().unwrap().relation, agree(&m).relation);
    let domains = vec![2; 8];
    let mut dense = Model {
        name: "too_wide".into(),
        observed: vec![0, 7],
        factors: Vec::new(),
        domains,
        context: [0; 4],
    };
    for a in 0..8 {
        for b in a + 1..8 {
            dense
                .factors
                .push(table(&dense.domains, vec![a, b], |_| true));
        }
    }
    let limits = Limits {
        scope: 3,
        ..Limits::default()
    };
    dense.validate(limits).unwrap();
    assert!(matches!(
        Cache::execute(
            &dense,
            Plan::build(&dense).unwrap(),
            limits,
            &mut Stats::default()
        ),
        Err(Error::Limit(_))
    ));
    assert_eq!(
        oracle::enumerate(&dense, limits).unwrap().0,
        agree(&dense).relation
    );
    assert!(matches!(
        oracle::enumerate(
            &dense,
            Limits {
                oracle_nodes: 2,
                ..limits
            }
        ),
        Err(Error::Limit(_))
    ));
}

#[test]
fn failed_incremental_transaction_keeps_previous_exact_state() {
    let mut m = fixture("atomic chain 8 3 1");
    let (old, mut cache) = candidate(&m);
    m.context[0] = 1;
    assert!(matches!(
        cache.update(
            &m,
            Limits {
                work: 60,
                ..Limits::default()
            }
        ),
        Err(Error::Limit(_))
    ));
    assert_eq!(cache.answer().unwrap().relation, old.relation);
}

#[test]
fn witness_tamper_is_rejected_and_omission_requires_completeness_oracle() {
    let m = fixture("tamper chain 4 2 1");
    let (mut answer, _) = candidate(&m);
    answer.witnesses[0][1] ^= 1;
    assert!(check_answer(&m, &answer).is_err());
    let (mut omitted, _) = candidate(&m);
    omitted.relation.rows.pop();
    omitted.witnesses.pop();
    // The witness checker establishes soundness only. It deliberately does not
    // masquerade as a proof that no bad outcome was omitted.
    check_answer(&m, &omitted).unwrap();
    assert_ne!(
        omitted.relation,
        oracle::enumerate(&m, Limits::default()).unwrap().0
    );
}

#[test]
fn empty_scope_and_unsatisfiable_factor_are_distinct() {
    for rows in [vec![0], vec![]] {
        let m = Model {
            name: "scalar".into(),
            domains: vec![],
            observed: vec![],
            factors: vec![Relation {
                scope: vec![],
                rows: rows.clone(),
            }],
            context: [0; 4],
        };
        assert_eq!(agree(&m).relation.rows, rows);
    }
}

#[test]
fn reused_relations_obey_reduced_row_limit() {
    for restrict_output in [false, true] {
        let mut m = fixture("reused_rows chain 8 3 1");
        if restrict_output {
            // The final answer has only 3 rows, but an internal summary still
            // has 9: checking just the final answer would miss this case.
            m.factors.push(table(&m.domains, vec![0], |v| v[0] == 0));
        }
        let (old, mut cache) = candidate(&m);
        assert_eq!(old.relation.rows.len(), if restrict_output { 3 } else { 9 });
        let narrow = Limits {
            rows: 6,
            ..Limits::default()
        };
        m.validate(narrow).unwrap();
        assert!(matches!(
            Cache::execute(&m, Plan::build(&m).unwrap(), narrow, &mut Stats::default()),
            Err(Error::Limit(_))
        ));
        assert!(matches!(cache.update(&m, narrow), Err(Error::Limit(_))));
        let unchanged = cache.answer().unwrap();
        assert_eq!(unchanged.relation, old.relation);
        check_answer(&m, &unchanged).unwrap();
        let restored = cache.update(&m, Limits::default()).unwrap();
        assert_eq!(restored.nodes_recomputed, 0);
        assert!(restored.nodes_reused > 0);
        assert!(restored.max_rows >= 9);
        assert_eq!(cache.answer().unwrap().relation, old.relation);
    }
}

#[test]
fn reused_relations_obey_reduced_scope_limit() {
    for (line, scope) in [
        ("reused_output owned 0 0 1", 6),
        ("reused_internal grid 2 6 1", 2),
    ] {
        // Owned fails on the final observation scope; grid has a wider internal
        // summary although both input factors and the final answer have scope 2.
        let m = fixture(line);
        let (old, mut cache) = candidate(&m);
        let narrow = Limits {
            scope,
            ..Limits::default()
        };
        m.validate(narrow).unwrap();
        assert!(matches!(
            Cache::execute(&m, Plan::build(&m).unwrap(), narrow, &mut Stats::default()),
            Err(Error::Limit(_))
        ));
        assert!(matches!(cache.update(&m, narrow), Err(Error::Limit(_))));
        let unchanged = cache.answer().unwrap();
        assert_eq!(unchanged.relation, old.relation);
        check_answer(&m, &unchanged).unwrap();
        let restored = cache.update(&m, Limits::default()).unwrap();
        assert_eq!(restored.nodes_recomputed, 0);
        assert!(restored.nodes_reused > 0);
        assert!(restored.max_scope > scope);
        assert_eq!(cache.answer().unwrap().relation, old.relation);
    }
}

#[test]
fn reused_relations_charge_copy_work_before_publication() {
    let m = fixture("reused_work chain 8 3 1");
    let (old, mut cache) = candidate(&m);
    let narrow = Limits {
        work: 42,
        ..Limits::default()
    };
    m.validate(narrow).unwrap();
    assert!(matches!(cache.update(&m, narrow), Err(Error::Limit(_))));
    assert_eq!(cache.answer().unwrap().relation, old.relation);
    let restored = cache.update(&m, Limits::default()).unwrap();
    assert_eq!(restored.nodes_recomputed, 0);
    assert!(restored.nodes_reused > 0);
    assert!(restored.work > narrow.work);
    assert_eq!(cache.answer().unwrap().relation, old.relation);
}

use crate::{
    budget::{self, R, Work},
    encoding::Representation,
    intern, oracle, report, telemetry, typed,
};

#[test]
fn dense_direct_complete_budget_is_exact() {
    let m = typed(
        "dense_direct_work",
        "resource T{n:i32} fn f(){let a=T{n:1};let b=T{n:2}} fn main()->i32{return 0}",
    )
    .unwrap();
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    let reference = oracle::solve(&m, f, budget::LIMIT).unwrap();
    let run = |limit| -> R<usize> {
        let mut s = crate::direct::solve(
            &m,
            f,
            limit,
            &mut telemetry::Telemetry::disabled(),
            "owner",
            true,
            Representation::Dense,
        )?;
        crate::compare(&mut s, &reference)?;
        let bytes = report::materialize(&mut s, &reference)?;
        s.bdd.work.charge(bytes.len())?;
        Ok(s.bdd.work.used)
    };
    let n = run(budget::LIMIT).unwrap();
    assert_eq!(run(n).unwrap(), n);
    assert!(run(n - 1).unwrap_err().starts_with("ResourceLimit:"));
    println!("076 exact dense direct complete budget n={n}");
}

#[test]
fn colliding_rows_remain_distinct_through_every_resize() {
    let mut pool = intern::Pool::default();
    let mut work = Work::new(budget::LIMIT);
    for n in 0..160u64 {
        assert_eq!(
            pool.intern_hashed(&[0, n, 7], 11, &mut work).unwrap(),
            n as usize
        );
    }
    for n in (0..160u64).rev() {
        assert_eq!(
            pool.intern_hashed(&[0, n, 7], 11, &mut work).unwrap(),
            n as usize
        );
        assert_eq!(pool.rows[n as usize], vec![0, n, 7]);
    }
    assert_eq!(pool.rows.len(), 160);
    assert_eq!(pool.stats.intern_hits, 160);
    assert!(pool.stats.rehashes > 1);
    assert!(pool.stats.equality_word_visits >= 160 * 160);
}

#[test]
fn hash_probing_and_row_comparison_obey_the_work_limit() {
    let run = |limit| -> R<usize> {
        let mut work = Work::new(limit);
        let mut pool = intern::Pool::default();
        for n in 0..40u64 {
            pool.intern_hashed(&[9, n], 3, &mut work)?;
        }
        for n in 0..40u64 {
            assert_eq!(pool.intern_hashed(&[9, n], 3, &mut work)?, n as usize);
        }
        Ok(work.used)
    };
    let n = run(budget::LIMIT).unwrap();
    assert_eq!(run(n).unwrap(), n);
    assert!(run(n - 1).unwrap_err().starts_with("ResourceLimit:"));
}

#[test]
fn shared_row_does_not_relax_total_relation_member_limit() {
    let mut indexed = intern::Indexed::default();
    let mut work = Work::new(budget::LIMIT);
    for n in 0..=budget::MAX_ROWS {
        let relation = indexed
            .register(format!("point:{n}"), true, &mut work)
            .unwrap();
        let result = indexed.insert(relation, 0, &mut work);
        if n == budget::MAX_ROWS {
            assert_eq!(
                result.unwrap_err(),
                "ResourceLimit: intern total relation rows"
            );
        } else {
            assert!(result.unwrap());
            assert!(!indexed.insert(relation, 0, &mut work).unwrap());
        }
    }
    assert_eq!(indexed.total, budget::MAX_ROWS);
}

#[test]
fn dense_and_local_intern_complete_budget_is_exact_and_deterministic() {
    let m = typed("intern_complete", "resource T{n:i32} fn consume(give x:T){} fn f(){let a=T{n:1};let b=T{n:2};let cb=move()=>{consume(give a);consume(give b)};cb()} fn main()->i32{return 0}").unwrap();
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    let reference = oracle::solve(&m, f, budget::LIMIT).unwrap();
    for representation in [Representation::Dense, Representation::Local] {
        let run = |limit| -> R<usize> {
            let (mut s, _) = intern::solve(
                &m,
                f,
                limit,
                &mut telemetry::Telemetry::disabled(),
                "owner",
                true,
                representation,
            )?;
            crate::compare(&mut s, &reference)?;
            let bytes = report::materialize(&mut s, &reference)?;
            s.bdd.work.charge(bytes.len())?;
            std::hint::black_box(zil_base::Fingerprint::of(bytes.as_bytes()));
            Ok(s.bdd.work.used)
        };
        let n = run(budget::LIMIT).unwrap();
        assert_eq!(run(n).unwrap(), n);
        assert_eq!(run(budget::LIMIT).unwrap(), n);
        assert!(run(n - 1).unwrap_err().starts_with("ResourceLimit:"));
        println!(
            "076 exact {} intern complete budget n={n}",
            representation.name()
        );
    }
}

#[test]
fn all_points_survive_interning_including_empty_and_shared_states() {
    let m = typed(
        "shared",
        include_str!("../../../tests/perf/impl059/correlated.zil"),
    )
    .unwrap();
    let f = m.functions.iter().find(|f| f.name == "correlated").unwrap();
    let reference = oracle::solve(&m, f, budget::LIMIT).unwrap();
    for representation in [Representation::Dense, Representation::Local] {
        let (mut s, stats) = intern::solve(
            &m,
            f,
            budget::LIMIT,
            &mut telemetry::Telemetry::disabled(),
            "owner",
            true,
            representation,
        )
        .unwrap();
        crate::compare(&mut s, &reference).unwrap();
        let actual = report::materialize(&mut s, &reference).unwrap();
        let expected = report::canonical(
            &reference.schema,
            &reference.labels,
            &reference.relations,
            reference.status,
            &mut Work::new(budget::LIMIT),
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            stats.relation_rows,
            reference
                .relations
                .values()
                .map(|rows| rows.len())
                .sum::<usize>()
        );
        assert!(stats.unique_rows < stats.relation_rows);
        assert!(stats.intern_hits > 0);
        assert_eq!(
            s.explicit
                .as_ref()
                .unwrap()
                .keys()
                .filter(|key| key.starts_with("point:"))
                .count(),
            s.events
        );
    }
}

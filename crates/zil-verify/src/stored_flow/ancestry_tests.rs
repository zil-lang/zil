use super::*;
fn compare(ids: &[usize], loans: &[Loan]) -> Check<Vec<usize>> {
    let expected = ancestry_by_linear_set(ids, loans, &mut Budget::new(&Limits::default()));
    let actual = ancestry(ids, loans, &mut Budget::new(&Limits::default()));
    assert_eq!(
        actual, expected,
        "complete sorted closure and original invalid-parent diagnostic"
    );
    actual
}
#[test]
fn visited_words_preserve_boundaries_sparse_arenas_cycles_and_root_order() {
    for width in [0, 1, 63, 64, 65, 127, 128, 129, 511, 512, 513, 4097] {
        let mut loans = vec![Loan::default(); width];
        assert_eq!(compare(&[], &loans).unwrap(), vec![]);
        if width == 0 {
            continue;
        }
        for (id, loan) in loans.iter_mut().enumerate().take(width.min(513)).skip(1) {
            loan.parents = vec![id - 1, id - 1];
        }
        let last = width.min(513) - 1;
        let roots = [last, 0, last, last / 2];
        assert_eq!(
            compare(&roots, &loans).unwrap(),
            (0..=last).collect::<Vec<_>>()
        );
        loans[0].parents = vec![last, 0];
        assert_eq!(
            compare(&roots, &loans).unwrap(),
            (0..=last).collect::<Vec<_>>(),
            "cycles terminate but remain a complete closure"
        );
        if width > 513 {
            assert_eq!(
                compare(&[width - 1], &loans).unwrap(),
                vec![width - 1],
                "a sparse arena does not invent middle ancestors"
            );
        }
    }
}
#[test]
fn visited_words_keep_invalid_roots_parents_and_fresh_dynamic_arenas() {
    for width in [0, 1, 63, 64, 65, 511, 512] {
        let mut loans = vec![Loan::default(); width];
        assert!(compare(&[width], &loans).is_err());
        assert!(compare(&[usize::MAX], &loans).is_err());
        if width > 0 {
            loans[0].parents = vec![width];
            assert!(compare(&[0], &loans).is_err());
            loans[0].parents.clear();
            assert_eq!(
                compare(&[0], &loans).unwrap(),
                vec![0],
                "next call rebuilds its actual current graph"
            );
            loans[0].parents = vec![width - 1];
            let mut expected = vec![0, width - 1];
            expected.sort_unstable();
            expected.dedup();
            assert_eq!(compare(&[0], &loans).unwrap(), expected);
        }
    }
}
#[test]
fn word_ancestry_pays_real_work_and_fresh_exact_limits() {
    let mut loans = vec![Loan::default(); 512];
    for row in &mut loans[1..] {
        row.parents = vec![0, 0];
    }
    let ids = (0..512).rev().collect::<Vec<_>>();
    let mut old = Budget::new(&Limits::default());
    let mut new = Budget::new(&Limits::default());
    let expected = ancestry_by_linear_set(&ids, &loans, &mut old).unwrap();
    assert_eq!(ancestry(&ids, &loans, &mut new).unwrap(), expected);
    assert!(
        new.remaining > old.remaining,
        "bounded visited and ascending enumeration remove real scans/sort"
    );
    let n = Limits::default().max_analysis_steps - new.remaining;
    for steps in [n, n - 1, n] {
        let result = ancestry(
            &ids,
            &loans,
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == n {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let empty_arena = vec![Loan::default(); 100000];
    assert_eq!(
        ancestry(
            &[],
            &empty_arena,
            &mut Budget::new(&Limits {
                max_analysis_steps: 1,
                ..Limits::default()
            })
        )
        .unwrap(),
        vec![],
        "empty roots stay constant work even in a large arena"
    );
}

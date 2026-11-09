use super::*;
fn compare(to: &[usize], from: &[usize]) {
    let mut old = to.to_vec();
    let mut new = to.to_vec();
    let expected = add_loans_by_scan(&mut old, from, &mut Budget::new(&Limits::default())).unwrap();
    let actual = add_loans(&mut new, from, &mut Budget::new(&Limits::default())).unwrap();
    assert_eq!(
        (actual, new),
        (expected, old),
        "complete vector and changed including original duplicates: {to:?} <- {from:?}"
    );
}
fn sequences(depth: usize, values: &[usize]) -> Vec<Vec<usize>> {
    let mut out = vec![vec![]];
    let mut frontier = vec![vec![]];
    for _ in 0..depth {
        let mut next = vec![];
        for p in &frontier {
            for v in values {
                let mut row = p.clone();
                row.push(*v);
                next.push(row);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}
#[test]
fn loan_union_preserves_arbitrary_input_normalization_duplicates_and_changed() {
    let rows = sequences(3, &[0, 1, usize::MAX]);
    for to in &rows {
        for from in &rows {
            compare(to, from);
        }
    }
    compare(&[7, 7, 2, 2, usize::MAX], &[]);
    compare(&[7, 7, 2, 2, usize::MAX], &[7, 2, 1, 1, usize::MAX]);
}
#[test]
fn loan_union_preserves_large_sorted_reverse_and_partial_rows() {
    for width in [0, 1, 63, 64, 65, 127, 128, 129, 511, 512] {
        let normal = (0..width).collect::<Vec<_>>();
        let reverse = normal.iter().rev().copied().collect::<Vec<_>>();
        let repeated = normal.iter().flat_map(|id| [*id, *id]).collect::<Vec<_>>();
        compare(&normal, &reverse);
        compare(&reverse, &normal);
        compare(&reverse, &[]);
        compare(&repeated, &[width, usize::MAX, width]);
    }
}
#[test]
fn sorted_union_removes_real_scans_and_keeps_fresh_exact_work() {
    let initial = (0..512).collect::<Vec<_>>();
    let from = initial.iter().rev().copied().collect::<Vec<_>>();
    let mut old = initial.clone();
    let mut new = initial.clone();
    let mut old_b = Budget::new(&Limits::default());
    let mut new_b = Budget::new(&Limits::default());
    assert_eq!(
        add_loans(&mut new, &from, &mut new_b).unwrap(),
        add_loans_by_scan(&mut old, &from, &mut old_b).unwrap()
    );
    assert_eq!(new, old);
    assert!(
        new_b.remaining > old_b.remaining,
        "binary membership removes actual full scans and repeated sorting"
    );
    let n = Limits::default().max_analysis_steps - new_b.remaining;
    for steps in [n, n - 1, n] {
        let mut to = initial.clone();
        let result = add_loans(
            &mut to,
            &from,
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == n {
            assert!(!result.unwrap());
            assert_eq!(to, old);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let mut reversed = from;
    sort_loan_ids(&mut reversed, &mut Budget::new(&Limits::default())).unwrap();
    assert_eq!(
        reversed, initial,
        "budgeted actual merge sort handles unordered input without a guessed quadratic fee"
    );
}

fn boundary(initial: &[usize], from: &[usize], label: &str) {
    let construct = || {
        let mut row = Vec::with_capacity(initial.len());
        row.extend_from_slice(initial);
        assert_eq!(
            row.capacity(),
            row.len(),
            "force the first new ID to grow storage"
        );
        row
    };
    let mut expected = initial.to_vec();
    let changed =
        add_loans_by_scan(&mut expected, from, &mut Budget::new(&Limits::default())).unwrap();
    let mut row = construct();
    let mut budget = Budget::new(&Limits::default());
    assert_eq!(add_loans(&mut row, from, &mut budget).unwrap(), changed);
    assert_eq!(row, expected);
    let n = Limits::default().max_analysis_steps - budget.remaining;
    for steps in [n, n - 1, n] {
        let mut row = construct();
        let result = add_loans(
            &mut row,
            from,
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == n {
            assert_eq!(result.unwrap(), changed);
            assert_eq!(row, expected);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    println!("{label}: N={n}, N-1 ResourceLimit, fresh N complete Vec/changed identical");
}
#[test]
fn inserted_interleaved_ids_sort_and_capacity_growth_have_real_exact_boundaries() {
    let evens = (0..512).map(|i| 2 * i).collect::<Vec<_>>();
    let reverse_odds = (0..512).rev().map(|i| 2 * i + 1).collect::<Vec<_>>();
    compare(&evens, &reverse_odds);
    boundary(
        &evens,
        &reverse_odds,
        "512 interleaved new IDs with real tail shifts",
    );
    boundary(
        &(0..129).rev().collect::<Vec<_>>(),
        &[],
        "unordered row normalized with empty from",
    );
    boundary(
        &[2],
        &[0, 1, 3],
        "initial capacity exactly one, actual growth plus front/middle shifts",
    );
    let mut row = (0..65).rev().collect::<Vec<_>>();
    let mut sorted = row.clone();
    sorted.sort_unstable();
    let mut full = row.clone();
    let mut budget = Budget::new(&Limits::default());
    assert!(!add_loans(&mut full, &[], &mut budget).unwrap());
    let n = Limits::default().max_analysis_steps - budget.remaining;
    let error = add_loans(
        &mut row,
        &[],
        &mut Budget::new(&Limits {
            max_analysis_steps: n - 1,
            ..Limits::default()
        }),
    )
    .unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    row.sort_unstable();
    assert_eq!(
        row, sorted,
        "a failed merge pass never loses or duplicates any original ID"
    );
}

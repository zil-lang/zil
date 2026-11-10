//! Independent Vec<u8> initialization semantics, including its zip domain.
use super::*;
fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn packed(values: &[u8]) -> Initialized {
    Initialized::from_flat(values, &mut budget(1_000_000)).unwrap()
}
fn scalar_merge(left: &mut [u8], right: &[u8]) -> bool {
    let mut changed = false;
    for (a, b) in left.iter_mut().zip(right) {
        let old = *a;
        *a |= *b;
        changed |= old != *a;
    }
    changed
}
#[test]
fn packed_initialization_matches_every_scalar_state_across_word_boundaries() {
    for len in [
        0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129,
    ] {
        for value in 0..4 {
            let mut old = vec![value; len];
            let mut new = Initialized::new(len, value, &mut budget(1_000_000)).unwrap();
            assert_eq!(new.flat(), old);
            for (index, slot) in old.iter_mut().enumerate() {
                assert_eq!(new.get(index, &mut budget(100)).unwrap(), *slot);
                let next = ((index + value as usize) % 4) as u8;
                *slot = next;
                new.set(index, next, &mut budget(100)).unwrap();
            }
            assert_eq!(new.flat(), old);
            let alias = new.clone();
            let snapshot = old.clone();
            for (index, slot) in old.iter_mut().enumerate().rev() {
                *slot = (*slot + 1) % 4;
                new.set(index, *slot, &mut budget(100)).unwrap();
            }
            assert_eq!(new.flat(), old);
            assert_eq!(alias.flat(), snapshot);
        }
    }
}
#[test]
fn packed_initialization_or_preserves_scalar_zip_and_the_complete_suffix() {
    let lengths = [0, 1, 7, 8, 9, 31, 32, 33, 63, 64, 65];
    for left_len in lengths {
        for right_len in lengths {
            for seed in 0..4 {
                let mut old: Vec<_> = (0..left_len).map(|i| ((i + seed) % 4) as u8).collect();
                let right: Vec<_> = (0..right_len).map(|i| ((i + seed + 1) % 4) as u8).collect();
                let mut new = packed(&old);
                let incoming = packed(&right);
                let alias = new.clone();
                let snapshot = old.clone();
                for _ in 0..2 {
                    assert_eq!(
                        new.merge(&incoming, &mut budget(1_000_000)).unwrap(),
                        scalar_merge(&mut old, &right)
                    );
                    assert_eq!(new.flat(), old);
                }
                assert_eq!(alias.flat(), snapshot);
                assert_eq!(incoming.flat(), right);
            }
        }
    }
}
#[test]
fn packed_initialization_rejects_bad_inventory_tail_value_and_index_without_writing() {
    for len in [0, 1, 31, 32, 33, 64, 65] {
        let mut value = Initialized::new(len, 1, &mut budget(1_000_000)).unwrap();
        let snapshot = value.clone();
        assert!(value.get(len, &mut budget(100)).is_err());
        assert!(value.set(len, 2, &mut budget(100)).is_err());
        assert_eq!(value, snapshot);
        if len > 0 {
            assert!(value.set(0, 4, &mut budget(100)).is_err());
            assert_eq!(value, snapshot);
        }
        assert!(Initialized::new(len, 4, &mut budget(100)).is_err());
        let missing = Initialized {
            storage: Storage::Packed {
                len,
                words: vec![0; len.div_ceil(LANES) + 1],
            },
        };
        let mut clean = value.clone();
        assert!(clean.merge(&missing, &mut budget(1_000_000)).is_err());
        assert_eq!(clean, value);
        assert!(missing.get(0, &mut budget(100)).is_err());
        if len > 0 && !len.is_multiple_of(LANES) {
            let mut words = vec![0; len.div_ceil(LANES)];
            *words.last_mut().unwrap() |= 1usize << ((len % LANES) * 2);
            let tail = Initialized {
                storage: Storage::Packed { len, words },
            };
            assert!(tail.get(0, &mut budget(100)).is_err());
            let mut left = value.clone();
            assert!(left.merge(&tail, &mut budget(1_000_000)).is_err());
            assert_eq!(left, value);
        }
    }
}
#[test]
fn packed_initialization_set_prepays_all_work_before_atomic_write() {
    for len in [1, SMALL - 1, SMALL, SMALL + 1, 65] {
        let index = len / 2;
        let seed = Initialized::new(len, 1, &mut budget(1000)).unwrap();
        let base = vec![1; len];
        let mut expected = base.clone();
        expected[index] = 2;
        let run = |n| {
            let mut value = seed.clone();
            let result = value.set(index, 2, &mut budget(n));
            (result, value)
        };
        let mut n = 0;
        while run(n).0.is_err() {
            n += 1;
            assert!(n < 100);
        }
        let (result, value) = run(n);
        result.unwrap();
        assert_eq!(value.get(index, &mut budget(100)).unwrap(), 2);
        assert_eq!(value.flat(), expected);
        assert_eq!(
            run(n - 1).0.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(run(n - 1).1, seed);
        let (first_result, first) = run(n);
        let (second_result, second) = run(n);
        first_result.unwrap();
        second_result.unwrap();
        assert_eq!(first, second);
        assert_eq!(first.flat(), expected);
        assert_eq!(second.flat(), expected);
        assert_eq!(seed.flat(), base);
        eprintln!("V adaptive initialization atomic set len={len} N={n}; N-1/fresh checked");
    }
}

#[test]
fn packed_initialization_repeated_clone_or_has_exact_fresh_request_boundaries() {
    let len = 1025;
    let original: Vec<_> = (0..len).map(|i| (i % 4) as u8).collect();
    let incoming: Vec<_> = (0..len).map(|i| ((i + 1) % 4) as u8).collect();
    let seed = packed(&original);
    let right = packed(&incoming);
    let run = |old: bool, n| {
        let mut b = budget(n);
        if old {
            let mut value = original.clone();
            for _ in 0..16 {
                b.spend(value.len())?;
                let mut copy = value.clone();
                b.spend(incoming.len())?;
                scalar_merge(&mut copy, &incoming);
                value = copy;
            }
            Ok::<_, Diagnostic>(value)
        } else {
            let mut value = seed.clone();
            for _ in 0..16 {
                b.spend(value.copy_work())?;
                let mut copy = value.clone();
                copy.merge(&right, &mut b)?;
                value = copy;
            }
            Ok(value.flat())
        }
    };
    let boundary = |old| {
        let (mut low, mut high) = (0, 1_000_000);
        assert!(run(old, high).is_ok());
        while low < high {
            let middle = (low + high) / 2;
            if run(old, middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        low
    };
    let new_n = boundary(false);
    let old_n = boundary(true);
    assert!(new_n < old_n, "packed N={new_n}, scalar N={old_n}");
    assert_eq!(
        run(false, new_n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(
        run(true, old_n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(run(false, new_n).unwrap(), run(true, old_n).unwrap());
    assert_eq!(run(false, new_n).unwrap(), run(false, new_n).unwrap());
    eprintln!(
        "V packed initialization 1025 locals x16 clone/OR N={new_n}, scalar N={old_n}; N-1/fresh checked"
    );
}

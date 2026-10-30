//! Query-read mechanism benchmark. Graph construction is outside the timer.
use std::hint::black_box;
use std::time::Instant;
use zil_base::{Fingerprint, ModuleId, Phase, ResultCategory};
use zil_db::{Database, Dependency, FingerprintKind, QueryKey, ResultFingerprints};

fn publish(db: &mut Database, keys: &[QueryKey], id: usize, inputs: &[usize]) {
    let fps = ResultFingerprints::default();
    db.begin(keys[id].clone()).unwrap();
    db.publish(
        keys[id].clone(),
        ResultCategory::Success,
        fps.clone(),
        inputs
            .iter()
            .map(|&input| Dependency {
                key: keys[input].clone(),
                kind: FingerprintKind::Semantic,
                observed: fps.semantic,
            })
            .collect(),
        vec![1],
    )
    .unwrap();
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        4,
        "usage: query_read_cp23 SCENARIO N REPETITIONS"
    );
    let scenario = args[1].as_str();
    let n: usize = args[2].parse().unwrap();
    let repetitions: usize = args[3].parse().unwrap();
    assert!(n > 0 && n < (u32::MAX as usize - 1) / 3 && repetitions > 0);
    let count = if scenario == "diamond" { 3 * n + 1 } else { n };
    let keys: Vec<_> = (0..=count)
        .map(|id| QueryKey {
            module: ModuleId(id as u32),
            phase: Phase::Types,
            input: Fingerprint::of(&(id as u64).to_le_bytes()),
            rules: Fingerprint::of(b"cp23-rules"),
            target: Fingerprint::of(b"cp23-target"),
        })
        .collect();
    let mut db = Database::default();
    let read_id = match scenario {
        "missing" | "leaf" => {
            for id in 0..count {
                publish(&mut db, &keys, id, &[]);
            }
            if scenario == "missing" {
                count
            } else {
                count / 2
            }
        }
        "chain" => {
            publish(&mut db, &keys, 0, &[]);
            for id in 1..count {
                publish(&mut db, &keys, id, &[id - 1]);
            }
            count - 1
        }
        "diamond" => {
            publish(&mut db, &keys, 0, &[]);
            for layer in 0..n {
                let base = 3 * layer;
                publish(&mut db, &keys, base + 1, &[base]);
                publish(&mut db, &keys, base + 2, &[base]);
                publish(&mut db, &keys, base + 3, &[base + 1, base + 2]);
            }
            count - 1
        }
        _ => panic!("unknown scenario: {scenario}"),
    };
    let mut checksum = 0usize;
    let start = Instant::now();
    for _ in 0..repetitions {
        let result = black_box(db.read(black_box(&keys[read_id])).unwrap());
        checksum = checksum.wrapping_add(result.map_or(0, |snapshot| snapshot.revision as usize));
    }
    let elapsed = start.elapsed().as_nanos();
    let expected = if scenario == "missing" {
        0
    } else {
        read_id + 1
    };
    assert_eq!(checksum, expected.wrapping_mul(repetitions));
    assert_eq!(db.len(), count);
    println!(
        "{{\"scenario\":\"{scenario}\",\"n\":{n},\"nodes\":{count},\"repetitions\":{repetitions},\"elapsed_ns\":{elapsed},\"checksum\":{checksum}}}"
    );
}

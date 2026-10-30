//! Focused query-metadata microbenchmark; not an end-to-end compiler score.
//! Build the same example against each revision and alternate the binaries.
use std::hint::black_box;
use std::time::{Duration, Instant};
use zil_base::{Fingerprint, ModuleId, Phase, ResultCategory};
use zil_db::{Database, Dependency, FingerprintKind, QueryKey, ResultFingerprints};

fn publish(db: &mut Database, key: &QueryKey, fps: &ResultFingerprints, input: Option<&QueryKey>) {
    let dependencies = input
        .map(|key| Dependency {
            key: key.clone(),
            kind: FingerprintKind::Semantic,
            observed: fps.semantic,
        })
        .into_iter()
        .collect();
    db.begin(key.clone()).unwrap();
    black_box(
        db.publish(
            key.clone(),
            ResultCategory::Success,
            fps.clone(),
            dependencies,
            vec![1],
        )
        .unwrap(),
    );
}

fn populate(keys: &[QueryKey], fps: &ResultFingerprints, fanout: bool) -> Database {
    let mut db = Database::default();
    for (index, key) in keys.iter().enumerate() {
        publish(&mut db, key, fps, (fanout && index > 0).then_some(&keys[0]));
    }
    db
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        4,
        "usage: compile_performance SCENARIO N REPETITIONS"
    );
    let scenario = args[1].as_str();
    let n: u32 = args[2].parse().unwrap();
    let repetitions: u32 = args[3].parse().unwrap();
    assert!(n > 0 && repetitions > 0 && n < u32::MAX);
    let keys: Vec<_> = (0..=n)
        .map(|id| QueryKey {
            module: ModuleId(id),
            phase: Phase::Types,
            input: Fingerprint::of(&id.to_le_bytes()),
            rules: Fingerprint::of(b"cp21-rules"),
            target: Fingerprint::of(b"cp21-target"),
        })
        .collect();
    let fps = ResultFingerprints {
        semantic: Fingerprint::of(b"semantic"),
        abi: Fingerprint::of(b"abi"),
        private_implementation: Fingerprint::of(b"body"),
        source_positions: Fingerprint::of(b"positions"),
        build_inputs: Fingerprint::of(b"inputs"),
    };
    let mut elapsed = Duration::ZERO;
    let mut checksum = 0usize;
    match scenario {
        "new-independent" | "new-fanout" => {
            for _ in 0..repetitions {
                let start = Instant::now();
                let db = populate(&keys[..n as usize], &fps, scenario == "new-fanout");
                elapsed += start.elapsed();
                assert_eq!(db.len(), n as usize);
                checksum += black_box(db.len());
            }
        }
        "absent-invalidate" | "existing-unchanged" => {
            let mut db = populate(&keys[..n as usize], &fps, true);
            let start = Instant::now();
            for _ in 0..repetitions {
                if scenario == "absent-invalidate" {
                    assert!(black_box(db.invalidate(&keys[n as usize], &fps)).is_empty());
                } else {
                    publish(&mut db, &keys[0], &fps, None);
                }
                checksum += black_box(db.len());
            }
            elapsed += start.elapsed();
            assert_eq!(db.len(), n as usize);
        }
        "existing-changed" => {
            let changed = ResultFingerprints {
                semantic: Fingerprint::of(b"changed"),
                ..fps.clone()
            };
            for _ in 0..repetitions {
                let mut db = populate(&keys[..n as usize], &fps, true);
                let start = Instant::now();
                publish(&mut db, &keys[0], &changed, None);
                elapsed += start.elapsed();
                assert_eq!(db.len(), 1);
                checksum += black_box(db.len());
            }
        }
        _ => panic!("unknown scenario: {scenario}"),
    }
    println!(
        "{{\"scenario\":\"{scenario}\",\"n\":{n},\"repetitions\":{repetitions},\"elapsed_ns\":{},\"checksum\":{checksum}}}",
        elapsed.as_nanos()
    );
}

//! Public-API controls for the published-graph invariant. Reachability is
//! computed by a boolean matrix, independently of Database's DFS traversal.
use std::sync::Arc;
use zil_base::{Fingerprint, ModuleId, Phase, ResultCategory};
use zil_db::{Database, Dependency, FingerprintKind, QueryKey, QuerySnapshot, ResultFingerprints};

fn key(id: usize) -> QueryKey {
    QueryKey {
        module: ModuleId(id as u32),
        phase: Phase::Types,
        input: Fingerprint::of(&[id as u8]),
        rules: Fingerprint::ZERO,
        target: Fingerprint::ZERO,
    }
}

fn publish(db: &mut Database, id: usize, dependencies: Vec<Dependency>) -> Arc<QuerySnapshot> {
    db.begin(key(id)).unwrap();
    db.publish(
        key(id),
        ResultCategory::Success,
        ResultFingerprints::default(),
        dependencies,
        vec![id as u8],
    )
    .unwrap()
}

#[test]
fn every_four_node_dag_matches_matrix_reachability_with_each_active_subset() {
    // All six possible backwards edges, including duplicate observations.
    // Existing unit tests independently cover 9,600 invalidation/replacement
    // combinations over every result facet; this test adds active read states.
    for edges in 0..64 {
        let mut db = Database::default();
        let mut reaches = [[false; 4]; 4];
        let mut published = Vec::new();
        for consumer in 0..4 {
            reaches[consumer][consumer] = true;
            let mut dependencies = Vec::new();
            for producer in 0..consumer {
                let bit = consumer * (consumer - 1) / 2 + producer;
                if edges & (1 << bit) != 0 {
                    reaches[consumer][producer] = true;
                    let edge = Dependency {
                        key: key(producer),
                        kind: FingerprintKind::Semantic,
                        observed: Fingerprint::ZERO,
                    };
                    dependencies.extend([edge.clone(), edge]);
                }
            }
            published.push(publish(&mut db, consumer, dependencies));
        }
        for via in 0..4 {
            for from in 0..4 {
                for to in 0..4 {
                    reaches[from][to] |= reaches[from][via] && reaches[via][to];
                }
            }
        }
        for active in 0..16 {
            for id in 0..4 {
                if active & (1 << id) != 0 {
                    db.begin(key(id)).unwrap();
                }
            }
            for id in 0..4 {
                let result = db.read(&key(id));
                if (0..4).any(|ancestor| reaches[id][ancestor] && active & (1 << ancestor) != 0) {
                    assert_eq!(result.unwrap_err().code, "E-QUERY-CYCLE");
                } else {
                    assert!(Arc::ptr_eq(&result.unwrap().unwrap(), &published[id]));
                }
            }
            assert!(db.read(&key(4)).unwrap().is_none());
            for id in 0..4 {
                db.abort(&key(id));
            }
            assert_eq!(db.active_count(), 0);
            assert!(Arc::ptr_eq(
                &db.read(&key(3)).unwrap().unwrap(),
                &published[3]
            ));
        }
        // A missing node is still active before its first publication.
        db.begin(key(4)).unwrap();
        assert_eq!(db.read(&key(4)).unwrap_err().code, "E-QUERY-CYCLE");
        db.clear();
        assert!(db.read(&key(4)).unwrap().is_none());
        assert_eq!(db.active_count(), 0);
    }
}

#[test]
fn changing_an_exported_arc_cannot_mutate_the_published_graph() {
    let mut db = Database::default();
    let original = publish(&mut db, 0, vec![]);
    let mut exported = db.read(&key(0)).unwrap().unwrap();
    let changed = Arc::make_mut(&mut exported);
    changed.fingerprints.semantic = Fingerprint::of(b"unpublished");
    changed.dependencies.push(Dependency {
        key: key(9),
        kind: FingerprintKind::Semantic,
        observed: Fingerprint::ZERO,
    });
    changed.bytes = Arc::from([99]);
    let current = db.read(&key(0)).unwrap().unwrap();
    assert!(Arc::ptr_eq(&current, &original));
    assert_eq!(current.fingerprints, ResultFingerprints::default());
    assert!(current.dependencies.is_empty());
    assert_eq!(&*current.bytes, &[0]);
    // The private table is the only publication path: even a detached copy
    // with arbitrary public fields cannot create or replace a stored node.
    assert!(!Arc::ptr_eq(&exported, &current));
    assert!(db.read(&key(9)).unwrap().is_none());
}

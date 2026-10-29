use super::*;
fn key(n: u8) -> QueryKey {
    QueryKey {
        module: ModuleId(n as u32),
        phase: Phase::Types,
        input: Fingerprint::of(&[n]),
        rules: Fingerprint::of(b"rules"),
        target: Fingerprint::of(b"target"),
    }
}
fn fingerprints() -> ResultFingerprints {
    ResultFingerprints {
        semantic: Fingerprint::of(b"contract"),
        abi: Fingerprint::of(b"abi"),
        private_implementation: Fingerprint::of(b"body"),
        source_positions: Fingerprint::of(b"positions"),
        build_inputs: Fingerprint::of(b"build"),
    }
}
fn publish(
    db: &mut Database,
    id: u8,
    fps: ResultFingerprints,
    dependencies: Vec<Dependency>,
) -> Arc<QuerySnapshot> {
    db.begin(key(id)).unwrap();
    db.publish(
        key(id),
        ResultCategory::Success,
        fps,
        dependencies,
        vec![id],
    )
    .unwrap()
}
fn edge(id: u8, kind: FingerprintKind, fps: &ResultFingerprints) -> Dependency {
    Dependency {
        key: key(id),
        kind,
        observed: fps.get(kind),
    }
}

#[test]
fn all_fingerprint_kinds_control_the_actual_transitive_invalidation_set() {
    let kinds = [
        FingerprintKind::Semantic,
        FingerprintKind::Abi,
        FingerprintKind::PrivateImplementation,
        FingerprintKind::SourcePositions,
        FingerprintKind::BuildInputs,
    ];
    for (changed_index, kind) in kinds.into_iter().enumerate() {
        let mut db = Database::default();
        let old = fingerprints();
        publish(&mut db, 0, old.clone(), vec![]);
        let mut original = Vec::new();
        for (index, edge_kind) in kinds.into_iter().enumerate() {
            original.push(publish(
                &mut db,
                index as u8 + 1,
                old.clone(),
                vec![edge(0, edge_kind, &old)],
            ));
            publish(
                &mut db,
                index as u8 + 11,
                old.clone(),
                vec![edge(index as u8 + 1, FingerprintKind::Semantic, &old)],
            );
        }
        let mut new = old.clone();
        let changed = Fingerprint::of(b"changed");
        match kind {
            FingerprintKind::Semantic => new.semantic = changed,
            FingerprintKind::Abi => new.abi = changed,
            FingerprintKind::PrivateImplementation => new.private_implementation = changed,
            FingerprintKind::SourcePositions => new.source_positions = changed,
            FingerprintKind::BuildInputs => new.build_inputs = changed,
        }
        // Replacing a published producer triggers invalidation without relying
        // on callers remembering a separate invalidate operation.
        publish(&mut db, 0, new, vec![]);
        for (index, previous) in original.iter().enumerate() {
            let direct = db.read(&key(index as u8 + 1)).unwrap();
            let indirect = db.read(&key(index as u8 + 11)).unwrap();
            if index == changed_index {
                assert!(direct.is_none());
                assert!(indirect.is_none());
            } else {
                assert!(Arc::ptr_eq(&direct.unwrap(), previous));
                assert!(indirect.is_some());
            }
        }
    }
}

#[test]
fn dependency_cycles_and_active_ancestors_cannot_be_observed_or_published() {
    let mut db = Database::default();
    let fps = fingerprints();
    publish(&mut db, 0, fps.clone(), vec![]);
    publish(
        &mut db,
        1,
        fps.clone(),
        vec![edge(0, FingerprintKind::Semantic, &fps)],
    );
    publish(
        &mut db,
        2,
        fps.clone(),
        vec![edge(1, FingerprintKind::Semantic, &fps)],
    );
    db.begin(key(0)).unwrap();
    assert_eq!(db.read(&key(2)).unwrap_err().code, "E-QUERY-CYCLE");
    let error = db
        .publish(
            key(0),
            ResultCategory::Success,
            fps.clone(),
            vec![edge(2, FingerprintKind::Semantic, &fps)],
            vec![],
        )
        .unwrap_err();
    assert_eq!(error.code, "E-QUERY-CYCLE");
    assert_eq!(db.active_count(), 0);
    assert!(db.read(&key(2)).unwrap().is_some());
}

#[test]
fn stale_input_and_cancelled_publication_abort_the_active_record() {
    let mut db = Database::default();
    db.begin(key(1)).unwrap();
    let error = db
        .publish(
            key(1),
            ResultCategory::Success,
            fingerprints(),
            vec![Dependency {
                key: key(2),
                kind: FingerprintKind::Abi,
                observed: Fingerprint::ZERO,
            }],
            vec![],
        )
        .unwrap_err();
    assert_eq!(error.code, "E-QUERY-STALE-INPUT");
    assert_eq!(db.active_count(), 0);
    db.begin(key(1)).unwrap();
    assert_eq!(
        db.publish(
            key(1),
            ResultCategory::Cancelled,
            fingerprints(),
            vec![],
            vec![]
        )
        .unwrap_err()
        .code,
        "E-QUERY-INCOMPLETE"
    );
    assert!(db.is_empty());
    assert_eq!(db.active_count(), 0);
}

#[test]
fn cancellation_is_shared_and_deadlines_do_not_require_background_threads() {
    let token = CancellationToken::default();
    let other = token.clone();
    assert!(!token.is_cancelled());
    other.cancel();
    assert_eq!(
        token.check(Phase::Analyze).unwrap_err().category,
        ResultCategory::Cancelled
    );
    let deadline = CancellationToken::with_timeout(Duration::ZERO);
    assert!(deadline.is_cancelled());
}

// Frozen pre-084 algorithm: test-only complete scan, including absent keys.
// It must remain independent of Database::invalidate's shortcut.
fn legacy_invalidated(
    snapshots: &BTreeMap<QueryKey, Arc<QuerySnapshot>>,
    producer: &QueryKey,
    new: &ResultFingerprints,
) -> BTreeSet<QueryKey> {
    let mut gone = BTreeSet::new();
    loop {
        let next = snapshots
            .iter()
            .filter(|(candidate, value)| {
                !gone.contains(*candidate)
                    && value.dependencies.iter().any(|dependency| {
                        (dependency.key == *producer
                            && new.get(dependency.kind) != dependency.observed)
                            || gone.contains(&dependency.key)
                    })
            })
            .map(|(candidate, _)| candidate.clone())
            .collect::<Vec<_>>();
        if next.is_empty() {
            return gone;
        }
        gone.extend(next);
    }
}

fn assert_closed(db: &Database) {
    for snapshot in db.snapshots.values() {
        for dependency in &snapshot.dependencies {
            assert!(db.snapshots.contains_key(&dependency.key));
        }
    }
}

#[test]
fn absent_producers_match_legacy_scanning_on_reachable_dags_and_after_eviction() {
    let kinds = [
        FingerprintKind::Semantic,
        FingerprintKind::Abi,
        FingerprintKind::PrivateImplementation,
        FingerprintKind::SourcePositions,
        FingerprintKind::BuildInputs,
    ];
    let old = fingerprints();
    let changed = Fingerprint::of(b"084-changed");
    let mut compared = 0;
    // All 64 topologically numbered four-node DAGs, five rotations of edge
    // kinds, each existing/absent producer and each changed/unchanged result.
    // Duplicated edges are intentional and cannot enlarge the lost-node set.
    for mask in 0..64u8 {
        for rotation in 0..kinds.len() {
            for producer in 0..=4u8 {
                for change in 0..=kinds.len() {
                    let mut db = Database::default();
                    for consumer in 0..4u8 {
                        let mut dependencies = Vec::new();
                        for input in 0..consumer {
                            let bit = consumer * consumer.saturating_sub(1) / 2 + input;
                            if mask & (1 << bit) != 0 {
                                let kind = kinds[(bit as usize + rotation) % kinds.len()];
                                let dependency = edge(input, kind, &old);
                                dependencies.push(dependency.clone());
                                if bit % 2 == 0 {
                                    dependencies.push(dependency);
                                }
                            }
                        }
                        publish(&mut db, consumer, old.clone(), dependencies);
                    }
                    assert_closed(&db);
                    let mut new = old.clone();
                    match change {
                        0 => new.semantic = changed,
                        1 => new.abi = changed,
                        2 => new.private_implementation = changed,
                        3 => new.source_positions = changed,
                        4 => new.build_inputs = changed,
                        _ => {}
                    }
                    let original = db.snapshots.clone();
                    let expected = legacy_invalidated(&original, &key(producer), &new);
                    let actual = db.invalidate(&key(producer), &new);
                    assert_eq!(actual, expected.iter().cloned().collect::<Vec<_>>());
                    assert_eq!(db.len(), original.len() - expected.len());
                    assert_closed(&db);
                    for (id, snapshot) in &original {
                        if expected.contains(id) {
                            assert!(db.read(id).unwrap().is_none());
                        } else {
                            assert!(Arc::ptr_eq(&db.read(id).unwrap().unwrap(), snapshot));
                        }
                    }
                    // Deleted keys also have no surviving incoming edges.
                    // Restore in topological order through the real publish API.
                    for deleted in &expected {
                        assert!(db.invalidate(deleted, &new).is_empty());
                        let snapshot = &original[deleted];
                        publish(
                            &mut db,
                            deleted.module.0 as u8,
                            snapshot.fingerprints.clone(),
                            snapshot.dependencies.clone(),
                        );
                        assert_closed(&db);
                    }
                    assert_eq!(db.len(), original.len());
                    // The public replacement path must retain kind-selective
                    // transitive invalidation, including producers with inputs.
                    let dependencies = original
                        .get(&key(producer))
                        .map_or_else(Vec::new, |s| s.dependencies.clone());
                    publish(&mut db, producer, new, dependencies);
                    for id in original.keys() {
                        assert_eq!(db.read(id).unwrap().is_none(), expected.contains(id));
                    }
                    assert_closed(&db);
                    assert_eq!(db.active_count(), 0);
                    compared += 1;
                }
            }
        }
    }
    assert_eq!(compared, 9_600);
}

#[test]
fn absent_invalidation_preserves_active_queries_revision_and_published_identity() {
    let mut db = Database::default();
    let fps = fingerprints();
    let snapshot = publish(&mut db, 0, fps.clone(), vec![]);
    publish(
        &mut db,
        1,
        fps.clone(),
        vec![edge(0, FingerprintKind::Semantic, &fps)],
    );
    db.begin(key(0)).unwrap();
    let revision = db.revision;
    assert!(
        db.invalidate(&key(7), &ResultFingerprints::default())
            .is_empty()
    );
    assert_eq!(db.revision, revision);
    assert_eq!(db.active_count(), 1);
    assert_eq!(db.read(&key(1)).unwrap_err().code, "E-QUERY-CYCLE");
    db.abort(&key(0));
    assert!(Arc::ptr_eq(&db.read(&key(0)).unwrap().unwrap(), &snapshot));
    assert_closed(&db);
    db.clear();
    assert!(db.invalidate(&key(0), &fps).is_empty());
    assert_eq!(db.active_count(), 0);
    publish(&mut db, 0, fps, vec![]);
    assert_closed(&db);
}

#[test]
fn failed_new_publications_cannot_create_forward_edges_or_replace_old_snapshots() {
    let mut db = Database::default();
    let fps = fingerprints();
    let snapshot = publish(&mut db, 0, fps.clone(), vec![]);
    for status in [ResultCategory::Cancelled, ResultCategory::InfraError] {
        db.begin(key(1)).unwrap();
        let error = db
            .publish(key(1), status, fps.clone(), vec![], vec![])
            .unwrap_err();
        assert_eq!(error.code, "E-QUERY-INCOMPLETE");
        assert_eq!(db.active_count(), 0);
        assert_eq!(db.len(), 1);
    }
    let error = db
        .publish(key(1), ResultCategory::Success, fps.clone(), vec![], vec![])
        .unwrap_err();
    assert_eq!(error.code, "E-QUERY-NOT-ACTIVE");
    db.begin(key(1)).unwrap();
    let error = db
        .publish(
            key(1),
            ResultCategory::Success,
            fps.clone(),
            vec![edge(2, FingerprintKind::Semantic, &fps)],
            vec![],
        )
        .unwrap_err();
    assert_eq!(error.code, "E-QUERY-STALE-INPUT");
    assert_eq!(db.active_count(), 0);
    assert!(db.invalidate(&key(2), &fps).is_empty());
    assert!(Arc::ptr_eq(&db.read(&key(0)).unwrap().unwrap(), &snapshot));
    assert_closed(&db);
}

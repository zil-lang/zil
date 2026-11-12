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

#[test]
fn collecting_raw_roots_keeps_transitive_receipts_and_current_invalidation() {
    let mut db = Database::default();
    let fps = fingerprints();
    publish(&mut db, 0, fps.clone(), vec![]);
    publish(
        &mut db,
        1,
        fps.clone(),
        vec![edge(0, FingerprintKind::Semantic, &fps)],
    );
    let root = publish(
        &mut db,
        2,
        fps.clone(),
        vec![edge(1, FingerprintKind::Abi, &fps)],
    );
    publish(&mut db, 3, fps.clone(), vec![]);
    db.retain_reachable([&key(2)]);
    assert_eq!(db.len(), 3);
    assert!(Arc::ptr_eq(&root, &db.read(&key(2)).unwrap().unwrap()));
    assert!(db.read(&key(3)).unwrap().is_none());
    let mut changed = fps;
    changed.semantic = Fingerprint::of(b"changed after raw eviction");
    publish(&mut db, 0, changed, vec![]);
    assert!(db.read(&key(2)).unwrap().is_none());
    db.retain_reachable(std::iter::empty());
    assert!(db.is_empty());
}

#[test]
fn collection_does_not_interrupt_an_active_query() {
    let mut db = Database::default();
    publish(&mut db, 0, fingerprints(), vec![]);
    db.begin(key(1)).unwrap();
    db.retain_reachable(std::iter::empty());
    assert_eq!(db.len(), 1);
    assert_eq!(db.active_count(), 1);
    db.abort(&key(1));
    db.retain_reachable(std::iter::empty());
    assert!(db.is_empty());
}

#[test]
fn immutable_read_batch_checks_the_compiler_frontend_prefix_once() {
    let mut db = Database::default();
    let fps = fingerprints();
    publish(&mut db, 0, fps.clone(), vec![]);
    publish(&mut db, 1, fps.clone(), vec![]);
    publish(
        &mut db,
        2,
        fps.clone(),
        vec![
            edge(0, FingerprintKind::Semantic, &fps),
            edge(1, FingerprintKind::Semantic, &fps),
        ],
    );
    publish(
        &mut db,
        3,
        fps.clone(),
        vec![edge(2, FingerprintKind::Semantic, &fps)],
    );
    publish(
        &mut db,
        4,
        fps,
        vec![edge(3, FingerprintKind::Semantic, &fingerprints())],
    );

    let mut separate_work = 0;
    let mut expected = Vec::new();
    for id in [4, 3, 2] {
        let mut reads = db.read_batch();
        expected.push(reads.read(&key(id)).unwrap().unwrap());
        separate_work += reads.stats().validated_snapshots;
    }
    assert_eq!(separate_work, 12);
    let mut reads = db.read_batch();
    for (id, expected) in [4, 3, 2].into_iter().zip(expected) {
        assert!(Arc::ptr_eq(
            &reads.read(&key(id)).unwrap().unwrap(),
            &expected
        ));
    }
    assert_eq!(
        reads.stats(),
        ReadBatchStats {
            validated_snapshots: 5,
            dependency_checks: 4,
            reused_validations: 2,
            retained_validations: 5
        }
    );
    for id in 10..=200 {
        assert!(reads.read(&key(id)).unwrap().is_none());
    }
    assert_eq!(reads.stats().retained_validations, db.len());
    assert_eq!(reads.stats().validated_snapshots, 5);
}

#[test]
fn new_views_follow_switched_dependencies_and_never_reuse_a_prior_terminal_value() {
    let mut db = Database::default();
    let fps = fingerprints();
    publish(&mut db, 0, fps.clone(), vec![]);
    publish(&mut db, 1, fps.clone(), vec![]);
    publish(
        &mut db,
        2,
        fps.clone(),
        vec![edge(0, FingerprintKind::Semantic, &fps)],
    );
    let original = publish(
        &mut db,
        3,
        fps.clone(),
        vec![edge(2, FingerprintKind::Semantic, &fps)],
    );
    {
        let mut reads = db.read_batch();
        assert!(Arc::ptr_eq(
            &reads.read(&key(3)).unwrap().unwrap(),
            &original
        ));
    }

    // A changed dependency set with an equal result preserves the consumer.
    publish(
        &mut db,
        2,
        fps.clone(),
        vec![edge(1, FingerprintKind::Semantic, &fps)],
    );
    let mut changed = fps.clone();
    changed.semantic = Fingerprint::of(b"new actual input");
    publish(&mut db, 0, changed.clone(), vec![]);
    {
        let mut reads = db.read_batch();
        assert!(Arc::ptr_eq(
            &reads.read(&key(3)).unwrap().unwrap(),
            &original
        ));
    }
    publish(&mut db, 1, changed.clone(), vec![]);
    {
        let mut reads = db.read_batch();
        assert!(reads.read(&key(3)).unwrap().is_none());
    }
    publish(
        &mut db,
        2,
        changed.clone(),
        vec![edge(1, FingerprintKind::Semantic, &changed)],
    );
    let current = publish(
        &mut db,
        3,
        changed.clone(),
        vec![edge(2, FingerprintKind::Semantic, &changed)],
    );
    let mut reads = db.read_batch();
    let recovered = reads.read(&key(3)).unwrap().unwrap();
    assert!(Arc::ptr_eq(&recovered, &current));
    assert!(!Arc::ptr_eq(&recovered, &original));
    assert_eq!(recovered.fingerprints.semantic, changed.semantic);
}

#[test]
fn batched_reads_preserve_active_cycle_missing_and_stale_closure_failures() {
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
    let unrelated = publish(&mut db, 3, fps.clone(), vec![]);
    db.begin(key(0)).unwrap();
    {
        let mut reads = db.read_batch();
        assert!(Arc::ptr_eq(
            &reads.read(&key(3)).unwrap().unwrap(),
            &unrelated
        ));
        let expected = db.read(&key(2)).unwrap_err();
        assert_eq!(reads.read(&key(2)).unwrap_err(), expected);
        assert_eq!(
            reads.read(&key(1)).unwrap_err(),
            db.read(&key(1)).unwrap_err()
        );
        assert_eq!(reads.stats().retained_validations, 1);
    }
    db.abort(&key(0));
    {
        let mut reads = db.read_batch();
        assert!(reads.read(&key(2)).unwrap().is_some());
    }

    // Forge malformed metadata inside the component test. A new view must
    // validate the full closure, even though an earlier view proved its owner.
    Arc::make_mut(db.snapshots.get_mut(&key(0)).unwrap()).dependencies =
        vec![edge(2, FingerprintKind::Semantic, &fps)];
    {
        let mut reads = db.read_batch();
        assert_eq!(
            reads.read(&key(2)).unwrap_err(),
            db.read(&key(2)).unwrap_err()
        );
    }
    Arc::make_mut(db.snapshots.get_mut(&key(0)).unwrap()).dependencies =
        vec![edge(7, FingerprintKind::Semantic, &fps)];
    {
        let mut reads = db.read_batch();
        assert!(reads.read(&key(2)).unwrap().is_none());
    }
    Arc::make_mut(db.snapshots.get_mut(&key(0)).unwrap()).dependencies = vec![];
    Arc::make_mut(db.snapshots.get_mut(&key(1)).unwrap()).dependencies[0].observed =
        Fingerprint::ZERO;
    let mut reads = db.read_batch();
    assert!(reads.read(&key(2)).unwrap().is_none());
    assert_eq!(reads.stats().retained_validations, 0);
}

#[test]
fn publication_batch_preserves_dependency_order_and_aborts_failed_replacements() {
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
    publish(&mut db, 3, fps.clone(), vec![]);
    publish(&mut db, 4, fps.clone(), vec![]);
    let original = publish(&mut db, 9, fps.clone(), vec![]);
    db.begin(key(4)).unwrap();
    let valid = edge(2, FingerprintKind::Semantic, &fps);
    let mut stale = edge(3, FingerprintKind::Semantic, &fps);
    stale.observed = Fingerprint::ZERO;
    let active = edge(4, FingerprintKind::Semantic, &fps);
    for (dependencies, code) in [
        (
            vec![valid.clone(), stale.clone(), active.clone()],
            "E-QUERY-STALE-INPUT",
        ),
        (vec![active, stale, valid], "E-QUERY-CYCLE"),
    ] {
        db.begin(key(9)).unwrap();
        let error = db
            .publish(
                key(9),
                ResultCategory::Success,
                fps.clone(),
                dependencies,
                vec![],
            )
            .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(db.active_count(), 1);
        assert!(Arc::ptr_eq(&db.read(&key(9)).unwrap().unwrap(), &original));
    }
    db.abort(&key(4));
    publish(
        &mut db,
        9,
        fps.clone(),
        vec![
            edge(2, FingerprintKind::Semantic, &fps),
            edge(1, FingerprintKind::Semantic, &fps),
        ],
    );
    assert_eq!(db.active_count(), 0);
}

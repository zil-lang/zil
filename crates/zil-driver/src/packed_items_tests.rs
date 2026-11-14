use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    directory: PathBuf,
    root: PathBuf,
    packs: PathBuf,
    plan: artifacts::ArtifactPlan,
    bounds: Bounds,
}
impl Fixture {
    fn new(maximum: usize) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "zil-packed-items-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("main.zil");
        let text = "pub fn main()->i32{return 1;}";
        fs::write(&path, text).unwrap();
        let source = SourceUnit::new(FileId(0), ModuleId(0), path.display().to_string(), text);
        let plan = artifacts::ArtifactPlan::new(&source, &CompileOptions::default()).unwrap();
        let root = directory.join("cache");
        let packs = root.join("packs-v1");
        assert!(private_directory(&root, true));
        assert!(private_directory(&packs, true));
        Self {
            directory,
            root,
            packs,
            plan,
            bounds: Bounds {
                maximum,
                max_items: 32,
            },
        }
    }
    fn publish(
        &self,
        id: u8,
        logical: &[u8],
        items: &[(Fingerprint, Arc<[u8]>)],
        epoch: &mut Epoch,
    ) -> PublicationStats {
        self.publish_with_hook(
            id,
            logical,
            items,
            epoch,
            &CancellationToken::default(),
            || {},
        )
        .unwrap()
    }
    fn publish_with_hook(
        &self,
        id: u8,
        logical: &[u8],
        items: &[(Fingerprint, Arc<[u8]>)],
        epoch: &mut Epoch,
        cancellation: &CancellationToken,
        before_manifest: impl FnOnce(),
    ) -> Option<PublicationStats> {
        let mut plan = self.plan.clone();
        let packs = plan.cache_directory("packs", &self.packs).ok()?;
        let key = Fingerprint::of(&[id]);
        plan.cache_destination("manifest", &path(&self.root, key))
            .ok()?;
        let pack_namespace = plan.pin_cache_directory(&packs).ok()?;
        super::publish_with_hook(
            PublishContext {
                packs: &pack_namespace,
                manifest_root: &self.root,
                namespace_key: &packs,
                manifest_key: key,
                cancellation,
                profile: true,
            },
            logical,
            items.to_vec(),
            self.bounds,
            epoch,
            before_manifest,
        )
    }
    fn manifest(&self, id: u8) -> Vec<u8> {
        let CacheRead::Candidate(bytes) =
            ArtifactCache::with_limit(&self.root, self.bounds.manifest_maximum().unwrap())
                .inspect_candidate(Fingerprint::of(&[id]))
                .unwrap()
        else {
            panic!("missing manifest")
        };
        bytes
    }
    fn reader(&self, id: u8) -> Reader {
        Reader::new(
            &self.packs,
            &self.manifest(id),
            self.bounds,
            &CancellationToken::default(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
fn item(bytes: impl AsRef<[u8]>) -> (Fingerprint, Arc<[u8]>) {
    let bytes = bytes.as_ref();
    (Fingerprint::of(bytes), bytes.into())
}
fn contents(reader: &mut Reader, expected: &[(Fingerprint, Arc<[u8]>)]) {
    for (key, bytes) in expected {
        assert_eq!(reader.load(*key).unwrap().as_ref(), bytes.as_ref());
    }
}

#[test]
fn cold_warm_dirty_and_historical_restore_use_actual_pack_write_counts() {
    let fixture = Fixture::new(512);
    let items: Vec<_> = (0..16)
        .map(|id| item(format!("raw-item-{id:02}")))
        .collect();
    let mut epoch = Epoch::default();
    let cold = fixture.publish(1, b"logical original manifest", &items, &mut epoch);
    assert_eq!(
        (
            cold.physical_pack_writes,
            cold.manifest_writes,
            cold.written_items
        ),
        (1, 1, 16)
    );
    assert_eq!(cold.referenced_packs, 1);
    let warm = fixture.publish(1, b"logical original manifest", &items, &mut epoch);
    assert_eq!(
        (
            warm.physical_pack_writes,
            warm.manifest_writes,
            warm.written_items
        ),
        (0, 1, 0)
    );
    assert_eq!(warm.existing_items, 16);
    let mut changed = items.clone();
    changed[0] = item(b"longer changed raw item");
    let dirty = fixture.publish(2, b"logical changed manifest", &changed, &mut epoch);
    assert_eq!(
        (
            dirty.physical_pack_writes,
            dirty.written_items,
            dirty.existing_items
        ),
        (1, 1, 15)
    );
    assert_eq!(dirty.compacted_items, 0);
    assert_eq!(dirty.referenced_packs, 2);

    // There is no memory index from the original publication. The old project
    // manifest independently names all old packs and selected raw item slices.
    let mut old = fixture.reader(1);
    assert_eq!(old.logical(), b"logical original manifest");
    contents(&mut old, &items);
    let mut restored = fixture.reader(2);
    contents(&mut restored, &changed);
    let mut recovered = restored.epoch();
    let again = fixture.publish(3, b"logical changed manifest", &changed, &mut recovered);
    assert_eq!(
        (
            again.physical_pack_writes,
            again.written_items,
            again.existing_items
        ),
        (0, 0, 16)
    );
}

#[test]
fn unique_pack_read_amplification_forces_bounded_current_byte_compaction() {
    let fixture = Fixture::new(100);
    let a = item([1; 10]);
    let b = item([2; 80]);
    let c = item([3; 10]);
    let d = item([4; 10]);
    let mut epoch = Epoch::default();
    fixture.publish(1, b"first", &[a.clone(), b.clone()], &mut epoch);
    let second = fixture.publish(2, b"second", &[a.clone(), c.clone()], &mut epoch);
    assert_eq!(second.referenced_pack_payload_bytes, 100);
    let third = fixture.publish(3, b"third", &[a.clone(), c.clone(), d.clone()], &mut epoch);
    assert_eq!(
        (
            third.physical_pack_writes,
            third.compacted_items,
            third.written_items
        ),
        (1, 2, 3)
    );
    assert_eq!(
        (third.referenced_packs, third.referenced_pack_payload_bytes),
        (1, 30)
    );
    contents(&mut fixture.reader(3), &[a.clone(), c, d]);
    contents(&mut fixture.reader(1), &[a, b]);
    assert_eq!(epoch.locations.len(), 3);
    assert_eq!(epoch.packs.len(), 1);
}

#[test]
fn malformed_transport_bounds_are_checked_before_pack_reads() {
    let fixture = Fixture::new(100);
    let a = Fingerprint::of(b"a");
    let b = Fingerprint::of(b"b");
    let pack_a = Fingerprint::of(b"pack-a");
    let pack_b = Fingerprint::of(b"pack-b");
    let good = Manifest {
        logical: b"logical".to_vec(),
        locations: BTreeMap::from([(
            a,
            Location {
                pack: pack_a,
                offset: 0,
                length: 1,
            },
        )]),
        packs: BTreeMap::from([(pack_a, PACK_HEADER + 90)]),
    };
    let bytes = good.encode(fixture.bounds).unwrap();
    assert!(Manifest::decode(&bytes, fixture.bounds).is_some());
    let amplified = Manifest {
        logical: b"logical".to_vec(),
        locations: BTreeMap::from([
            (
                a,
                Location {
                    pack: pack_a,
                    offset: 0,
                    length: 1,
                },
            ),
            (
                b,
                Location {
                    pack: pack_b,
                    offset: 0,
                    length: 1,
                },
            ),
        ]),
        packs: BTreeMap::from([(pack_a, PACK_HEADER + 90), (pack_b, PACK_HEADER + 90)]),
    };
    assert!(amplified.encode(fixture.bounds).is_none());
    for (offset, replacement) in [
        (24, u32::MAX.to_le_bytes().to_vec()),
        (
            MANIFEST_HEADER + PACK_ROW + 64,
            u64::MAX.to_le_bytes().to_vec(),
        ),
    ] {
        let mut forged = bytes.clone();
        forged[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            Reader::new(
                &fixture.packs,
                &forged,
                fixture.bounds,
                &CancellationToken::default()
            )
            .is_none()
        );
    }
    let mut short = fixture.bounds;
    short.maximum = 89;
    assert!(Manifest::decode(&bytes, short).is_none());
    assert_eq!(fs::read_dir(&fixture.packs).unwrap().count(), 0);
}

#[test]
fn exact_payload_quota_and_compaction_keep_the_selected_raw_byte_limit() {
    let fixture = Fixture::new(100);
    let a = item([1; 60]);
    let b = item([2; 40]);
    let changed = item([3; 60]);
    let mut epoch = Epoch::default();
    let cold = fixture.publish(1, b"N", &[a, b.clone()], &mut epoch);
    assert_eq!(cold.referenced_pack_payload_bytes, 100);
    let compacted = fixture.publish(2, b"N", &[changed.clone(), b.clone()], &mut epoch);
    assert_eq!(
        (compacted.physical_pack_writes, compacted.compacted_items),
        (1, 1)
    );
    assert_eq!(compacted.referenced_pack_payload_bytes, 100);
    contents(&mut fixture.reader(2), &[changed.clone(), b.clone()]);
    let short = Fixture::new(99);
    assert!(
        short
            .publish_with_hook(
                1,
                b"N-1",
                &[changed, b],
                &mut Epoch::default(),
                &CancellationToken::default(),
                || {}
            )
            .is_none()
    );
    assert_eq!(fs::read_dir(&short.packs).unwrap().count(), 0);
}

#[test]
fn valid_pack_integrity_does_not_replace_selected_item_integrity() {
    let fixture = Fixture::new(100);
    let a = item([1; 10]);
    let b = item([2; 10]);
    let mut epoch = Epoch::default();
    fixture.publish(1, b"both", &[a.clone(), b], &mut epoch);
    fixture.publish(2, b"selected", std::slice::from_ref(&a), &mut epoch);
    let old = epoch.locations[&a.0];
    let shifted = if old.offset == 0 { 1 } else { old.offset - 1 };
    let mut manifest = fixture.manifest(2);
    let at = MANIFEST_HEADER + PACK_ROW + 64;
    manifest[at..at + 8].copy_from_slice(&u64::try_from(shifted).unwrap().to_le_bytes());
    ArtifactCache::with_limit(&fixture.root, fixture.bounds.manifest_maximum().unwrap())
        .write_candidate(Fingerprint::of(&[2]), &manifest)
        .unwrap();
    assert!(fixture.reader(2).load(a.0).is_none());
}

#[test]
fn corruption_outside_a_selected_slice_invalidates_the_whole_pack_then_repairs() {
    let fixture = Fixture::new(100);
    let a = item([1; 10]);
    let b = item([2; 10]);
    let mut epoch = Epoch::default();
    fixture.publish(1, b"both", &[a.clone(), b.clone()], &mut epoch);
    fixture.publish(2, b"only a", std::slice::from_ref(&a), &mut epoch);
    let pack = *epoch.packs.keys().next().unwrap();
    let cache = ArtifactCache::with_limit(&fixture.packs, fixture.bounds.pack_maximum().unwrap());
    let CacheRead::Candidate(mut bytes) = cache.inspect_candidate(pack).unwrap() else {
        panic!()
    };
    // Preserve the outer ArtifactCache checksum but corrupt the unselected item.
    let selected = epoch.locations[&a.0];
    let position = (PACK_HEADER..bytes.len())
        .find(|position| {
            *position < PACK_HEADER + selected.offset
                || *position >= PACK_HEADER + selected.offset + selected.length
        })
        .unwrap();
    bytes[position] ^= 1;
    cache.write_candidate(pack, &bytes).unwrap();
    assert!(fixture.reader(2).load(a.0).is_none());
    let repaired = fixture.publish(2, b"only a", std::slice::from_ref(&a), &mut epoch);
    assert_eq!(
        (repaired.physical_pack_writes, repaired.written_items),
        (1, 1)
    );
    contents(&mut fixture.reader(2), std::slice::from_ref(&a));
}

#[test]
fn failed_final_publication_preserves_the_old_manifest_and_remembered_epoch() {
    let fixture = Fixture::new(100);
    let original = item(b"original");
    let replacement = item(b"replacement");
    let mut epoch = Epoch::default();
    fixture.publish(
        1,
        b"original manifest",
        std::slice::from_ref(&original),
        &mut epoch,
    );
    let before = fixture.manifest(1);
    let previous_locations = epoch.locations.clone();
    let cancellation = CancellationToken::default();
    assert!(
        fixture
            .publish_with_hook(
                1,
                b"new manifest",
                std::slice::from_ref(&replacement),
                &mut epoch,
                &cancellation,
                || cancellation.cancel()
            )
            .is_none()
    );
    assert_eq!(fixture.manifest(1), before);
    assert_eq!(epoch.locations, previous_locations);
    contents(&mut fixture.reader(1), std::slice::from_ref(&original));
    let retried = fixture.publish(
        1,
        b"new manifest",
        std::slice::from_ref(&replacement),
        &mut epoch,
    );
    assert_eq!(
        (retried.physical_pack_writes, retried.manifest_writes),
        (0, 1)
    );
    contents(&mut fixture.reader(1), std::slice::from_ref(&replacement));
}

#[test]
#[cfg(target_os = "linux")]
fn namespace_replacement_after_pack_sync_cannot_commit_a_manifest() {
    let fixture = Fixture::new(100);
    let original = item(b"original");
    let replacement = item(b"replacement");
    let mut epoch = Epoch::default();
    fixture.publish(1, b"old", std::slice::from_ref(&original), &mut epoch);
    let before = fixture.manifest(1);
    let held = fixture.directory.join("held-packs");
    let token = CancellationToken::default();
    assert!(
        fixture
            .publish_with_hook(
                1,
                b"new",
                std::slice::from_ref(&replacement),
                &mut epoch,
                &token,
                || {
                    fs::rename(&fixture.packs, &held).unwrap();
                    fs::create_dir(&fixture.packs).unwrap();
                }
            )
            .is_none()
    );
    assert_eq!(fixture.manifest(1), before);
    assert_eq!(fs::read_dir(&fixture.packs).unwrap().count(), 0);
}

fn shared_fixture() -> (Fixture, Vec<(Fingerprint, Arc<[u8]>)>, Vec<u8>) {
    let mut fixture = Fixture::new(128 * 1024);
    fixture.bounds.max_items = 1024;
    let items = (0u32..1024).map(|id| item(id.to_le_bytes())).collect();
    // Varied bindings, rather than all-zero data, exercise real suffix sharing.
    let mut state = 17u64;
    let mut logical = Vec::new();
    for _ in 0..4096 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        logical.extend_from_slice(&state.to_le_bytes());
    }
    (fixture, items, logical)
}

fn cache_bytes(root: &Path) -> u64 {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                cache_bytes(&entry.path())
            } else {
                entry.metadata().unwrap().len()
            }
        })
        .sum()
}

#[test]
fn shared_manifest_32_edits_keep_all_bytes_and_restart_restore_without_history_index() {
    let (fixture, mut items, mut logical) = shared_fixture();
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    assert_eq!(fixture.manifest(0).get(..8), Some(SHARED_MAGIC.as_slice()));
    let cold_size = cache_bytes(&fixture.root);
    let original = (items.clone(), logical.clone());
    let mut full_manifest_bytes = 0;
    for id in 1u8..=32 {
        items[511] = item([id; 5]);
        logical[4000] = id;
        fixture.publish(id, &logical, &items, &mut epoch);
        let mut restarted = fixture.reader(id);
        assert_eq!(restarted.logical(), logical);
        contents(&mut restarted, &items);
        let decoded = read_manifest(
            &fixture.packs,
            &fixture.manifest(id),
            fixture.bounds,
            &CancellationToken::default(),
        )
        .unwrap();
        full_manifest_bytes += decoded.encode(fixture.bounds).unwrap().len() as u64;
        // Drop all placement hints at every edit: only current restored data
        // can seed the next publication; no extra in-memory retention policy.
        epoch = restarted.epoch();
    }
    let growth = cache_bytes(&fixture.root) - cold_size;
    assert!(
        growth < full_manifest_bytes / 3,
        "actual retained bytes {growth} must share repeated tables from {full_manifest_bytes} full bytes"
    );
    drop(epoch);
    for id in 0u8..=32 {
        let mut old = fixture.reader(id);
        let mut expected_items = original.0.clone();
        let mut expected_logical = original.1.clone();
        if id != 0 {
            expected_items[511] = item([id; 5]);
            expected_logical[4000] = id;
        }
        assert_eq!(old.logical(), expected_logical);
        contents(&mut old, &expected_items);
    }
    let mut restored = Epoch::hint(
        &fixture.packs,
        &fixture.manifest(0),
        fixture.bounds,
        &CancellationToken::default(),
    )
    .unwrap();
    let warm = fixture.publish(0, &original.1, &original.0, &mut restored);
    assert_eq!(warm.physical_pack_writes, 0);
    assert_eq!(warm.manifest_bytes, fixture.manifest(0).len());
    // The old inline envelope stays readable at the same source identity;
    // publishing it again may choose sharing without changing any raw bytes.
    let inline = read_manifest(
        &fixture.packs,
        &fixture.manifest(0),
        fixture.bounds,
        &CancellationToken::default(),
    )
    .unwrap()
    .encode(fixture.bounds)
    .unwrap();
    ArtifactCache::with_limit(&fixture.root, fixture.bounds.manifest_maximum().unwrap())
        .write_candidate(Fingerprint::of(&[0]), &inline)
        .unwrap();
    contents(&mut fixture.reader(0), &original.0);
    let migrated = fixture.publish(0, &original.1, &original.0, &mut restored);
    assert_eq!(migrated.physical_pack_writes, 0);
    assert_eq!(migrated.manifest_bytes, fixture.manifest(0).len());
}

#[test]
fn shared_manifest_missing_corrupt_and_wrong_content_chunks_are_misses_then_repaired() {
    let (fixture, items, logical) = shared_fixture();
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    let descriptor = fixture.manifest(0);
    let key = Fingerprint(
        descriptor[SHARED_HEADER..SHARED_HEADER + 32]
            .try_into()
            .unwrap(),
    );
    let chunk = path(&fixture.packs, key);
    let raw = fs::read(&chunk).unwrap();
    let token = CancellationToken::default();
    fs::remove_file(&chunk).unwrap();
    assert!(Reader::new(&fixture.packs, &descriptor, fixture.bounds, &token).is_none());
    assert!(Epoch::hint(&fixture.packs, &descriptor, fixture.bounds, &token).is_none());
    fixture.publish(0, &logical, &items, &mut epoch);
    assert_eq!(fs::read(&chunk).unwrap(), raw);
    let mut corrupted = raw.clone();
    *corrupted.last_mut().unwrap() ^= 1;
    fs::write(&chunk, corrupted).unwrap();
    assert!(Reader::new(&fixture.packs, &descriptor, fixture.bounds, &token).is_none());
    fixture.publish(0, &logical, &items, &mut epoch);
    // Recompute ArtifactCache's checksum under the old name: the content CAS
    // key, not merely the valid raw wrapper, must still reject this substitution.
    let cache = ArtifactCache::with_limit(&fixture.packs, CHUNK_MAX);
    let CacheRead::Candidate(mut bytes) = cache.inspect_candidate(key).unwrap() else {
        panic!()
    };
    bytes[0] ^= 1;
    cache.write_candidate(key, &bytes).unwrap();
    assert!(Reader::new(&fixture.packs, &descriptor, fixture.bounds, &token).is_none());
    fixture.publish(0, &logical, &items, &mut epoch);
    contents(&mut fixture.reader(0), &items);
    assert_eq!(fs::read(&chunk).unwrap(), raw);
    #[cfg(unix)]
    {
        let unrelated = fixture.directory.join("unrelated");
        fs::write(&unrelated, &raw).unwrap();
        fs::remove_file(&chunk).unwrap();
        std::os::unix::fs::symlink(&unrelated, &chunk).unwrap();
        assert!(Reader::new(&fixture.packs, &descriptor, fixture.bounds, &token).is_none());
        fixture.publish(0, &logical, &items, &mut epoch);
        assert!(
            !fs::symlink_metadata(&chunk)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&unrelated).unwrap(), raw);
        contents(&mut fixture.reader(0), &items);
    }
}

#[test]
fn shared_manifest_valid_chunks_do_not_authorize_changed_digest_order_or_quotas() {
    let (fixture, items, logical) = shared_fixture();
    fixture.publish(0, &logical, &items, &mut Epoch::default());
    let descriptor = fixture.manifest(0);
    let token = CancellationToken::default();
    let reject = |bytes: &[u8], bounds| {
        assert!(Reader::new(&fixture.packs, bytes, bounds, &token).is_none());
    };
    for (offset, value) in [
        (8, u64::MAX.to_le_bytes().to_vec()),
        (48, u32::MAX.to_le_bytes().to_vec()),
        (SHARED_HEADER + 32, 0u64.to_le_bytes().to_vec()),
    ] {
        let mut changed = descriptor.clone();
        changed[offset..offset + value.len()].copy_from_slice(&value);
        reject(&changed, fixture.bounds);
    }
    let mut changed = descriptor.clone();
    changed[16] ^= 1;
    reject(&changed, fixture.bounds);
    let mut changed = descriptor.clone();
    let first = descriptor[SHARED_HEADER..SHARED_HEADER + PACK_ROW].to_vec();
    changed[SHARED_HEADER..SHARED_HEADER + PACK_ROW]
        .copy_from_slice(&descriptor[SHARED_HEADER + PACK_ROW..SHARED_HEADER + 2 * PACK_ROW]);
    changed[SHARED_HEADER + PACK_ROW..SHARED_HEADER + 2 * PACK_ROW].copy_from_slice(&first);
    reject(&changed, fixture.bounds);
    reject(&descriptor[..descriptor.len() - 1], fixture.bounds);
    // A transport quota satisfied by the small descriptor does not admit the
    // reconstructed logical bytes, original item count or body payload.
    reject(
        &descriptor,
        Bounds {
            maximum: logical.len() - 1,
            ..fixture.bounds
        },
    );
    reject(
        &descriptor,
        Bounds {
            max_items: items.len() - 1,
            ..fixture.bounds
        },
    );
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(Reader::new(&fixture.packs, &descriptor, fixture.bounds, &cancelled).is_none());
}

#[test]
fn cancelled_shared_publication_keeps_old_root_epoch_and_all_historical_bytes() {
    let (fixture, mut items, logical) = shared_fixture();
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    let old = fixture.manifest(0);
    let locations = epoch.locations.clone();
    let original = items.clone();
    items[511] = item(b"changed private body");
    let token = CancellationToken::default();
    assert!(
        fixture
            .publish_with_hook(0, &logical, &items, &mut epoch, &token, || token.cancel())
            .is_none()
    );
    assert_eq!(fixture.manifest(0), old);
    assert_eq!(epoch.locations, locations);
    contents(&mut fixture.reader(0), &original);
    let retry = fixture.publish(0, &logical, &items, &mut epoch);
    assert_eq!(retry.physical_pack_writes, 0);
    assert_eq!(retry.manifest_bytes, fixture.manifest(0).len());
    contents(&mut fixture.reader(0), &items);
}

#[test]
#[cfg(target_os = "linux")]
fn shared_namespace_replacement_cannot_publish_a_new_root_or_epoch() {
    let (fixture, mut items, logical) = shared_fixture();
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    let old = fixture.manifest(0);
    let locations = epoch.locations.clone();
    items[511] = item(b"new private body");
    let held = fixture.directory.join("held-shared-packs");
    assert!(
        fixture
            .publish_with_hook(
                0,
                &logical,
                &items,
                &mut epoch,
                &CancellationToken::default(),
                || {
                    fs::rename(&fixture.packs, &held).unwrap();
                    fs::create_dir(&fixture.packs).unwrap();
                }
            )
            .is_none()
    );
    assert_eq!(fixture.manifest(0), old);
    assert_eq!(epoch.locations, locations);
    assert_eq!(fs::read_dir(&fixture.packs).unwrap().count(), 0);
    fs::remove_dir(&fixture.packs).unwrap();
    fs::rename(&held, &fixture.packs).unwrap();
    assert!(
        fixture
            .publish(0, &logical, &items, &mut epoch)
            .manifest_writes
            == 1
    );
    contents(&mut fixture.reader(0), &items);
}

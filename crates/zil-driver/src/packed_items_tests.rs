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
        shared: None,
        recipes: BTreeMap::new(),
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
        shared: None,
        recipes: BTreeMap::new(),
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

type SharedFixture = (Fixture, Vec<(Fingerprint, Arc<[u8]>)>, Vec<u8>);

fn shared_fixture() -> SharedFixture {
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
    let index_end = SHARED_HEADER + count(&descriptor, 48).unwrap() * PACK_ROW;
    let chunk_count = count(&descriptor, 48).unwrap();
    let meta_bounds =
        metadata_bounds(fixture.bounds, number(&descriptor, 8).unwrap(), chunk_count).unwrap();
    let table = Manifest::decode(&descriptor[index_end..], meta_bounds).unwrap();
    let key = *table.packs.keys().next().unwrap();
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
    let cache = ArtifactCache::with_limit(&fixture.packs, meta_bounds.pack_maximum().unwrap());
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
    let shared_locations = epoch.shared.as_ref().unwrap().locations.clone();
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
    assert_eq!(epoch.shared.as_ref().unwrap().locations, shared_locations);
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

#[test]
fn shared_locator_chunks_use_one_physical_pack_and_checked_offsets() {
    let (fixture, items, logical) = shared_fixture();
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    let shared = epoch.shared.as_ref().unwrap();
    assert!(shared.locations.len() > 1);
    assert_eq!(shared.packs.len(), 1);
    // Real filesystem outputs: one body pack and one metadata pack. Hundreds
    // of logical records do not create hundreds of sync/rename transactions.
    assert_eq!(fs::read_dir(&fixture.packs).unwrap().count(), 2);
    let descriptor = fixture.manifest(0);
    let index_end = SHARED_HEADER + count(&descriptor, 48).unwrap() * PACK_ROW;
    let meta_bounds = metadata_bounds(
        fixture.bounds,
        number(&descriptor, 8).unwrap(),
        count(&descriptor, 48).unwrap(),
    )
    .unwrap();
    let mut locator = Manifest::decode(&descriptor[index_end..], meta_bounds).unwrap();
    let first = *locator.locations.keys().next().unwrap();
    locator.locations.get_mut(&first).unwrap().offset = usize::MAX;
    assert!(locator.encode(meta_bounds).is_none());
    let mut restored = fixture.reader(0);
    assert_eq!(restored.logical(), logical);
    contents(&mut restored, &items);
    let rebound = restored.epoch();
    assert_eq!(rebound.shared.as_ref().unwrap().locations, shared.locations);
}

#[test]
fn repeated_logical_reads_share_current_checked_bytes_and_cancel_still_wins() {
    let fixture = Fixture::new(8192);
    let bytes: Arc<[u8]> = vec![29; 2049].into();
    let key = Fingerprint::of(&bytes);
    let mut epoch = Epoch::default();
    fixture.publish(
        91,
        b"current logical rows",
        &[(key, bytes.clone())],
        &mut epoch,
    );
    let mut reader = fixture.reader(91);
    let first = reader.load(key).unwrap();
    let second = reader.load(key).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(first.as_ref(), bytes.as_ref());
    assert_eq!(reader.requested.len(), 1);
    // Already admitted immutable bytes remain only a raw-data snapshot, not a
    // way around cancellation or a second reader's actual file validation.
    reader.cancellation.cancel();
    assert!(reader.load(key).is_none());
    let file = epoch.first_pack_path().unwrap();
    fs::write(file, b"corrupt after the current reader").unwrap();
    assert!(fixture.reader(91).load(key).is_none());
}

#[test]
fn stamped_publication_still_rejects_wrong_current_bytes_for_the_same_key() {
    let fixture = Fixture::new(512);
    let original = item(b"original");
    let mut epoch = Epoch::default();
    fixture.publish(1, b"logical", std::slice::from_ref(&original), &mut epoch);
    let manifest = fixture.manifest(1);
    let locations = epoch.locations.clone();
    let wrong: Arc<[u8]> = Arc::from(&b"modified"[..]);
    // Same key, length and unchanged physical stamp must not bypass equality.
    assert!(
        fixture
            .publish_with_hook(
                1,
                b"replacement",
                &[(original.0, wrong)],
                &mut epoch,
                &CancellationToken::default(),
                || {},
            )
            .is_none()
    );
    assert_eq!(fixture.manifest(1), manifest);
    assert_eq!(epoch.locations, locations);
    contents(&mut fixture.reader(1), &[original]);
}

#[test]
fn restored_hint_shares_one_pack_snapshot_and_rechecks_changed_files() {
    let fixture = Fixture::new(512);
    let items = vec![item(b"first body"), item(b"second body")];
    let mut epoch = Epoch::default();
    fixture.publish(1, b"logical", &items, &mut epoch);
    let mut reader = fixture.reader(1);
    contents(&mut reader, &items);
    let mut hint = reader.hint_epoch();
    let key = *hint.packs.keys().next().unwrap();
    let snapshot = reader.loaded[&key].0.clone();
    assert!(Arc::ptr_eq(
        hint.packs[&key].bytes.as_ref().unwrap(),
        &snapshot
    ));
    let warm = fixture.publish(2, b"logical", &items, &mut hint);
    assert_eq!(warm.physical_pack_writes, 0);
    assert!(Arc::ptr_eq(
        hint.packs[&key].bytes.as_ref().unwrap(),
        &snapshot
    ));
    // Retaining the raw bytes cannot mask a missing or changed cache file.
    fs::write(path(&fixture.packs, key), b"corrupt").unwrap();
    let repaired = fixture.publish(3, b"logical", &items, &mut hint);
    assert_eq!(repaired.physical_pack_writes, 1);
    assert!(!Arc::ptr_eq(
        hint.packs[&key].bytes.as_ref().unwrap(),
        &snapshot
    ));
    contents(&mut fixture.reader(3), &items);
    contents(&mut fixture.reader(1), &items);
}

#[test]
fn direct_manifest_encoding_preserves_decoder_bounds_and_exact_bytes() {
    let fixture = Fixture::new(512);
    let items = vec![item(b"first body"), item(b"second body")];
    let mut epoch = Epoch::default();
    fixture.publish(1, b"logical", &items, &mut epoch);
    let original = fixture.manifest(1);
    let mut manifest = Manifest::decode(&original, fixture.bounds).unwrap();
    assert_eq!(manifest.encode(fixture.bounds).unwrap(), original);
    let first = items[0].0;
    let saved = manifest.locations[&first];
    manifest.locations.get_mut(&first).unwrap().length = 0;
    assert!(manifest.encode(fixture.bounds).is_none());
    manifest.locations.insert(
        first,
        Location {
            offset: usize::MAX,
            ..saved
        },
    );
    assert!(manifest.encode(fixture.bounds).is_none());
    manifest.locations.insert(first, saved);
    let pack_len = manifest.packs[&saved.pack];
    manifest.packs.insert(saved.pack, PACK_HEADER);
    assert!(manifest.encode(fixture.bounds).is_none());
    manifest.packs.insert(saved.pack, pack_len);
    manifest
        .packs
        .insert(Fingerprint::of(b"unreferenced pack"), PACK_HEADER + 1);
    assert!(manifest.encode(fixture.bounds).is_none());
    manifest
        .packs
        .remove(&Fingerprint::of(b"unreferenced pack"));
    assert!(
        manifest
            .encode(Bounds {
                maximum: 1,
                ..fixture.bounds
            })
            .is_none()
    );
    assert!(
        manifest
            .encode(Bounds {
                max_items: 1,
                ..fixture.bounds
            })
            .is_none()
    );
    assert_eq!(manifest.encode(fixture.bounds).unwrap(), original);
}

fn scalar_segment(id: u32, name: &str, body: &[u8]) -> (Fingerprint, Arc<[u8]>) {
    let mut bytes = vec![(id % 2) as u8, 0, 0, 0, 0, 0];
    bytes.extend_from_slice(&id.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(name.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(body);
    item(bytes)
}

fn dictionary_fixture(functions: usize) -> SharedFixture {
    let mut items: Vec<_> = (0..functions)
        .map(|id| {
            scalar_segment(
                id as u32,
                &format!("label_{id}_λ"),
                &[71 + (id % 4) as u8; 4096],
            )
        })
        .collect();
    items.push(item(b"opaque plan segment: all original bytes"));
    items.push(item(b"opaque metadata segment: all original bytes"));
    let mut fixture =
        Fixture::new(items.iter().map(|(_, bytes)| bytes.len()).sum::<usize>() + 8192);
    fixture.bounds.max_items = items.len();
    (
        fixture,
        items,
        b"original logical rows and bindings".to_vec(),
    )
}

#[test]
fn scalar_dictionary_shares_actual_disk_bodies_and_republishes_original_bytes() {
    let (fixture, mut items, logical) = dictionary_fixture(96);
    let original = items.clone();
    let logical_bytes: usize = items.iter().map(|(_, bytes)| bytes.len()).sum();
    let mut epoch = Epoch::default();
    let cold = fixture.publish(0, &logical, &items, &mut epoch);
    assert_eq!(
        fixture.manifest(0).get(..8),
        Some(dictionary::MAGIC.as_slice())
    );
    assert_eq!(cold.physical_pack_writes, 1);
    assert_eq!((cold.written_items, cold.existing_items), (items.len(), 0));
    assert_eq!(epoch.locations.len(), 6);
    let physical = 4 * 4096 + items[96].1.len() + items[97].1.len();
    assert_eq!(cold.referenced_pack_payload_bytes, physical);
    assert!(cache_bytes(&fixture.root) < logical_bytes as u64 / 4);
    let original_locations = epoch.locations.clone();
    drop(epoch);

    // Restore only from files, and feed exactly Reader.load's original keys
    // and full bytes back through the production publisher.
    let mut reader = fixture.reader(0);
    assert_eq!(reader.take_logical(), logical);
    let restored: Vec<_> = items
        .iter()
        .map(|(key, _)| (*key, reader.load(*key).unwrap()))
        .collect();
    assert_eq!(restored, items);
    assert!(Arc::ptr_eq(
        &reader.load(items[0].0).unwrap(),
        &restored[0].1
    ));
    assert!(reader.load(Fingerprint::of(&[71; 4096])).is_none());
    let mut epoch = reader.epoch();
    assert_eq!(epoch.locations, original_locations);
    drop(reader);
    let warm = fixture.publish(1, &logical, &restored, &mut epoch);
    assert_eq!((warm.physical_pack_writes, warm.written_bytes), (0, 0));
    assert_eq!(warm.existing_items, items.len());

    // Different outer ID, UTF-8 label and length must not rewrite any body.
    items[0] = scalar_segment(1007, "renamed_longer_λ", &[71; 4096]);
    let renamed = fixture.publish(2, &logical, &items, &mut epoch);
    assert_eq!(
        (renamed.physical_pack_writes, renamed.written_bytes),
        (0, 0)
    );
    assert_eq!(epoch.locations, original_locations);
    contents(&mut fixture.reader(2), &items);

    // A new body adds only its bytes. Every still-current locator is reused.
    items[1] = scalar_segment(1, "changed", &[93; 4096]);
    let edited = fixture.publish(3, &logical, &items, &mut epoch);
    assert_eq!(
        (edited.physical_pack_writes, edited.written_bytes),
        (1, 4096)
    );
    for (key, location) in original_locations {
        assert_eq!(epoch.locations[&key], location);
    }
    contents(&mut fixture.reader(3), &items);
    contents(&mut fixture.reader(0), &original);
    let mut hint = Epoch::hint(
        &fixture.packs,
        &fixture.manifest(3),
        fixture.bounds,
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(
        fixture
            .publish(4, &logical, &items, &mut hint)
            .physical_pack_writes,
        0
    );
}

#[test]
fn scalar_dictionary_keeps_original_aggregate_and_item_quotas_before_io() {
    let (mut fixture, items, logical) = dictionary_fixture(32);
    let total: usize = items.iter().map(|(_, bytes)| bytes.len()).sum();
    fixture.bounds.maximum = total - 1;
    assert!(
        fixture
            .publish_with_hook(
                0,
                &logical,
                &items,
                &mut Epoch::default(),
                &CancellationToken::default(),
                || {}
            )
            .is_none()
    );
    assert_eq!(fs::read_dir(&fixture.packs).unwrap().count(), 0);
    fixture.bounds.maximum = total;
    fixture.bounds.max_items = items.len() - 1;
    assert!(
        fixture
            .publish_with_hook(
                0,
                &logical,
                &items,
                &mut Epoch::default(),
                &CancellationToken::default(),
                || {}
            )
            .is_none()
    );
    assert_eq!(fs::read_dir(&fixture.packs).unwrap().count(), 0);
    fixture.bounds.max_items = items.len();
    fixture.publish(0, &logical, &items, &mut Epoch::default());
    let descriptor = fixture.manifest(0);
    let token = CancellationToken::default();
    assert!(
        Reader::new(
            &fixture.packs,
            &descriptor,
            Bounds {
                maximum: total - 1,
                ..fixture.bounds
            },
            &token
        )
        .is_none()
    );
    assert!(
        Reader::new(
            &fixture.packs,
            &descriptor,
            Bounds {
                max_items: items.len() - 1,
                ..fixture.bounds
            },
            &token
        )
        .is_none()
    );
    contents(&mut fixture.reader(0), &items);
}

#[test]
fn scalar_dictionary_unknown_headers_plans_and_metadata_remain_complete() {
    let (mut fixture, mut items, logical) = dictionary_fixture(16);
    let valid = scalar_segment(1, "name", b"body").1.to_vec();
    let mut unknown = valid.clone();
    unknown[1] = 1;
    let mut bad_length = valid.clone();
    bad_length[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut bad_utf8 = valid.clone();
    bad_utf8[14] = 255;
    let unsplit = vec![
        item(unknown),
        item(bad_length),
        item(bad_utf8),
        item(&valid[..9]),
        scalar_segment(7, "empty_body", &[]),
    ];
    items.extend(unsplit.clone());
    fixture.bounds.max_items = items.len();
    fixture.publish(0, &logical, &items, &mut Epoch::default());
    let manifest = dictionary::decode(&fixture.manifest(0), fixture.bounds).unwrap();
    for (key, _) in unsplit.iter().chain(items[16..18].iter()) {
        assert!(manifest.recipes[key].prefix.is_empty());
        assert_eq!(manifest.recipes[key].body, *key);
    }
    contents(&mut fixture.reader(0), &items);
}

#[test]
fn scalar_dictionary_checks_original_hash_prefix_lengths_tables_and_no_recursion() {
    let (fixture, items, logical) = dictionary_fixture(16);
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    let locations = epoch.locations.clone();
    let disk_bytes = cache_bytes(&fixture.root);
    let descriptor = fixture.manifest(0);
    let token = CancellationToken::default();
    let reject = |bytes: &[u8]| {
        assert!(Reader::new(&fixture.packs, bytes, fixture.bounds, &token).is_none())
    };
    for (offset, value) in [
        (8, u64::MAX.to_le_bytes().to_vec()),
        (16, u32::MAX.to_le_bytes().to_vec()),
        (20, u32::MAX.to_le_bytes().to_vec()),
        (dictionary::HEADER + 64, u64::MAX.to_le_bytes().to_vec()),
    ] {
        let mut changed = descriptor.clone();
        changed[offset..offset + value.len()].copy_from_slice(&value);
        reject(&changed);
    }
    reject(&descriptor[..descriptor.len() - 1]);
    let inner_start = dictionary::HEADER + count(&descriptor, 20).unwrap();
    for magic in [dictionary::MAGIC, SHARED_MAGIC] {
        let mut changed = descriptor.clone();
        changed[inner_start..inner_start + 8].copy_from_slice(magic);
        reject(&changed);
    }
    let mut manifest = dictionary::decode(&descriptor, fixture.bounds).unwrap();
    let key = items[0].0;
    // Valid UTF-8 header and valid body hash still cannot substitute a different
    // full item under the original key.
    manifest.recipes.get_mut(&key).unwrap().prefix[6] ^= 1;
    let forged = manifest.encode(fixture.bounds).unwrap();
    assert!(
        Reader::new(&fixture.packs, &forged, fixture.bounds, &token)
            .unwrap()
            .load(key)
            .is_none()
    );
    manifest.recipes.get_mut(&key).unwrap().prefix[6] ^= 1;
    let other_body = manifest.recipes[&items[1].0].body;
    manifest.recipes.get_mut(&key).unwrap().body = other_body;
    let forged = manifest.encode(fixture.bounds).unwrap();
    assert!(
        Reader::new(&fixture.packs, &forged, fixture.bounds, &token)
            .unwrap()
            .load(key)
            .is_none()
    );
    let mut wrong_key = items.clone();
    wrong_key[0].1 = items[1].1.clone();
    assert!(
        fixture
            .publish_with_hook(0, &logical, &wrong_key, &mut epoch, &token, || {})
            .is_none()
    );
    assert_eq!(fixture.manifest(0), descriptor);
    assert_eq!(epoch.locations, locations);
    assert_eq!(cache_bytes(&fixture.root), disk_bytes);
}

#[test]
fn scalar_dictionary_corruption_truncation_and_missing_packs_are_repaired() {
    let (fixture, items, logical) = dictionary_fixture(16);
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    let file = epoch.first_pack_path().unwrap();
    let raw = fs::read(&file).unwrap();
    for kind in 0..3 {
        match kind {
            0 => {
                fs::remove_file(&file).unwrap();
            }
            1 => {
                fs::write(&file, &raw[..raw.len() - 1]).unwrap();
            }
            _ => {
                let mut corrupt = raw.clone();
                *corrupt.last_mut().unwrap() ^= 1;
                fs::write(&file, corrupt).unwrap();
            }
        }
        assert!(fixture.reader(0).load(items[0].0).is_none());
        assert_eq!(
            fixture
                .publish(0, &logical, &items, &mut epoch)
                .physical_pack_writes,
            1
        );
        assert_eq!(fs::read(&file).unwrap(), raw);
        contents(&mut fixture.reader(0), &items);
    }
    let mut reader = fixture.reader(0);
    reader.load(items[0].0).unwrap();
    reader.cancellation.cancel();
    assert!(reader.load(items[0].0).is_none());
}

#[test]
fn scalar_dictionary_shared_metadata_history_and_cancel_keep_current_locators() {
    let (fixture, mut items, logical) = dictionary_fixture(256);
    let original = items.clone();
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    assert_eq!(fixture.manifest(0).get(..8), Some(SHARED_MAGIC.as_slice()));
    let cold = cache_bytes(&fixture.root);
    let mut full_sizes = 0u64;
    for edit in 1..=12u8 {
        items[0] = scalar_segment(
            u32::from(edit) + 800,
            &format!("new_label_{edit}"),
            &[71; 4096],
        );
        let previous = epoch.locations.clone();
        assert_eq!(
            fixture
                .publish(edit, &logical, &items, &mut epoch)
                .physical_pack_writes,
            0
        );
        assert_eq!(epoch.locations, previous);
        let mut reader = fixture.reader(edit);
        contents(&mut reader, &items);
        full_sizes += reader.manifest.encode(fixture.bounds).unwrap().len() as u64;
        epoch = reader.hint_epoch();
    }
    assert!(cache_bytes(&fixture.root) - cold < full_sizes / 2);
    contents(&mut fixture.reader(0), &original);
    let before = fixture.manifest(12);
    let locations = epoch.locations.clone();
    let shared = epoch.shared.as_ref().unwrap().locations.clone();
    items[0] = scalar_segment(900, "cancelled_body", &[99; 4096]);
    let token = CancellationToken::default();
    assert!(
        fixture
            .publish_with_hook(12, &logical, &items, &mut epoch, &token, || token.cancel())
            .is_none()
    );
    assert_eq!(fixture.manifest(12), before);
    assert_eq!(epoch.locations, locations);
    assert_eq!(epoch.shared.as_ref().unwrap().locations, shared);
    assert_eq!(
        fixture
            .publish(13, &logical, &items, &mut epoch)
            .physical_pack_writes,
        0
    );
    contents(&mut fixture.reader(13), &items);
}

#[test]
#[cfg(target_os = "linux")]
fn scalar_dictionary_namespace_race_cannot_advance_manifest_or_epoch() {
    let (fixture, mut items, logical) = dictionary_fixture(16);
    let mut epoch = Epoch::default();
    fixture.publish(0, &logical, &items, &mut epoch);
    let before = fixture.manifest(0);
    let locations = epoch.locations.clone();
    items[0] = scalar_segment(900, "changed", &[99; 4096]);
    let held = fixture.directory.join("held-dictionary");
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
    assert_eq!(fixture.manifest(0), before);
    assert_eq!(epoch.locations, locations);
    assert_eq!(fs::read_dir(&fixture.packs).unwrap().count(), 0);
    fs::remove_dir(&fixture.packs).unwrap();
    fs::rename(&held, &fixture.packs).unwrap();
    assert_eq!(
        fixture
            .publish(1, &logical, &items, &mut epoch)
            .physical_pack_writes,
        0
    );
    contents(&mut fixture.reader(1), &items);
}

fn reset_transport_work() {
    ITEM_HASH_BYTES.with(|total| total.set(0));
    PACK_READ_BYTES.with(|total| total.set(0));
    SNAPSHOT_READ_BYTES.with(|total| total.set(0));
}

fn transport_work() -> (usize, usize, usize) {
    (
        ITEM_HASH_BYTES.with(|total| total.get()),
        PACK_READ_BYTES.with(|total| total.get()),
        SNAPSHOT_READ_BYTES.with(|total| total.get()),
    )
}

#[test]
fn publication_checks_items_once_and_compares_complete_snapshots_without_materializing_packs() {
    let fixture = Fixture::new(24_000);
    let items = vec![item([11; 4000]), item([22; 4000]), item([33; 4000])];
    let mut epoch = Epoch::default();
    reset_transport_work();
    fixture.publish(0, b"logical", &items, &mut epoch);
    // Cold: one original-item hash pass, and one complete post-write file
    // comparison. No second item hash or newly materialized pack read.
    assert_eq!(transport_work(), (12_000, 0, 12_000 + PACK_HEADER));
    let pack = *epoch.packs.keys().next().unwrap();
    let snapshot = epoch.packs[&pack].bytes.as_ref().unwrap().clone();
    reset_transport_work();
    assert_eq!(
        fixture
            .publish(1, b"logical", &items, &mut epoch)
            .physical_pack_writes,
        0
    );
    // Warm: original key proofs come from exact immutable slice equality;
    // ALL current physical bytes are still read and compared once.
    assert_eq!(transport_work(), (0, 0, 12_000 + PACK_HEADER));
    assert!(Arc::ptr_eq(
        epoch.packs[&pack].bytes.as_ref().unwrap(),
        &snapshot
    ));

    let mut reader = fixture.reader(1);
    contents(&mut reader, &items);
    let mut restored = reader.hint_epoch();
    reset_transport_work();
    fixture.publish(2, b"logical", &items, &mut restored);
    assert_eq!(transport_work(), (0, 0, 12_000 + PACK_HEADER));

    // A disk-only hint has neither checked item keys nor checked snapshots.
    let mut hint = Epoch::hint(
        &fixture.packs,
        &fixture.manifest(2),
        fixture.bounds,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(hint.checked.is_empty());
    reset_transport_work();
    fixture.publish(3, b"logical", &items, &mut hint);
    assert_eq!(transport_work(), (12_000, 12_000 + PACK_HEADER, 0));

    // Merely reading one slice cannot certify other locators in that pack.
    let mut reader = fixture.reader(3);
    reader.load(items[0].0).unwrap();
    let mut partial = reader.hint_epoch();
    assert_eq!(partial.checked.len(), 1);
    reset_transport_work();
    fixture.publish(4, b"logical", &items, &mut partial);
    assert_eq!(transport_work(), (8000, 0, 12_000 + PACK_HEADER));
}

#[test]
fn unrequested_forged_locator_cannot_certify_a_current_item_from_a_checked_pack() {
    let fixture = Fixture::new(512);
    let items = vec![item(b"first body"), item(b"other body")];
    fixture.publish(0, b"logical", &items, &mut Epoch::default());
    let before = fixture.manifest(0);
    let mut manifest = Manifest::decode(&before, fixture.bounds).unwrap();
    let wrong = manifest.locations[&items[1].0];
    manifest.locations.insert(items[0].0, wrong);
    let forged = manifest.encode(fixture.bounds).unwrap();
    let token = CancellationToken::default();
    let mut reader = Reader::new(&fixture.packs, &forged, fixture.bounds, &token).unwrap();
    reader.load(items[1].0).unwrap();
    let mut hint = reader.hint_epoch();
    assert!(hint.packs.values().all(|pack| pack.bytes.is_some()));
    assert!(!hint.checked.contains(&items[0].0));
    let locations = hint.locations.clone();
    let checked = hint.checked.clone();
    let disk = cache_bytes(&fixture.root);
    // These supplied bytes equal the forged slice, but not the original key.
    let substituted = [(items[0].0, items[1].1.clone()), items[1].clone()];
    assert!(
        fixture
            .publish_with_hook(0, b"new", &substituted, &mut hint, &token, || {})
            .is_none()
    );
    assert_eq!(fixture.manifest(0), before);
    assert_eq!(hint.locations, locations);
    assert_eq!(hint.checked, checked);
    assert_eq!(cache_bytes(&fixture.root), disk);
}

#[test]
fn snapshot_comparison_checks_wrapper_and_stale_bytes_even_with_matching_file_stamps() {
    let fixture = Fixture::new(512);
    let items = vec![item([11; 40]), item([22; 40])];
    let mut epoch = Epoch::default();
    fixture.publish(0, b"both", &items, &mut epoch);
    let pack = *epoch.packs.keys().next().unwrap();
    let file = path(&fixture.packs, pack);
    let raw = fs::read(&file).unwrap();
    let snapshot = epoch.packs[&pack].bytes.as_ref().unwrap().clone();
    let selected = items[0].clone();
    let stale = epoch.locations[&items[1].0];
    let before = fixture.manifest(0);
    let locations = epoch.locations.clone();
    // Include bytes outside the selected slice: a valid selected item does not
    // permit a corrupt whole pack to be referenced by the new manifest.
    for offset in [0, 8, 40, 72, 80, 80 + PACK_HEADER + stale.offset] {
        let mut corrupt = raw.clone();
        corrupt[offset] ^= 1;
        fs::write(&file, corrupt).unwrap();
        epoch.packs.get_mut(&pack).unwrap().stamp = stamp(&file);
        assert!(
            inspect_snapshot(
                &fixture.packs,
                pack,
                snapshot.len(),
                &snapshot,
                fixture.bounds
            )
            .is_none()
        );
        let token = CancellationToken::default();
        assert!(
            fixture
                .publish_with_hook(
                    0,
                    b"selected",
                    std::slice::from_ref(&selected),
                    &mut epoch,
                    &token,
                    || token.cancel()
                )
                .is_none()
        );
        assert_eq!(fixture.manifest(0), before);
        assert_eq!(epoch.locations, locations);
        fs::write(&file, &raw).unwrap();
        epoch.packs.get_mut(&pack).unwrap().stamp = stamp(&file);
    }
    let mut longer = raw.clone();
    longer.push(0);
    for bytes in [&raw[..raw.len() - 1], longer.as_slice()] {
        fs::write(&file, bytes).unwrap();
        assert!(
            inspect_snapshot(
                &fixture.packs,
                pack,
                snapshot.len(),
                &snapshot,
                fixture.bounds
            )
            .is_none()
        );
    }
    // A normal retry repairs the corrupt referenced pack instead of trusting
    // the retained bytes or the stamp captured from the corrupt file.
    epoch.packs.get_mut(&pack).unwrap().stamp = stamp(&file);
    assert_eq!(
        fixture
            .publish(1, b"both", &items, &mut epoch)
            .physical_pack_writes,
        1
    );
    assert_eq!(fs::read(&file).unwrap(), raw);
    contents(&mut fixture.reader(1), &items);
}

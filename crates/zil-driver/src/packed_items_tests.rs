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

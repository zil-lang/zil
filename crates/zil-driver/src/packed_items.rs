//! A transport envelope for bounded raw item manifests. Hashes prove integrity,
//! never Verify authority. The logical manifest and its original quota checks
//! remain the consumer's responsibility.
use super::*;
use crate::artifacts::CacheNamespace;
use std::collections::{BTreeMap, BTreeSet};
use zil_db::CancellationToken;

const MANIFEST_MAGIC: &[u8; 8] = b"ZILPKM01";
const PACK_MAGIC: &[u8; 8] = b"ZILPAK01";
const MANIFEST_HEADER: usize = 32;
const PACK_ROW: usize = 40;
const ITEM_ROW: usize = 80;
const PACK_HEADER: usize = 20;

#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    pub maximum: usize,
    pub max_items: usize,
}
impl Bounds {
    pub(crate) fn manifest_maximum(self) -> Option<usize> {
        self.maximum.checked_add(MANIFEST_HEADER)?.checked_add(
            self.max_items
                .checked_mul(PACK_ROW.checked_add(ITEM_ROW)?)?,
        )
    }
    fn pack_maximum(self) -> Option<usize> {
        self.maximum.checked_add(PACK_HEADER)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Location {
    pack: Fingerprint,
    offset: usize,
    length: usize,
}
#[derive(Clone)]
struct PackRecord {
    length: usize,
    stamp: Option<FileStamp>,
}

type VerifiedPack = (Arc<[u8]>, Option<FileStamp>);
type CheckedPack = Option<(Option<Arc<[u8]>>, Option<FileStamp>)>;

/// Only the most recently published or restored current item set is remembered.
/// Old manifests carry their own locations; no process-global item index exists.
#[derive(Clone, Default)]
pub(crate) struct Epoch {
    namespace: Option<PathBuf>,
    locations: BTreeMap<Fingerprint, Location>,
    packs: BTreeMap<Fingerprint, PackRecord>,
}
impl Epoch {
    pub(crate) fn hint(root: &Path, bytes: &[u8], bounds: Bounds) -> Option<Self> {
        let manifest = Manifest::decode(bytes, bounds)?;
        Some(Self {
            namespace: Some(root.to_path_buf()),
            locations: manifest.locations,
            packs: manifest
                .packs
                .into_iter()
                .map(|(key, length)| {
                    (
                        key,
                        PackRecord {
                            length,
                            stamp: None,
                        },
                    )
                })
                .collect(),
        })
    }
    #[cfg(test)]
    pub(crate) fn first_pack_path(&self) -> Option<PathBuf> {
        Some(path(self.namespace.as_deref()?, *self.packs.keys().next()?))
    }
}

#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    length: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed: (i64, i64),
}
fn stamp(path: &Path) -> Option<FileStamp> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Some(FileStamp {
        length: metadata.len(),
        modified: metadata.modified().ok(),
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
        #[cfg(unix)]
        changed: (metadata.ctime(), metadata.ctime_nsec()),
    })
}
fn path(root: &Path, key: Fingerprint) -> PathBuf {
    root.join(format!("{key}.candidate"))
}
fn number(bytes: &[u8], start: usize) -> Option<usize> {
    usize::try_from(u64::from_le_bytes(
        bytes.get(start..start.checked_add(8)?)?.try_into().ok()?,
    ))
    .ok()
}
fn count(bytes: &[u8], start: usize) -> Option<usize> {
    usize::try_from(u32::from_le_bytes(
        bytes.get(start..start.checked_add(4)?)?.try_into().ok()?,
    ))
    .ok()
}
fn push_number(bytes: &mut Vec<u8>, value: usize) -> Option<()> {
    bytes.extend_from_slice(&u64::try_from(value).ok()?.to_le_bytes());
    Some(())
}
fn push_count(bytes: &mut Vec<u8>, value: usize) -> Option<()> {
    bytes.extend_from_slice(&u32::try_from(value).ok()?.to_le_bytes());
    Some(())
}

struct Manifest {
    logical: Vec<u8>,
    locations: BTreeMap<Fingerprint, Location>,
    packs: BTreeMap<Fingerprint, usize>,
}
pub(crate) fn is_packed(bytes: &[u8]) -> bool {
    bytes.get(..8) == Some(MANIFEST_MAGIC)
}

impl Manifest {
    fn decode(bytes: &[u8], bounds: Bounds) -> Option<Self> {
        if bytes.len() > bounds.manifest_maximum()? || !is_packed(bytes) {
            return None;
        }
        let logical_len = number(bytes, 8)?;
        let total = number(bytes, 16)?;
        let items = count(bytes, 24)?;
        let packs = count(bytes, 28)?;
        if logical_len > bounds.maximum
            || total > bounds.maximum
            || items == 0
            || items > bounds.max_items
            || packs == 0
            || packs > items
        {
            return None;
        }
        let item_start = MANIFEST_HEADER.checked_add(packs.checked_mul(PACK_ROW)?)?;
        let logical_start = item_start.checked_add(items.checked_mul(ITEM_ROW)?)?;
        if logical_start.checked_add(logical_len)? != bytes.len() {
            return None;
        }
        let mut pack_table = BTreeMap::new();
        let mut physical = 0usize;
        let mut previous = None;
        for row in bytes[MANIFEST_HEADER..item_start].as_chunks::<PACK_ROW>().0 {
            let key = Fingerprint(row[..32].try_into().ok()?);
            if previous.is_some_and(|previous| previous >= key) {
                return None;
            }
            previous = Some(key);
            let length = number(row, 32)?;
            let payload = length.checked_sub(PACK_HEADER).filter(|size| *size > 0)?;
            physical = physical
                .checked_add(payload)
                .filter(|size| *size <= bounds.maximum)?;
            pack_table.insert(key, length);
        }
        let mut locations = BTreeMap::new();
        let mut referenced = BTreeSet::new();
        let mut accumulated = 0usize;
        previous = None;
        for row in bytes[item_start..logical_start].as_chunks::<ITEM_ROW>().0 {
            let key = Fingerprint(row[..32].try_into().ok()?);
            if previous.is_some_and(|previous| previous >= key) {
                return None;
            }
            previous = Some(key);
            let pack = Fingerprint(row[32..64].try_into().ok()?);
            let offset = number(row, 64)?;
            let length = number(row, 72)?;
            if length == 0 {
                return None;
            }
            let capacity = pack_table.get(&pack)?.checked_sub(PACK_HEADER)?;
            if offset.checked_add(length)? > capacity {
                return None;
            }
            accumulated = accumulated
                .checked_add(length)
                .filter(|size| *size <= total)?;
            referenced.insert(pack);
            locations.insert(
                key,
                Location {
                    pack,
                    offset,
                    length,
                },
            );
        }
        if accumulated != total || referenced.len() != pack_table.len() {
            return None;
        }
        Some(Self {
            logical: bytes[logical_start..].to_vec(),
            locations,
            packs: pack_table,
        })
    }
    fn encode(&self, bounds: Bounds) -> Option<Vec<u8>> {
        let total = self
            .locations
            .values()
            .try_fold(0usize, |sum, item| sum.checked_add(item.length))?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MANIFEST_MAGIC);
        push_number(&mut bytes, self.logical.len())?;
        push_number(&mut bytes, total)?;
        push_count(&mut bytes, self.locations.len())?;
        push_count(&mut bytes, self.packs.len())?;
        for (key, length) in &self.packs {
            bytes.extend_from_slice(&key.0);
            push_number(&mut bytes, *length)?;
        }
        for (key, item) in &self.locations {
            bytes.extend_from_slice(&key.0);
            bytes.extend_from_slice(&item.pack.0);
            push_number(&mut bytes, item.offset)?;
            push_number(&mut bytes, item.length)?;
        }
        bytes.extend_from_slice(&self.logical);
        Self::decode(&bytes, bounds)?;
        Some(bytes)
    }
}

fn inspect_pack(
    root: &Path,
    key: Fingerprint,
    length: usize,
    bounds: Bounds,
) -> Option<VerifiedPack> {
    if length > bounds.pack_maximum()? {
        return None;
    }
    let file = path(root, key);
    let before = stamp(&file)?;
    let CacheRead::Candidate(bytes) = ArtifactCache::with_limit(root, length)
        .inspect_candidate(key)
        .ok()?
    else {
        return None;
    };
    if bytes.len() != length
        || bytes.get(..8)? != PACK_MAGIC
        || Fingerprint::of(&bytes) != key
        || number(&bytes, 8)? != bytes.len().checked_sub(PACK_HEADER)?
        || count(&bytes, 16)? == 0
        // A current manifest can select a small subset of an older pack. Its
        // old item count is bounded by nonempty payload bytes, not today's
        // selected function count, and never controls an allocation here.
        || count(&bytes, 16)? > number(&bytes, 8)?
    {
        return None;
    }
    let after = stamp(&file);
    let remembered = after.filter(|after| after == &before);
    Some((bytes.into(), remembered))
}
fn segment(bytes: &[u8], item: Location) -> Option<&[u8]> {
    let start = PACK_HEADER.checked_add(item.offset)?;
    bytes.get(start..start.checked_add(item.length)?)
}

pub(crate) struct Reader {
    root: PathBuf,
    bounds: Bounds,
    manifest: Manifest,
    loaded: BTreeMap<Fingerprint, VerifiedPack>,
    requested: BTreeSet<Fingerprint>,
    cancellation: CancellationToken,
}
impl Reader {
    pub(crate) fn new(
        root: &Path,
        bytes: &[u8],
        bounds: Bounds,
        cancellation: &CancellationToken,
    ) -> Option<Self> {
        if cancellation.is_cancelled() || !private_directory(root, false) {
            return None;
        }
        // All offsets, table sizes, logical bytes and the sum of unique pack
        // payload lengths are checked before any pack file is opened.
        Some(Self {
            root: root.to_path_buf(),
            bounds,
            manifest: Manifest::decode(bytes, bounds)?,
            loaded: BTreeMap::new(),
            requested: BTreeSet::new(),
            cancellation: cancellation.clone(),
        })
    }
    pub(crate) fn logical(&self) -> &[u8] {
        &self.manifest.logical
    }
    pub(crate) fn load(&mut self, key: Fingerprint) -> Option<Arc<[u8]>> {
        if self.cancellation.is_cancelled() {
            return None;
        }
        let item = *self.manifest.locations.get(&key)?;
        if !self.loaded.contains_key(&item.pack) {
            let bytes = inspect_pack(
                &self.root,
                item.pack,
                *self.manifest.packs.get(&item.pack)?,
                self.bounds,
            )?;
            self.loaded.insert(item.pack, bytes);
        }
        let bytes = segment(&self.loaded.get(&item.pack)?.0, item)?;
        if Fingerprint::of(bytes) != key || self.cancellation.is_cancelled() {
            return None;
        }
        self.requested.insert(key);
        Some(bytes.into())
    }
    pub(crate) fn epoch(&self) -> Epoch {
        let locations: BTreeMap<_, _> = self
            .requested
            .iter()
            .map(|key| (*key, self.manifest.locations[key]))
            .collect();
        let packs = locations
            .values()
            .map(|item| item.pack)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|key| {
                let (bytes, stamp) = &self.loaded[&key];
                (
                    key,
                    PackRecord {
                        length: bytes.len(),
                        stamp: stamp.clone(),
                    },
                )
            })
            .collect();
        Epoch {
            namespace: Some(self.root.clone()),
            locations,
            packs,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PublicationStats {
    pub written_items: usize,
    pub existing_items: usize,
    pub written_bytes: usize,
    pub manifest_bytes: usize,
    pub physical_pack_writes: usize,
    pub physical_pack_bytes: usize,
    pub manifest_writes: usize,
    pub compacted_items: usize,
    pub referenced_packs: usize,
    pub referenced_pack_payload_bytes: usize,
    pub encode_ns: u128,
    pub guard_ns: u128,
    pub item_io_ns: u128,
    pub manifest_io_ns: u128,
}
pub(crate) struct PublishContext<'a> {
    pub packs: &'a CacheNamespace,
    pub manifest_root: &'a Path,
    pub namespace_key: &'a Path,
    pub manifest_key: Fingerprint,
    pub cancellation: &'a CancellationToken,
    pub profile: bool,
}

pub(crate) fn publish(
    context: PublishContext<'_>,
    logical: &[u8],
    items: Vec<(Fingerprint, Arc<[u8]>)>,
    bounds: Bounds,
    epoch: &mut Epoch,
) -> Option<PublicationStats> {
    publish_with_hook(context, logical, items, bounds, epoch, || {})
}

fn publish_with_hook(
    context: PublishContext<'_>,
    logical: &[u8],
    items: Vec<(Fingerprint, Arc<[u8]>)>,
    bounds: Bounds,
    epoch: &mut Epoch,
    before_manifest: impl FnOnce(),
) -> Option<PublicationStats> {
    let mut stats = PublicationStats::default();
    let encode = publication_start(context.profile);
    if context.cancellation.is_cancelled()
        || logical.len() > bounds.maximum
        || items.is_empty()
        || items.len() > bounds.max_items
    {
        return None;
    }
    let logical_items = items.len();
    let mut current = BTreeMap::new();
    let mut current_total = 0usize;
    for (key, bytes) in items {
        if context.cancellation.is_cancelled() || bytes.is_empty() {
            return None;
        }
        if let Some(previous) = current.get(&key) {
            if previous != &bytes {
                return None;
            }
        } else {
            current_total = current_total
                .checked_add(bytes.len())
                .filter(|size| *size <= bounds.maximum)?;
            current.insert(key, bytes);
        }
    }
    stats.encode_ns += publication_elapsed(encode);
    let namespace_matches = epoch.namespace.as_deref() == Some(context.namespace_key);
    let mut proposed = BTreeMap::new();
    let mut old_packs = BTreeMap::new();
    let mut proposed_bytes = 0usize;
    if namespace_matches {
        for (key, bytes) in &current {
            if let Some(item) = epoch.locations.get(key)
                && let Some(record) = epoch.packs.get(&item.pack)
                && item.length == bytes.len()
                && record.length <= bounds.pack_maximum()?
                && item.offset.checked_add(item.length)?
                    <= record.length.checked_sub(PACK_HEADER)?
            {
                proposed.insert(*key, *item);
                old_packs.insert(item.pack, record.length);
                proposed_bytes = proposed_bytes.checked_add(item.length)?;
            }
        }
    }
    // Bound old pack I/O before opening any old pack, including unused bytes.
    let old_bytes = old_packs.values().try_fold(0usize, |sum, length| {
        sum.checked_add(length.checked_sub(PACK_HEADER)?)
    })?;
    let missing_bytes = current_total.checked_sub(proposed_bytes)?;
    let compact = old_bytes.checked_add(missing_bytes)? > bounds.maximum;
    if compact {
        stats.compacted_items = proposed.len();
    }
    let mut checked = BTreeMap::<Fingerprint, CheckedPack>::new();
    let mut locations = BTreeMap::new();
    let mut packs = BTreeMap::new();
    let mut records = BTreeMap::new();
    let mut dirty = BTreeSet::new();
    for key in current.keys() {
        let io = publication_start(context.profile);
        let found = (|| {
            if compact {
                return None;
            }
            let item = *proposed.get(key)?;
            let record = epoch.packs.get(&item.pack)?;
            let value = checked.entry(item.pack).or_insert_with(|| {
                let now = stamp(&path(context.packs.root(), item.pack));
                if now.is_some() && now == record.stamp {
                    Some((None, now))
                } else {
                    inspect_pack(context.packs.root(), item.pack, record.length, bounds)
                        .map(|(bytes, stamp)| (Some(bytes), stamp))
                }
            });
            let (pack_bytes, stamp) = value.as_ref()?;
            if let Some(pack_bytes) = pack_bytes
                && Fingerprint::of(segment(pack_bytes, item)?) != *key
            {
                return None;
            }
            packs.insert(item.pack, record.length);
            records.insert(
                item.pack,
                PackRecord {
                    length: record.length,
                    stamp: stamp.clone(),
                },
            );
            Some(item)
        })();
        stats.item_io_ns += publication_elapsed(io);
        if let Some(item) = found {
            locations.insert(*key, item);
        } else {
            dirty.insert(*key);
        }
    }
    // The planned read volume includes stale bytes inside every referenced
    // old pack. Exceeding the original byte allowance compacts current shared
    // item bytes; it never grants a fresh allowance to each old pack.
    let old_payload = packs.values().try_fold(0usize, |sum, length| {
        sum.checked_add(length.checked_sub(PACK_HEADER)?)
    })?;
    let dirty_bytes = dirty
        .iter()
        .try_fold(0usize, |sum, key| sum.checked_add(current[key].len()))?;
    if old_payload.checked_add(dirty_bytes)? > bounds.maximum {
        stats.compacted_items = current.len().checked_sub(dirty.len())?;
        locations.clear();
        packs.clear();
        records.clear();
        dirty = current.keys().copied().collect();
    }
    if !dirty.is_empty() {
        let encode = publication_start(context.profile);
        let data_len = dirty
            .iter()
            .try_fold(0usize, |sum, key| sum.checked_add(current[key].len()))?;
        let mut bytes = Vec::with_capacity(PACK_HEADER.checked_add(data_len)?);
        bytes.extend_from_slice(PACK_MAGIC);
        push_number(&mut bytes, data_len)?;
        push_count(&mut bytes, dirty.len())?;
        let mut offsets = Vec::with_capacity(dirty.len());
        for key in &dirty {
            let item = &current[key];
            if context.cancellation.is_cancelled() || Fingerprint::of(item) != *key {
                return None;
            }
            offsets.push((*key, bytes.len().checked_sub(PACK_HEADER)?, item.len()));
            bytes.extend_from_slice(item);
        }
        let pack = Fingerprint::of(&bytes);
        stats.encode_ns += publication_elapsed(encode);
        let io = publication_start(context.profile);
        let existing = inspect_pack(context.packs.root(), pack, bytes.len(), bounds);
        stats.item_io_ns += publication_elapsed(io);
        let stamp = if let Some((_, stamp)) =
            existing.filter(|(existing, _)| existing.as_ref() == bytes.as_slice())
        {
            stamp
        } else {
            let guard = publication_start(context.profile);
            context.packs.validate_write().ok()?;
            stats.guard_ns += publication_elapsed(guard);
            let io = publication_start(context.profile);
            ArtifactCache::with_limit(context.packs.root(), bounds.pack_maximum()?)
                .write_candidate_cancellable(pack, &bytes, context.cancellation)
                .ok()?;
            let written_stamp = stamp(&path(context.packs.root(), pack))?;
            let (_, verified_stamp) =
                inspect_pack(context.packs.root(), pack, bytes.len(), bounds)?;
            stats.item_io_ns += publication_elapsed(io);
            stats.physical_pack_writes = 1;
            stats.physical_pack_bytes = usize::try_from(written_stamp.length).ok()?;
            stats.written_items = dirty.len();
            stats.written_bytes = data_len;
            verified_stamp
        };
        for (key, offset, length) in offsets {
            locations.insert(
                key,
                Location {
                    pack,
                    offset,
                    length,
                },
            );
        }
        packs.insert(pack, bytes.len());
        records.insert(
            pack,
            PackRecord {
                length: bytes.len(),
                stamp,
            },
        );
    }
    stats.existing_items = logical_items.checked_sub(stats.written_items)?;
    stats.referenced_packs = packs.len();
    stats.referenced_pack_payload_bytes = packs.values().try_fold(0usize, |sum, length| {
        sum.checked_add(length.checked_sub(PACK_HEADER)?)
    })?;
    let encode = publication_start(context.profile);
    let envelope = Manifest {
        logical: logical.to_vec(),
        locations: locations.clone(),
        packs,
    }
    .encode(bounds)?;
    stats.encode_ns += publication_elapsed(encode);
    before_manifest();
    let guard = publication_start(context.profile);
    if context.cancellation.is_cancelled() {
        return None;
    }
    context.packs.validate().ok()?;
    stats.guard_ns += publication_elapsed(guard);
    let io = publication_start(context.profile);
    ArtifactCache::with_limit(context.manifest_root, bounds.manifest_maximum()?)
        .write_candidate_cancellable(context.manifest_key, &envelope, context.cancellation)
        .ok()?;
    stats.manifest_io_ns = publication_elapsed(io);
    stats.manifest_writes = 1;
    stats.manifest_bytes = envelope.len();
    *epoch = Epoch {
        namespace: Some(context.namespace_key.to_path_buf()),
        locations,
        packs: records,
    };
    Some(stats)
}

#[cfg(test)]
#[path = "packed_items_tests.rs"]
mod tests;

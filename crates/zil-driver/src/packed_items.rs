//! A transport envelope for bounded raw item manifests. Hashes prove integrity,
//! never Verify authority. The logical manifest and its original quota checks
//! remain the consumer's responsibility.
use super::*;
use crate::artifacts::CacheNamespace;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use zil_db::CancellationToken;

#[path = "packed_items_dictionary.rs"]
mod dictionary;

const MANIFEST_MAGIC: &[u8; 8] = b"ZILPKM01";
const PACK_MAGIC: &[u8; 8] = b"ZILPAK01";
const MANIFEST_HEADER: usize = 32;
const PACK_ROW: usize = 40;
const ITEM_ROW: usize = 80;
const PACK_HEADER: usize = 20;
// A second transport layer shares the otherwise repeated locator/binding
// tables. Its output is exactly the old envelope, not a semantic certificate.
const SHARED_MAGIC: &[u8; 8] = b"ZILPKS02";
const SHARED_HEADER: usize = 52;
const CHUNK_MIN: usize = 2048;
const CHUNK_MAX: usize = 8192;
const SHARED_THRESHOLD: usize = 2 * CHUNK_MAX;

const fn gear_table() -> [u64; 256] {
    let mut table = [0; 256];
    let mut index = 0;
    while index < table.len() {
        let mut value = (index as u64).wrapping_add(0x9e3779b97f4a7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        table[index] = value ^ (value >> 31);
        index += 1;
    }
    table
}
const GEAR: [u64; 256] = gear_table();

/// Content-defined boundaries resynchronize after insertions in sorted tables;
/// fixed offsets would duplicate their entire shifted suffix. Always bounded,
/// deterministic and independent of function identities or benchmark names.
fn chunks(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut remaining = bytes;
    std::iter::from_fn(move || {
        if remaining.is_empty() {
            return None;
        }
        let mut end = remaining.len().min(CHUNK_MAX);
        let mut hash = 0u64;
        for (index, byte) in remaining[..end].iter().enumerate() {
            hash = (hash << 1).wrapping_add(GEAR[usize::from(*byte)]);
            if index + 1 >= CHUNK_MIN && hash & 2047 == 0 {
                end = index + 1;
                break;
            }
        }
        let (chunk, rest) = remaining.split_at(end);
        remaining = rest;
        Some(chunk)
    })
}

#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    pub maximum: usize,
    pub max_items: usize,
}
impl Bounds {
    pub(crate) fn manifest_maximum(self) -> Option<usize> {
        // Logical bytes and inline reconstruction prefixes each retain the
        // original byte allowance. Only transport table overhead grows.
        self.maximum
            .checked_mul(2)?
            .checked_add(MANIFEST_HEADER)?
            .checked_add(dictionary::HEADER)?
            .checked_add(
                self.max_items.checked_mul(
                    PACK_ROW
                        .checked_add(ITEM_ROW)?
                        .checked_add(dictionary::ROW)?,
                )?,
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
    // One bounded current pack snapshot; never semantic reuse authority.
    // Unique payloads obey the original aggregate physical allowance, including
    // stale bytes. Cloned epochs share allocations, never chain old epochs.
    bytes: Option<Arc<Vec<u8>>>,
}

type VerifiedPack = (Arc<Vec<u8>>, Option<FileStamp>);
type CheckedPack = Option<VerifiedPack>;

// Borrowed immutable bytes whose item key has already been checked. Placement
// consumes this association instead of hashing the same item a second time.
#[derive(Clone, Copy)]
struct CheckedItem<'a> {
    key: Fingerprint,
    bytes: &'a [u8],
}
impl<'a> CheckedItem<'a> {
    fn check(key: Fingerprint, bytes: &'a [u8], previous: &Epoch) -> Option<Self> {
        // This compares actual bytes against a previously checked immutable
        // slice. A locator, path or file stamp alone cannot establish its key.
        if previous.checked_bytes(key) != Some(bytes) && item_fingerprint(bytes) != key {
            return None;
        }
        Some(Self { key, bytes })
    }
    fn of(bytes: &'a [u8]) -> Self {
        Self {
            key: item_fingerprint(bytes),
            bytes,
        }
    }
}

#[cfg(test)]
thread_local! {
    static ITEM_HASH_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PACK_READ_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static SNAPSHOT_READ_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
fn item_fingerprint(bytes: &[u8]) -> Fingerprint {
    #[cfg(test)]
    ITEM_HASH_BYTES.with(|total| total.set(total.get() + bytes.len()));
    Fingerprint::of(bytes)
}

/// Only the most recently published or restored current item set is remembered.
/// Old manifests carry their own locations; no process-global item index exists.
#[derive(Clone, Default)]
pub(crate) struct Epoch {
    namespace: Option<PathBuf>,
    locations: BTreeMap<Fingerprint, Location>,
    packs: BTreeMap<Fingerprint, PackRecord>,
    // Only keys actually bound to their immutable pack slices by publication
    // or Reader.load. Unread manifest locators are never in this set.
    checked: BTreeSet<Fingerprint>,
    // Exactly one metadata placement epoch, never an epoch history.
    shared: Option<Arc<Epoch>>,
}
impl Epoch {
    fn checked_bytes(&self, key: Fingerprint) -> Option<&[u8]> {
        if !self.checked.contains(&key) {
            return None;
        }
        let item = *self.locations.get(&key)?;
        segment(self.packs.get(&item.pack)?.bytes.as_ref()?, item)
    }
    pub(crate) fn hint(
        root: &Path,
        bytes: &[u8],
        bounds: Bounds,
        cancellation: &CancellationToken,
    ) -> Option<Self> {
        if !private_directory(root, false) {
            return None;
        }
        let manifest = read_manifest(root, bytes, bounds, cancellation)?;
        Some(Self {
            namespace: Some(root.to_path_buf()),
            shared: manifest.shared,
            locations: manifest.locations,
            checked: BTreeSet::new(),
            packs: manifest
                .packs
                .into_iter()
                .map(|(key, length)| {
                    (
                        key,
                        PackRecord {
                            length,
                            stamp: None,
                            bytes: None,
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
    shared: Option<Arc<Epoch>>,
    // Empty for the legacy complete-item transport. Otherwise locations name
    // physical suffixes and these rows bind the original complete item keys.
    recipes: BTreeMap<Fingerprint, dictionary::Recipe>,
}
pub(crate) fn is_packed(bytes: &[u8]) -> bool {
    bytes.get(..8) == Some(MANIFEST_MAGIC)
        || bytes.get(..8) == Some(SHARED_MAGIC)
        || bytes.get(..8) == Some(dictionary::MAGIC)
}

impl Manifest {
    fn decode(bytes: &[u8], bounds: Bounds) -> Option<Self> {
        if bytes.len() > bounds.manifest_maximum()? || bytes.get(..8) != Some(MANIFEST_MAGIC) {
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
            shared: None,
            recipes: BTreeMap::new(),
        })
    }
    #[cfg(test)]
    fn encode(&self, bounds: Bounds) -> Option<Vec<u8>> {
        dictionary::encode(
            &self.logical,
            &self.locations,
            &self.packs,
            &self.recipes,
            bounds,
        )
    }
    fn encode_parts(
        logical: &[u8],
        locations: &BTreeMap<Fingerprint, Location>,
        packs: &BTreeMap<Fingerprint, usize>,
        bounds: Bounds,
    ) -> Option<Vec<u8>> {
        // Validate the same invariants as decode before allocating the output.
        // Sorted unique tables are guaranteed by BTreeMap; do not reconstruct
        // them and copy the logical payload just to discard a decoded manifest.
        if logical.len() > bounds.maximum
            || locations.is_empty()
            || locations.len() > bounds.max_items
            || packs.is_empty()
            || packs.len() > locations.len()
        {
            return None;
        }
        let mut physical = 0usize;
        for length in packs.values() {
            let payload = length.checked_sub(PACK_HEADER).filter(|size| *size > 0)?;
            physical = physical
                .checked_add(payload)
                .filter(|size| *size <= bounds.maximum)?;
        }
        let mut total = 0usize;
        let mut referenced = BTreeSet::new();
        for item in locations.values() {
            if item.length == 0
                || item.offset.checked_add(item.length)?
                    > packs.get(&item.pack)?.checked_sub(PACK_HEADER)?
            {
                return None;
            }
            total = total
                .checked_add(item.length)
                .filter(|size| *size <= bounds.maximum)?;
            referenced.insert(item.pack);
        }
        if referenced.len() != packs.len() {
            return None;
        }
        let size = MANIFEST_HEADER
            .checked_add(packs.len().checked_mul(PACK_ROW)?)?
            .checked_add(locations.len().checked_mul(ITEM_ROW)?)?
            .checked_add(logical.len())?;
        if size > bounds.manifest_maximum()? {
            return None;
        }
        let mut bytes = Vec::with_capacity(size);
        bytes.extend_from_slice(MANIFEST_MAGIC);
        push_number(&mut bytes, logical.len())?;
        push_number(&mut bytes, total)?;
        push_count(&mut bytes, locations.len())?;
        push_count(&mut bytes, packs.len())?;
        for (key, length) in packs {
            bytes.extend_from_slice(&key.0);
            push_number(&mut bytes, *length)?;
        }
        for (key, item) in locations {
            bytes.extend_from_slice(&key.0);
            bytes.extend_from_slice(&item.pack.0);
            push_number(&mut bytes, item.offset)?;
            push_number(&mut bytes, item.length)?;
        }
        bytes.extend_from_slice(logical);
        Some(bytes)
    }
}

fn metadata_bounds(bounds: Bounds, total: usize, count: usize) -> Option<Bounds> {
    if total < SHARED_THRESHOLD
        || total > bounds.manifest_maximum()?
        || count == 0
        || count > total.div_ceil(CHUNK_MIN)
    {
        return None;
    }
    let maximum = bounds.manifest_maximum()?;
    // A referenced physical pack can include old chunks. Bound that header by
    // the original metadata allowance, while the CURRENT descriptor count and
    // exact referenced-key set are checked independently below. Using today's
    // chunk count here would needlessly rewrite a whole pack after one edit.
    Some(Bounds {
        maximum,
        max_items: maximum.div_ceil(CHUNK_MIN),
    })
}

fn read_manifest(
    root: &Path,
    bytes: &[u8],
    bounds: Bounds,
    cancellation: &CancellationToken,
) -> Option<Manifest> {
    if cancellation.is_cancelled() || bytes.len() > bounds.manifest_maximum()? {
        return None;
    }
    if bytes.get(..8) != Some(SHARED_MAGIC) {
        return dictionary::decode(bytes, bounds);
    }
    let total = number(bytes, 8)?;
    let digest = Fingerprint(bytes.get(16..48)?.try_into().ok()?);
    let count = count(bytes, 48)?;
    let meta_bounds = metadata_bounds(bounds, total, count)?;
    let index_end = SHARED_HEADER.checked_add(count.checked_mul(PACK_ROW)?)?;
    let rows = bytes
        .get(SHARED_HEADER..index_end)?
        .as_chunks::<PACK_ROW>()
        .0;
    // The raw inner table is one bounded level, never a recursive envelope.
    let chunks = Manifest::decode(bytes.get(index_end..)?, meta_bounds)?;
    if !chunks.logical.is_empty() {
        return None;
    }
    let mut expected = 0usize;
    let mut used = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        let key = Fingerprint(row[..32].try_into().ok()?);
        let length = number(row, 32)?;
        if length == 0
            || length > CHUNK_MAX
            || (index + 1 < count && length < CHUNK_MIN)
            || chunks.locations.get(&key)?.length != length
        {
            return None;
        }
        expected = expected.checked_add(length).filter(|size| *size <= total)?;
        used.insert(key);
    }
    if expected != total || used.len() != chunks.locations.len() {
        return None;
    }
    // All declared offsets and aggregate physical payload (including stale
    // bytes) were bounded above, before opening a single metadata pack.
    let mut loaded = BTreeMap::new();
    let mut records = BTreeMap::new();
    for (&key, &length) in &chunks.packs {
        if cancellation.is_cancelled() {
            return None;
        }
        let (data, stamp) = inspect_pack(root, key, length, meta_bounds)?;
        loaded.insert(key, data);
        records.insert(
            key,
            PackRecord {
                length,
                stamp,
                bytes: loaded.get(&key).cloned(),
            },
        );
    }
    let mut envelope = Vec::with_capacity(total);
    for row in rows {
        if cancellation.is_cancelled() {
            return None;
        }
        let key = Fingerprint(row[..32].try_into().ok()?);
        let item = *chunks.locations.get(&key)?;
        let data = segment(loaded.get(&item.pack)?, item)?;
        if Fingerprint::of(data) != key {
            return None;
        }
        envelope.extend_from_slice(data);
    }
    if envelope.len() != total || Fingerprint::of(&envelope) != digest {
        return None;
    }
    let mut decoded = dictionary::decode(&envelope, bounds)?;
    decoded.shared = Some(Arc::new(Epoch {
        namespace: Some(root.to_path_buf()),
        checked: chunks.locations.keys().copied().collect(),
        locations: chunks.locations,
        packs: records,
        shared: None,
    }));
    Some(decoded)
}

fn share_manifest(
    context: &PublishContext<'_>,
    envelope: Vec<u8>,
    bounds: Bounds,
    previous: Option<&Epoch>,
    stats: &mut PublicationStats,
) -> Option<(Vec<u8>, Option<Arc<Epoch>>)> {
    if envelope.len() < SHARED_THRESHOLD {
        return Some((envelope, None));
    }
    let encode = publication_start(context.profile);
    let pieces: Vec<_> = chunks(&envelope).collect();
    let meta_bounds = metadata_bounds(bounds, envelope.len(), pieces.len())?;
    let mut descriptor =
        Vec::with_capacity(SHARED_HEADER.checked_add(pieces.len().checked_mul(PACK_ROW)?)?);
    descriptor.extend_from_slice(SHARED_MAGIC);
    push_number(&mut descriptor, envelope.len())?;
    descriptor.extend_from_slice(&Fingerprint::of(&envelope).0);
    push_count(&mut descriptor, pieces.len())?;
    let mut items = Vec::with_capacity(pieces.len());
    for piece in pieces {
        if context.cancellation.is_cancelled() {
            return None;
        }
        let item = CheckedItem::of(piece);
        descriptor.extend_from_slice(&item.key.0);
        push_number(&mut descriptor, piece.len())?;
        items.push(item);
    }
    stats.encode_ns += publication_elapsed(encode);
    let empty = Epoch::default();
    let mut metadata = PublicationStats::default();
    let Placement {
        locations,
        packs,
        records,
        ..
    } = place_items(
        context,
        items,
        meta_bounds,
        previous.unwrap_or(&empty),
        &mut metadata,
        true,
    )?;
    let encode = publication_start(context.profile);
    descriptor.extend_from_slice(&Manifest::encode_parts(
        &[],
        &locations,
        &packs,
        meta_bounds,
    )?);
    if descriptor.len() > bounds.manifest_maximum()? {
        return None;
    }
    stats.encode_ns += metadata.encode_ns + publication_elapsed(encode);
    stats.guard_ns += metadata.guard_ns;
    stats.manifest_io_ns += metadata.item_io_ns;
    stats.manifest_bytes = stats
        .manifest_bytes
        .checked_add(metadata.physical_pack_bytes)?;
    Some((
        descriptor,
        Some(Arc::new(Epoch {
            namespace: Some(context.namespace_key.to_path_buf()),
            checked: locations.keys().copied().collect(),
            locations,
            packs: records,
            shared: None,
        })),
    ))
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
        .inspect_content_candidate(key)
        .ok()?
    else {
        return None;
    };
    check_pack_shape(&bytes, length)?;
    let after = stamp(&file);
    let remembered = after.filter(|after| after == &before);
    #[cfg(test)]
    PACK_READ_BYTES.with(|total| total.set(total.get() + bytes.len()));
    Some((Arc::new(bytes), remembered))
}

fn check_pack_shape(bytes: &[u8], length: usize) -> Option<()> {
    if bytes.len() != length
        || bytes.get(..8)? != PACK_MAGIC
        || number(bytes, 8)? != bytes.len().checked_sub(PACK_HEADER)?
        || count(bytes, 16)? == 0
        // A current manifest can select a small subset of an older pack. Its
        // old item count is bounded by nonempty payload bytes, not today's
        // selected function count, and never controls an allocation here.
        || count(bytes, 16)? > number(bytes, 8)?
    {
        return None;
    }
    Some(())
}

/// Revalidate a current file against its already digest-checked immutable
/// payload. Read and compare the ENTIRE envelope and payload, including stale
/// pack bytes. Neither the path nor equal stat fields can skip this read.
/// Exact equality to that snapshot proves the same checksum without another
/// full hash, allocation, or removal/copy of ArtifactCache's envelope header.
fn inspect_snapshot(
    root: &Path,
    key: Fingerprint,
    length: usize,
    snapshot: &Arc<Vec<u8>>,
    bounds: Bounds,
) -> Option<VerifiedPack> {
    if length > bounds.pack_maximum()? {
        return None;
    }
    check_pack_shape(snapshot, length)?;
    let file_path = path(root, key);
    let before = stamp(&file_path)?;
    let mut file = fs::File::open(&file_path).ok()?;
    // ArtifactCache's raw v2 envelope. Both its identity and checksum must
    // equal the CAS key that was checked when this snapshot was admitted.
    // Unknown formats are a miss, never an unchecked payload fallback.
    let mut header = [0u8; 80];
    if usize::try_from(file.metadata().ok()?.len()).ok()? != length.checked_add(header.len())? {
        return None;
    }
    file.read_exact(&mut header).ok()?;
    if &header[..8] != b"ZILRAW02"
        || header[8..40] != key.0
        || header[40..72] != key.0
        || number(&header, 72)? != length
    {
        return None;
    }
    let mut buffer = [0u8; 16 * 1024];
    for expected in snapshot.chunks(buffer.len()) {
        let actual = &mut buffer[..expected.len()];
        file.read_exact(actual).ok()?;
        if actual != expected {
            return None;
        }
    }
    if file.read(&mut buffer[..1]).ok()? != 0 {
        return None;
    }
    let remembered = stamp(&file_path).filter(|after| after == &before);
    #[cfg(test)]
    SNAPSHOT_READ_BYTES.with(|total| total.set(total.get() + snapshot.len()));
    Some((snapshot.clone(), remembered))
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
    // Successful immutable item bytes only, local to this one bounded reader.
    // Repeated logical rows need neither another hash nor another payload copy.
    items: BTreeMap<Fingerprint, Arc<[u8]>>,
    item_bytes: usize,
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
            manifest: read_manifest(root, bytes, bounds, cancellation)?,
            loaded: BTreeMap::new(),
            items: BTreeMap::new(),
            item_bytes: 0,
            requested: BTreeSet::new(),
            cancellation: cancellation.clone(),
        })
    }
    #[cfg(test)]
    pub(crate) fn logical(&self) -> &[u8] {
        &self.manifest.logical
    }
    /// The decoder consumes this request's bounded logical table while the
    /// same Reader keeps serving its already checked physical item locations.
    pub(crate) fn take_logical(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.manifest.logical)
    }
    /// Reuse this request's bounded locators and loaded raw pack snapshots.
    /// Publication rechecks complete current file and segment bytes;
    /// unread packs remain hints and must still be inspected from disk.
    pub(crate) fn hint_epoch(&self) -> Epoch {
        Epoch {
            namespace: Some(self.root.clone()),
            locations: self.manifest.locations.clone(),
            checked: self.requested.clone(),
            packs: self
                .manifest
                .packs
                .iter()
                .map(|(key, length)| {
                    (
                        *key,
                        PackRecord {
                            length: *length,
                            stamp: self.loaded.get(key).and_then(|(_, stamp)| stamp.clone()),
                            bytes: self.loaded.get(key).map(|(bytes, _)| bytes.clone()),
                        },
                    )
                })
                .collect(),
            shared: self.manifest.shared.clone(),
        }
    }
    pub(crate) fn load(&mut self, key: Fingerprint) -> Option<Arc<[u8]>> {
        if self.cancellation.is_cancelled() {
            return None;
        }
        if let Some(bytes) = self.items.get(&key) {
            return Some(bytes.clone());
        }
        let recipe = if self.manifest.recipes.is_empty() {
            None
        } else {
            Some(self.manifest.recipes.get(&key)?)
        };
        let physical_key = recipe.map_or(key, |recipe| recipe.body);
        let item = *self.manifest.locations.get(&physical_key)?;
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
        if (!self.requested.contains(&physical_key) && item_fingerprint(bytes) != physical_key)
            || self.cancellation.is_cancelled()
        {
            return None;
        }
        let bytes: Arc<[u8]> =
            if let Some(recipe) = recipe.filter(|recipe| !recipe.prefix.is_empty()) {
                let length = recipe.prefix.len().checked_add(bytes.len())?;
                if length > self.bounds.maximum {
                    return None;
                }
                let mut restored = Vec::with_capacity(length);
                restored.extend_from_slice(&recipe.prefix);
                restored.extend_from_slice(bytes);
                if Fingerprint::of(&restored) != key || self.cancellation.is_cancelled() {
                    return None;
                }
                restored.into()
            } else {
                bytes.into()
            };
        self.requested.insert(physical_key);
        if let Some(total) = self.item_bytes.checked_add(bytes.len())
            && total <= self.bounds.maximum
            && self.items.len() < self.bounds.max_items
        {
            self.item_bytes = total;
            self.items.insert(key, bytes.clone());
        }
        // Retention refusal changes only work, never the read result or quota.
        Some(bytes)
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
                        bytes: Some(bytes.clone()),
                    },
                )
            })
            .collect();
        Epoch {
            namespace: Some(self.root.clone()),
            checked: self.requested.clone(),
            locations,
            packs,
            shared: self.manifest.shared.clone(),
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
    if context.cancellation.is_cancelled()
        || logical.len() > bounds.maximum
        || items.is_empty()
        || items.len() > bounds.max_items
    {
        return None;
    }
    let encode = publication_start(context.profile);
    let prepared = dictionary::prepare(&items, bounds, epoch, context.cancellation)?;
    stats.encode_ns += publication_elapsed(encode);
    let Placement {
        locations,
        packs,
        records,
        written_pack,
    } = place_items(
        &context,
        prepared.parts,
        bounds,
        epoch,
        &mut stats,
        !prepared.recipes.is_empty(),
    )?;
    if !prepared.recipes.is_empty() {
        // Keep logical item counters in the caller's original domain, even
        // when many original items depend on the same newly written suffix.
        stats.written_items = prepared
            .recipes
            .values()
            .filter(|recipe| written_pack == Some(locations[&recipe.body].pack))
            .count();
    }
    stats.existing_items = items.len().checked_sub(stats.written_items)?;
    let encode = publication_start(context.profile);
    let envelope = dictionary::encode(logical, &locations, &packs, &prepared.recipes, bounds)?;
    stats.encode_ns += publication_elapsed(encode);
    let (envelope, shared) = share_manifest(
        &context,
        envelope,
        bounds,
        epoch.shared.as_deref(),
        &mut stats,
    )?;
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
    stats.manifest_io_ns += publication_elapsed(io);
    stats.manifest_writes = 1;
    stats.manifest_bytes = stats.manifest_bytes.checked_add(envelope.len())?;
    *epoch = Epoch {
        namespace: Some(context.namespace_key.to_path_buf()),
        checked: locations.keys().copied().collect(),
        locations,
        packs: records,
        shared,
    };
    Some(stats)
}

struct Placement {
    locations: BTreeMap<Fingerprint, Location>,
    packs: BTreeMap<Fingerprint, usize>,
    records: BTreeMap<Fingerprint, PackRecord>,
    written_pack: Option<Fingerprint>,
}

// Body and metadata chunks use the same complete pack bounds and current
// file checks. Placement does not publish a root or install a pending epoch.
fn place_items(
    context: &PublishContext<'_>,
    items: Vec<CheckedItem<'_>>,
    bounds: Bounds,
    epoch: &Epoch,
    stats: &mut PublicationStats,
    selective_compaction: bool,
) -> Option<Placement> {
    if context.cancellation.is_cancelled() || items.is_empty() || items.len() > bounds.max_items {
        return None;
    }
    let encode = publication_start(context.profile);
    let logical_items = items.len();
    let mut current = BTreeMap::<Fingerprint, &[u8]>::new();
    let mut current_total = 0usize;
    for CheckedItem { key, bytes } in items {
        if context.cancellation.is_cancelled() || bytes.is_empty() {
            return None;
        }
        if let Some(previous) = current.get(&key) {
            if *previous != bytes {
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
    let mut old_bytes = old_packs.values().try_fold(0usize, |sum, length| {
        sum.checked_add(length.checked_sub(PACK_HEADER)?)
    })?;
    if selective_compaction
        && old_bytes.checked_add(current_total.checked_sub(proposed_bytes)?)? > bounds.maximum
    {
        // Repack the cheapest stale groups first. Rewriting the entire locator
        // after a few edits would lose content-defined disk sharing. This is
        // still bounded by the same original aggregate physical I/O allowance.
        let mut groups = BTreeMap::<Fingerprint, (usize, Vec<Fingerprint>)>::new();
        for (key, item) in &proposed {
            let group = groups.entry(item.pack).or_default();
            group.0 = group.0.checked_add(item.length)?;
            group.1.push(*key);
        }
        let mut choices: Vec<_> = groups
            .into_iter()
            .filter_map(|(pack, (live, keys))| {
                let payload = old_packs.get(&pack)?.checked_sub(PACK_HEADER)?;
                let stale = payload.checked_sub(live)?;
                (stale > 0).then_some((pack, live, stale, keys))
            })
            .collect();
        choices.sort_by(|a, b| {
            ((b.2 as u128) * (a.1 as u128))
                .cmp(&((a.2 as u128) * (b.1 as u128)))
                .then_with(|| a.0.cmp(&b.0))
        });
        for (pack, live, _, keys) in choices {
            if old_bytes.checked_add(current_total.checked_sub(proposed_bytes)?)? <= bounds.maximum
            {
                break;
            }
            old_bytes =
                old_bytes.checked_sub(old_packs.remove(&pack)?.checked_sub(PACK_HEADER)?)?;
            proposed_bytes = proposed_bytes.checked_sub(live)?;
            stats.compacted_items += keys.len();
            for key in keys {
                proposed.remove(&key)?;
            }
        }
    }
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
        if context.cancellation.is_cancelled() {
            return None;
        }
        let io = publication_start(context.profile);
        let found = (|| {
            if compact {
                return None;
            }
            let item = *proposed.get(key)?;
            let record = epoch.packs.get(&item.pack)?;
            let value = checked.entry(item.pack).or_insert_with(|| {
                if record.stamp.is_some()
                    && let Some(bytes) = &record.bytes
                {
                    inspect_snapshot(
                        context.packs.root(),
                        item.pack,
                        record.length,
                        bytes,
                        bounds,
                    )
                } else {
                    inspect_pack(context.packs.root(), item.pack, record.length, bounds)
                }
            });
            let (pack_bytes, stamp) = value.as_ref()?;
            let stored = segment(pack_bytes, item)?;
            // CheckedItem already binds these immutable current bytes to key.
            // Exact equality also validates an untrusted hint's offset without
            // rehashing the same slice after checking the whole pack.
            if stored != *current.get(key)? {
                return None;
            }
            packs.insert(item.pack, record.length);
            records.entry(item.pack).or_insert_with(|| PackRecord {
                length: record.length,
                stamp: stamp.clone(),
                bytes: Some(pack_bytes.clone()),
            });
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
    let mut written_pack = None;
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
            let item = current[key];
            if context.cancellation.is_cancelled() {
                return None;
            }
            offsets.push((*key, bytes.len().checked_sub(PACK_HEADER)?, item.len()));
            bytes.extend_from_slice(item);
        }
        let pack = Fingerprint::of(&bytes);
        let bytes = Arc::new(bytes);
        stats.encode_ns += publication_elapsed(encode);
        let io = publication_start(context.profile);
        let existing = inspect_snapshot(context.packs.root(), pack, bytes.len(), &bytes, bounds);
        stats.item_io_ns += publication_elapsed(io);
        let (pack_bytes, stamp) = if let Some(verified) = existing {
            verified
        } else {
            let guard = publication_start(context.profile);
            context.packs.validate_write().ok()?;
            stats.guard_ns += publication_elapsed(guard);
            let io = publication_start(context.profile);
            ArtifactCache::with_limit(context.packs.root(), bounds.pack_maximum()?)
                .write_candidate_cancellable(pack, &bytes, context.cancellation)
                .ok()?;
            let written_stamp = stamp(&path(context.packs.root(), pack))?;
            let verified =
                inspect_snapshot(context.packs.root(), pack, bytes.len(), &bytes, bounds)?;
            stats.item_io_ns += publication_elapsed(io);
            stats.physical_pack_writes = 1;
            stats.physical_pack_bytes = usize::try_from(written_stamp.length).ok()?;
            stats.written_items = dirty.len();
            stats.written_bytes = data_len;
            written_pack = Some(pack);
            verified
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
                bytes: Some(pack_bytes),
            },
        );
    }
    stats.existing_items = logical_items.checked_sub(stats.written_items)?;
    stats.referenced_packs = packs.len();
    stats.referenced_pack_payload_bytes = packs.values().try_fold(0usize, |sum, length| {
        sum.checked_add(length.checked_sub(PACK_HEADER)?)
    })?;
    Some(Placement {
        locations,
        packs,
        records,
        written_pack,
    })
}

#[cfg(test)]
#[path = "packed_items_tests.rs"]
mod tests;

//! Segmented transport of the exact semantic-v4 raw byte stream, in a separate
//! opt-in v5 namespace. Full current decoding and independent Verify remain.
use super::*;
use std::sync::Arc;
use zil_ir::wire::{FunctionEncodingStats, RawFunctionEncoding, WireSegment};

const MANIFEST_MAGIC: &[u8; 8] = b"ZILSMS05";
const MANIFEST_HEADER: usize = 20;
const MANIFEST_ROW: usize = 40;

#[derive(Clone, Copy, Debug, Default)]
pub struct SemanticPublicationStats {
    pub serialized_functions: usize,
    pub reused_functions: usize,
    pub written_segments: usize,
    pub existing_segments: usize,
    pub written_bytes: usize,
    pub manifest_bytes: usize,
}

#[derive(Clone, Default)]
pub(crate) struct SemanticPublicationSession {
    previous: Option<Arc<TypedProgram>>,
    slots: Vec<Arc<OnceLock<RawFunctionEncoding>>>,
    published: BTreeMap<(PathBuf, Fingerprint), FileStamp>,
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
fn file_stamp(path: &Path) -> Option<FileStamp> {
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

impl SemanticPublicationSession {
    fn encode(
        &mut self,
        typed: &Arc<TypedProgram>,
        candidate: &CandidatePlan,
        limits: &Limits,
    ) -> Option<(Vec<WireSegment>, FunctionEncodingStats)> {
        metadata_budget(typed, limits)?;
        // Retain the current immutable program, not deep copies of its functions.
        // Complete raw equality includes all binding IDs, spans and contracts;
        // SharedVec pointer equality avoids walking unchanged large body arrays.
        let slots: Vec<_> = typed
            .module
            .functions
            .iter()
            .enumerate()
            .map(|(index, function)| {
                self.previous
                    .as_ref()
                    .and_then(|previous| previous.module.functions.get(index))
                    .filter(|previous| *previous == function)
                    .and_then(|_| self.slots.get(index))
                    .cloned()
                    .unwrap_or_else(|| Arc::new(OnceLock::new()))
            })
            .collect();
        let functions: Vec<_> = typed
            .module
            .functions
            .iter()
            .zip(&slots)
            .map(|(function, slot)| (function, slot.as_ref()))
            .collect();
        let mut stats = FunctionEncodingStats::default();
        let wire = zil_ir::wire::encode_module_candidate_segments(
            &typed.module,
            candidate,
            &functions,
            limits,
            &mut stats,
        )
        .ok()?;
        let wire_length = wire.iter().try_fold(0usize, |length, segment| {
            length.checked_add(segment.bytes.len())
        })?;
        let tail = encode_metadata(typed, limits)?;
        let total = 12usize.checked_add(wire_length)?.checked_add(tail.len())?;
        if total > limits.max_source_bytes {
            return None;
        }
        let mut prefix = Vec::with_capacity(12);
        prefix.extend_from_slice(MAGIC);
        prefix.extend_from_slice(&u32::try_from(wire_length).ok()?.to_le_bytes());
        let mut segments = Vec::with_capacity(wire.len() + 2);
        segments.push(WireSegment {
            fingerprint: Fingerprint::of(&prefix),
            bytes: prefix.into(),
        });
        segments.extend(wire);
        segments.push(WireSegment {
            fingerprint: Fingerprint::of(&tail),
            bytes: tail.into(),
        });
        self.previous = Some(typed.clone());
        self.slots = slots;
        Some((segments, stats))
    }
}

fn manifest(segments: &[WireSegment], limits: &Limits) -> Option<Vec<u8>> {
    let maximum_segments = limits.max_functions.checked_add(4)?;
    if segments.is_empty() || segments.len() > maximum_segments {
        return None;
    }
    let size = MANIFEST_HEADER.checked_add(segments.len().checked_mul(MANIFEST_ROW)?)?;
    if size > limits.max_source_bytes {
        return None;
    }
    let total = segments.iter().try_fold(0usize, |length, segment| {
        length.checked_add(segment.bytes.len())
    })?;
    if total > limits.max_source_bytes {
        return None;
    }
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(MANIFEST_MAGIC);
    bytes.extend_from_slice(&u64::try_from(total).ok()?.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(segments.len()).ok()?.to_le_bytes());
    for segment in segments {
        if segment.bytes.is_empty() {
            return None;
        }
        bytes.extend_from_slice(&segment.fingerprint.0);
        bytes.extend_from_slice(&u64::try_from(segment.bytes.len()).ok()?.to_le_bytes());
    }
    Some(bytes)
}

fn assemble(
    manifest: &[u8],
    limits: &Limits,
    load: &mut dyn FnMut(Fingerprint) -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    if manifest.len() > limits.max_source_bytes || manifest.get(..8)? != MANIFEST_MAGIC {
        return None;
    }
    let total = usize::try_from(u64::from_le_bytes(manifest.get(8..16)?.try_into().ok()?)).ok()?;
    let count = u32::from_le_bytes(manifest.get(16..20)?.try_into().ok()?) as usize;
    if total > limits.max_source_bytes
        || count == 0
        || count > limits.max_functions.checked_add(4)?
        || MANIFEST_HEADER.checked_add(count.checked_mul(MANIFEST_ROW)?)? != manifest.len()
    {
        return None;
    }
    let mut expected = 0usize;
    for row in manifest[MANIFEST_HEADER..].as_chunks::<MANIFEST_ROW>().0 {
        let length = usize::try_from(u64::from_le_bytes(row[32..40].try_into().ok()?)).ok()?;
        if length == 0 {
            return None;
        }
        expected = expected.checked_add(length)?;
        if expected > total {
            return None;
        }
    }
    if expected != total {
        return None;
    }
    let mut bytes = Vec::with_capacity(total);
    for row in manifest[MANIFEST_HEADER..].as_chunks::<MANIFEST_ROW>().0 {
        let fingerprint = Fingerprint(row[..32].try_into().ok()?);
        let length = usize::try_from(u64::from_le_bytes(row[32..40].try_into().ok()?)).ok()?;
        let segment = load(fingerprint)?;
        if segment.len() != length || Fingerprint::of(&segment) != fingerprint {
            return None;
        }
        bytes.extend_from_slice(&segment);
    }
    Some(bytes)
}

impl Store {
    pub(super) fn load_segments(
        &self,
        manifest: &[u8],
        options: &CompileOptions,
    ) -> Option<(TypedProgram, CandidatePlan)> {
        let root = self.root.join("items");
        if !private_directory(&root, false) {
            return None;
        }
        let cache = ArtifactCache::with_limit(&root, self.maximum);
        let bytes = assemble(manifest, &options.limits, &mut |key| match cache
            .inspect_candidate(key)
            .ok()?
        {
            CacheRead::Candidate(bytes) => Some(bytes),
            _ => None,
        })?;
        decode(&bytes, &options.limits)
    }
    pub(super) fn save_segments(
        &self,
        typed: &Arc<TypedProgram>,
        candidate: &CandidatePlan,
        options: &CompileOptions,
        plan: &artifacts::ArtifactPlan,
        publication: &mut SemanticPublicationSession,
    ) -> Option<SemanticPublicationStats> {
        if options.cancellation.is_cancelled() || plan.validate().is_err() {
            return None;
        }
        // One directory guard covers every content-addressed child. Sources
        // cannot be placed anywhere under it, including an unplanned item name.
        let mut guarded = plan.clone();
        let root = guarded
            .cache_directory("semantic-items", &self.root.join("items"))
            .ok()?;
        let (segments, encoding) = publication.encode(typed, candidate, &options.limits)?;
        let manifest = manifest(&segments, &options.limits)?;
        if options.cancellation.is_cancelled()
            || guarded.validate().is_err()
            || !private_directory(&self.root, true)
            || !private_directory(&root, true)
        {
            return None;
        }
        let cache = ArtifactCache::with_limit(&root, self.maximum);
        let mut stats = SemanticPublicationStats {
            serialized_functions: encoding.serialized_functions,
            reused_functions: encoding.reused_functions,
            ..Default::default()
        };
        let mut published = BTreeMap::new();
        for segment in segments {
            if options.cancellation.is_cancelled() {
                return None;
            }
            let path = root.join(format!("{}.candidate", segment.fingerprint));
            let key = (root.clone(), segment.fingerprint);
            let before = file_stamp(&path);
            // Known immutable items need only a metadata comparison. A first
            // encounter (or changed metadata) receives complete bounded cache
            // integrity validation, so same-sized corrupt files get repaired.
            let remembered = before
                .as_ref()
                .is_some_and(|stamp| publication.published.get(&key) == Some(stamp));
            let valid_existing = remembered
                || (before.is_some()
                    && matches!(cache.inspect_candidate(segment.fingerprint),
                Ok(CacheRead::Candidate(bytes)) if bytes.len()==segment.bytes.len() && Fingerprint::of(&bytes)==segment.fingerprint));
            if valid_existing {
                stats.existing_segments += 1;
            } else {
                if guarded.validate().is_err() {
                    return None;
                }
                cache
                    .write_candidate_cancellable(
                        segment.fingerprint,
                        &segment.bytes,
                        &options.cancellation,
                    )
                    .ok()?;
                stats.written_segments += 1;
                stats.written_bytes = stats.written_bytes.checked_add(segment.bytes.len())?;
            }
            published.insert(key, file_stamp(&path)?);
        }
        if options.cancellation.is_cancelled() || guarded.validate().is_err() {
            return None;
        }
        ArtifactCache::with_limit(&self.root, self.maximum)
            .write_candidate_cancellable(self.key, &manifest, &options.cancellation)
            .ok()?;
        publication.published = published;
        stats.manifest_bytes = manifest.len();
        Some(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    const SOURCE: &str = "pub fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} pub fn main()->i32{return f(2);}";
    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "semantic-items-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn fixture(path: &Path, text: &str) -> (Arc<TypedProgram>, CandidatePlan) {
        fs::write(path, text).unwrap();
        let (typed, candidate) = super::super::tests::raw(path);
        (Arc::new(typed), candidate)
    }
    fn flatten(segments: &[WireSegment]) -> Vec<u8> {
        segments
            .iter()
            .flat_map(|segment| segment.bytes.iter().copied())
            .collect()
    }
    #[test]
    fn segmented_semantics_preserves_legacy_bytes_and_shares_unchanged_functions() {
        let directory = directory();
        let path = directory.join("main.zil");
        let (typed, candidate) = fixture(&path, SOURCE);
        let limits = Limits::default();
        let mut publication = SemanticPublicationSession::default();
        let (cold, stats) = publication.encode(&typed, &candidate, &limits).unwrap();
        assert_eq!(stats.serialized_functions, 2);
        assert_eq!(flatten(&cold), encode(&typed, &candidate, &limits).unwrap());
        let (warm, stats) = publication.encode(&typed, &candidate, &limits).unwrap();
        assert_eq!((stats.serialized_functions, stats.reused_functions), (0, 2));
        let shared = warm
            .iter()
            .filter(|item| cold.iter().any(|old| Arc::ptr_eq(&item.bytes, &old.bytes)))
            .count();
        assert_eq!(shared, 2);
        let (changed, next_candidate) = fixture(&path, &SOURCE.replace("x+1", "x+2"));
        let (edited, stats) = publication
            .encode(&changed, &next_candidate, &limits)
            .unwrap();
        assert_eq!((stats.serialized_functions, stats.reused_functions), (1, 1));
        assert_eq!(
            flatten(&edited),
            encode(&changed, &next_candidate, &limits).unwrap()
        );
        let shared = edited
            .iter()
            .filter(|item| warm.iter().any(|old| Arc::ptr_eq(&item.bytes, &old.bytes)))
            .count();
        assert_eq!(shared, 1);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn manifest_bounds_precede_reads_and_corruption_is_a_complete_miss() {
        let directory = directory();
        let path = directory.join("main.zil");
        let (typed, candidate) = fixture(&path, SOURCE);
        let limits = Limits::default();
        let (segments, _) = SemanticPublicationSession::default()
            .encode(&typed, &candidate, &limits)
            .unwrap();
        let bytes = manifest(&segments, &limits).unwrap();
        let assembled = assemble(&bytes, &limits, &mut |key| {
            segments
                .iter()
                .find(|s| s.fingerprint == key)
                .map(|s| s.bytes.to_vec())
        })
        .unwrap();
        assert_eq!(assembled, encode(&typed, &candidate, &limits).unwrap());
        assert_eq!(
            decode(&assembled, &limits).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        let mut forged = bytes.clone();
        forged[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut reads = 0;
        assert!(
            assemble(&forged, &limits, &mut |_| {
                reads += 1;
                None
            })
            .is_none()
        );
        assert_eq!(reads, 0);
        assert!(
            assemble(&bytes, &limits, &mut |key| {
                let mut data = segments
                    .iter()
                    .find(|s| s.fingerprint == key)?
                    .bytes
                    .to_vec();
                data[0] ^= 1;
                Some(data)
            })
            .is_none()
        );
        let exact = assembled.len();
        for (maximum, accepted) in [(exact, true), (exact - 1, false)] {
            let limits = Limits {
                max_source_bytes: maximum,
                ..Limits::default()
            };
            assert_eq!(
                SemanticPublicationSession::default()
                    .encode(&typed, &candidate, &limits)
                    .is_some(),
                accepted
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn semantic_store_publishes_items_before_manifest_and_repairs_corrupt_items() {
        let directory = directory();
        let path = directory.join("main.zil");
        let (typed, candidate) = fixture(&path, SOURCE);
        let options = CompileOptions {
            producer_reuse: true,
            ..Default::default()
        };
        let source = SourceUnit::new(FileId(0), ModuleId(0), path.display().to_string(), SOURCE);
        let plan = artifacts::ArtifactPlan::new(&source, &options).unwrap();
        let store = Store {
            root: directory.join("semantic-v5"),
            key: Fingerprint::of(b"request"),
            maximum: options.limits.max_source_bytes,
            segmented: true,
        };
        let mut publication = SemanticPublicationSession::default();
        let cold = store
            .save(&typed, &candidate, &options, &plan, &mut publication)
            .unwrap();
        assert_eq!(cold.serialized_functions, 2);
        assert!(cold.written_segments > 0);
        assert_eq!(
            store.load(&options).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        let warm = store
            .save(&typed, &candidate, &options, &plan, &mut publication)
            .unwrap();
        assert_eq!(
            (
                warm.serialized_functions,
                warm.reused_functions,
                warm.written_segments
            ),
            (0, 2, 0)
        );
        let item = publication.published.keys().next().unwrap().clone();
        let item_path = item.0.join(format!("{}.candidate", item.1));
        let mut bad = fs::read(&item_path).unwrap();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        fs::write(&item_path, bad).unwrap();
        assert!(store.load(&options).is_none());
        let repaired = store
            .save(&typed, &candidate, &options, &plan, &mut publication)
            .unwrap();
        assert_eq!(repaired.written_segments, 1);
        assert_eq!(
            store.load(&options).unwrap(),
            ((*typed).clone(), candidate.clone())
        );
        let before = fs::read(store.path()).unwrap();
        options.cancellation.cancel();
        assert!(
            store
                .save(&typed, &candidate, &options, &plan, &mut publication)
                .is_none()
        );
        assert_eq!(before, fs::read(store.path()).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }
}

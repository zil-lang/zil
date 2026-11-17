//! Explicit trusted-local frontend cache. The stable key only selects a prior
//! manifest. Current source/context equality in each producer admits raw data.
use super::*;
use zil_db::{ArtifactCache, CacheRead};
use zil_expand::FrontendProducts;

type LoadedProducts = (FrontendProducts, BTreeMap<ModuleId, Arc<[u8]>>);

pub(super) struct Store {
    root: PathBuf,
    key: Fingerprint,
    maximum: usize,
    loaded: std::cell::RefCell<BTreeMap<ModuleId, Arc<[u8]>>>,
}
impl Store {
    pub(super) fn new(
        root: &SourceUnit,
        identity: &QueryKey,
        options: &CompileOptions,
        artifacts: &mut ArtifactPlan,
    ) -> Option<Self> {
        if !options.push_reuse {
            return None;
        }
        let mut identity = identity.clone();
        // Store::new supplies the existing explicit trust gate, compiler and
        // complete option identity. Source content is checked after selection.
        identity.input = Fingerprint::of(
            format!(
                "frontend-v1:{:?}:{:?}:{}",
                root.module_id, root.file_id, root.path
            )
            .as_bytes(),
        );
        let semantic = super::super::semantic_cache::Store::new(options, &identity)?;
        let path = semantic.path();
        let key = Fingerprint::of(path.file_stem()?.to_str()?.as_bytes());
        let directory = path.parent()?.parent()?.join("frontend-v1");
        let directory = artifacts
            .cache_directory("frontend-products", &directory)
            .ok()?;
        Some(Self {
            root: directory,
            key,
            maximum: options.limits.max_source_bytes,
            loaded: Default::default(),
        })
    }
    fn slot(&self, id: ModuleId) -> Fingerprint {
        let mut bytes = self.key.0.to_vec();
        bytes.extend_from_slice(&id.0.to_le_bytes());
        Fingerprint::of(&bytes)
    }
    pub(super) fn load(
        &self,
        options: &CompileOptions,
        producer: &mut zil_expand::ExpandSession,
        materializer: &mut super::super::ct_artifact::ScalarMaterializationSession,
    ) {
        if !producer.needs_frontend_recovery() {
            return;
        }
        let load = || -> Option<LoadedProducts> {
            if !private_directory(&self.root, false) || options.cancellation.is_cancelled() {
                return None;
            }
            let cache = ArtifactCache::with_limit(&self.root, self.maximum);
            let CacheRead::Candidate(manifest) = cache.inspect_candidate(self.key).ok()? else {
                return None;
            };
            let mut r = Reader(&manifest);
            if r.take(8)? != b"ZILFEC02" {
                return None;
            }
            let context = r.blob()?.to_vec();
            let count = r.number()?;
            if count > options.limits.max_functions || count > r.0.len() / 44 {
                return None;
            }
            // Charge the original logical manifest, including the complete CT
            // source bytes, before allocating a reconstructed receipt.
            let mut total = 16usize.checked_add(context.len())?;
            total = total.checked_add(count.checked_mul(44)?)?;
            let mut products = FrontendProducts {
                context,
                modules: BTreeMap::new(),
            };
            let mut ct = BTreeMap::new();
            let mut loaded = BTreeMap::new();
            for _ in 0..count {
                let id = ModuleId(u32::try_from(r.number()?).ok()?);
                let checksum = Fingerprint(r.take(32)?.try_into().ok()?);
                let length = r.number()?;
                let ct_length = r.number()?;
                let ct_record = r.blob()?;
                total = total.checked_add(length)?.checked_add(ct_length)?;
                if total > self.maximum || options.cancellation.is_cancelled() {
                    return None;
                }
                let CacheRead::Candidate(bytes) = cache.inspect_candidate(self.slot(id)).ok()?
                else {
                    continue;
                };
                if bytes.len() != length || Fingerprint::of(&bytes) != checksum {
                    continue;
                }
                let bytes: Arc<[u8]> = bytes.into();
                if products.modules.insert(id, bytes.clone()).is_some() {
                    return None;
                }
                loaded.insert(id, bytes);
                if ct_length != 0 {
                    let receipt = restore_receipt(ct_record, ct_length, products.source_text(id))?;
                    ct.insert(id, receipt.into());
                } else if !ct_record.is_empty() {
                    return None;
                }
            }
            if !r.0.is_empty() {
                return None;
            }
            *self.loaded.borrow_mut() = loaded;
            Some((products, ct))
        };
        if let Some((products, ct)) = load()
            && producer.restore_frontend_products(products, &options.limits)
        {
            materializer.restore_raw_receipts(ct);
        }
    }
    pub(super) fn save(
        &self,
        producer: &zil_expand::ExpandSession,
        materializer: &super::super::ct_artifact::ScalarMaterializationSession,
        options: &CompileOptions,
        artifacts: &ArtifactPlan,
    ) {
        if options.cancellation.is_cancelled() {
            return;
        }
        let Some(products) = producer.encode_frontend_products(&options.limits) else {
            return;
        };
        let ct = materializer.encode_raw_receipts(&options.limits);
        let _ = self.publish_products(&products, &ct, options, artifacts, &mut || {});
    }

    fn publish_products(
        &self,
        products: &FrontendProducts,
        ct: &BTreeMap<ModuleId, Arc<[u8]>>,
        options: &CompileOptions,
        artifacts: &ArtifactPlan,
        before_write: &mut dyn FnMut(),
    ) -> Option<()> {
        if options.cancellation.is_cancelled() {
            return None;
        }
        let mut manifest = Vec::new();
        manifest.extend_from_slice(b"ZILFEC02");
        blob(&mut manifest, &products.context)?;
        number(&mut manifest, products.modules.len())?;
        let mut total = products.context.len();
        let mut logical_manifest = 16usize
            .checked_add(products.context.len())?
            .checked_add(products.modules.len().checked_mul(44)?)?;
        for (id, bytes) in &products.modules {
            number(&mut manifest, id.0 as usize)?;
            manifest.extend_from_slice(&Fingerprint::of(bytes).0);
            number(&mut manifest, bytes.len())?;
            let receipt: &[u8] = ct.get(id).map_or(&[], |b| b.as_ref());
            number(&mut manifest, receipt.len())?;
            let compact = compact_receipt(receipt, products.source_text(*id))?;
            blob(&mut manifest, &compact)?;
            logical_manifest = logical_manifest.checked_add(receipt.len())?;
            total = total.checked_add(bytes.len())?;
            if total.checked_add(logical_manifest)? > self.maximum || manifest.len() > self.maximum
            {
                return None;
            }
        }
        if options.cancellation.is_cancelled()
            || artifacts.validate().is_err()
            || !private_directory(&self.root, true)
        {
            return None;
        }
        let namespace = artifacts.pin_cache_directory(&self.root).ok()?;
        let cache = ArtifactCache::with_limit(namespace.root(), self.maximum);
        for (id, bytes) in &products.modules {
            if options.cancellation.is_cancelled() {
                return None;
            }
            // Immutable bytes already read in this request are a complete
            // equality witness. A selector/digest alone cannot skip a write.
            // Do this before either full or per-write filesystem validation.
            if self
                .loaded
                .borrow()
                .get(id)
                .is_some_and(|old| Arc::ptr_eq(old, bytes) || old.as_ref() == bytes.as_ref())
            {
                continue;
            }
            // A resident request has no loaded map. Compare the actual current
            // candidate bytes before paying any per-WRITE guard in that case.
            if let Ok(CacheRead::Candidate(existing)) = cache.inspect_candidate(self.slot(*id))
                && existing.as_slice() == bytes.as_ref()
            {
                continue;
            }
            before_write();
            if options.cancellation.is_cancelled() {
                return None;
            }
            // The pinned batch checks every planned identity and ancestor at
            // each write, with full collision/canonical checks at both ends.
            // Repeating the complete all-source matrix here makes cold writes
            // quadratic just as surely as checking unchanged products does.
            namespace.validate_write().ok()?;
            // Stable module slots bound retained disk volume. A crash during
            // replacement makes the old manifest miss its checksum; the new
            // manifest is committed only after all actual products succeed.
            cache
                .write_candidate_cancellable(self.slot(*id), bytes, &options.cancellation)
                .ok()?;
        }
        if options.cancellation.is_cancelled() {
            return None;
        }
        // Always validate the complete batch at the final boundary, including
        // when every product and the complete manifest were unchanged.
        namespace.validate().ok()?;
        if !matches!(cache.inspect_candidate(self.key), Ok(CacheRead::Candidate(ref old)) if old == &manifest)
        {
            before_write();
            if options.cancellation.is_cancelled() {
                return None;
            }
            namespace.validate().ok()?;
            cache
                .write_candidate_cancellable(self.key, &manifest, &options.cancellation)
                .ok()?;
        }
        Some(())
    }
}
// The CT decoder still receives its original complete bytes. This transport
// merely factors source text already stored in the checksummed frontend slot.
// Unknown shapes use a literal; no current-source hash becomes an authority.
fn compact_receipt(bytes: &[u8], source: Option<&[u8]>) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    if let Some(source) = source
        && source.len() > 8
        && let Some((prefix, suffix)) =
            super::super::ct_artifact::ScalarMaterializationSession::split_raw_receipt(
                bytes, source,
            )
    {
        let mut out = vec![1];
        blob(&mut out, prefix)?;
        blob(&mut out, suffix)?;
        return Some(out);
    }
    let mut out = Vec::with_capacity(bytes.len().checked_add(1)?);
    out.push(0);
    out.extend_from_slice(bytes);
    Some(out)
}

fn restore_receipt(record: &[u8], length: usize, source: Option<&[u8]>) -> Option<Vec<u8>> {
    let (&tag, record) = record.split_first()?;
    match tag {
        0 => (record.len() == length).then(|| record.to_vec()),
        1 => {
            let source = source?;
            let mut reader = Reader(record);
            let prefix = reader.blob()?;
            let suffix = reader.blob()?;
            if !reader.0.is_empty()
                || prefix
                    .len()
                    .checked_add(source.len())?
                    .checked_add(suffix.len())?
                    != length
            {
                return None;
            }
            let mut out = Vec::with_capacity(length);
            out.extend_from_slice(prefix);
            out.extend_from_slice(source);
            out.extend_from_slice(suffix);
            Some(out)
        }
        _ => None,
    }
}

fn number(out: &mut Vec<u8>, n: usize) -> Option<()> {
    out.extend_from_slice(&u32::try_from(n).ok()?.to_le_bytes());
    Some(())
}
fn blob(out: &mut Vec<u8>, b: &[u8]) -> Option<()> {
    number(out, b.len())?;
    out.extend_from_slice(b);
    Some(())
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let result = self.0.get(..n)?;
        self.0 = &self.0[n..];
        Some(result)
    }
    fn number(&mut self) -> Option<usize> {
        usize::try_from(u32::from_le_bytes(self.take(4)?.try_into().ok()?)).ok()
    }
    fn blob(&mut self) -> Option<&'a [u8]> {
        let length = self.number()?;
        self.take(length)
    }
}
fn private_directory(path: &Path, create: bool) -> bool {
    if create {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        if builder.create(path).is_err() {
            return false;
        }
    }
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Fixture {
        directory: PathBuf,
        store: Store,
        artifacts: ArtifactPlan,
        options: CompileOptions,
    }
    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "zil-frontend-publication-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            let path = directory.join("main.zil");
            let text = "pub fn main()->i32=>0;";
            fs::write(&path, text).unwrap();
            let source = SourceUnit::new(FileId(0), ModuleId(0), path.display().to_string(), text);
            let options = CompileOptions::default();
            let mut artifacts = ArtifactPlan::new(&source, &options).unwrap();
            let root = artifacts
                .cache_directory("frontend-products", &directory.join("cache"))
                .unwrap();
            Self {
                directory,
                artifacts,
                options,
                store: Store {
                    root,
                    key: Fingerprint::of(b"frontend-publication-test"),
                    maximum: Limits::default().max_source_bytes,
                    loaded: Default::default(),
                },
            }
        }
        fn products(&self) -> FrontendProducts {
            FrontendProducts {
                context: b"current exact namespace".to_vec(),
                modules: (0u32..102)
                    .map(|id| (ModuleId(id), Arc::<[u8]>::from(id.to_le_bytes().as_slice())))
                    .collect(),
            }
        }
        fn publish(&self, products: &FrontendProducts) -> usize {
            let mut guards = 0;
            self.store
                .publish_products(
                    products,
                    &BTreeMap::new(),
                    &self.options,
                    &self.artifacts,
                    &mut || {
                        guards += 1;
                    },
                )
                .unwrap();
            guards
        }
        fn manifest(&self, root: &Path) -> Vec<u8> {
            let CacheRead::Candidate(bytes) = ArtifactCache::with_limit(root, self.store.maximum)
                .inspect_candidate(self.store.key)
                .unwrap()
            else {
                panic!("current manifest");
            };
            bytes
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn factored_ct_source_restores_exact_original_receipt_and_rejects_bad_lengths() {
        let source = b"pub fn main()->i32=>42;";
        let path = b"main.zil";
        let mut receipt = b"ZILCTMR1".to_vec();
        for value in [
            64u64,
            0,
            0,
            17,
            0,
            0,
            0,
            path.len() as u64,
            source.len() as u64,
        ] {
            receipt.extend_from_slice(&value.to_le_bytes());
        }
        receipt.extend_from_slice(path);
        receipt.extend_from_slice(source);
        receipt.extend_from_slice(b"complete placement bytes");
        let compact = compact_receipt(&receipt, Some(source)).unwrap();
        assert_eq!(compact[0], 1);
        assert!(compact.len() < receipt.len());
        assert_eq!(
            restore_receipt(&compact, receipt.len(), Some(source)).unwrap(),
            receipt
        );
        assert!(restore_receipt(&compact, receipt.len() - 1, Some(source)).is_none());
        assert!(restore_receipt(&compact, receipt.len() + 1, Some(source)).is_none());
        assert!(restore_receipt(&compact, receipt.len(), None).is_none());
        let mut trailing = compact.clone();
        trailing.push(0);
        assert!(restore_receipt(&trailing, receipt.len(), Some(source)).is_none());
        for end in 0..compact.len() {
            assert!(restore_receipt(&compact[..end], receipt.len(), Some(source)).is_none());
        }
        // Unknown/mismatched source cannot factor or silently replace bytes.
        let literal = compact_receipt(&receipt, Some(b"different text")).unwrap();
        assert_eq!(literal[0], 0);
        assert_eq!(
            restore_receipt(&literal, receipt.len(), None).unwrap(),
            receipt
        );
        let mut overflow = compact;
        overflow[1..5].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(restore_receipt(&overflow, receipt.len(), Some(source)).is_none());
    }

    #[test]
    fn compact_receipts_do_not_relax_the_original_logical_cache_bound() {
        let mut fixture = Fixture::new();
        let mut products = fixture.products();
        products.modules.retain(|id, _| id.0 == 0);
        fixture.publish(&products);
        let mut manifest = fixture.manifest(&fixture.store.root);
        let length_offset = 16 + products.context.len() + 40;
        manifest[length_offset..length_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        ArtifactCache::with_limit(&fixture.store.root, fixture.store.maximum)
            .write_candidate_cancellable(
                fixture.store.key,
                &manifest,
                &fixture.options.cancellation,
            )
            .unwrap();
        fixture.store.maximum = manifest.len() + 4;
        let mut producer = zil_expand::ExpandSession::default();
        let mut materializer =
            super::super::super::ct_artifact::ScalarMaterializationSession::default();
        fixture
            .store
            .load(&fixture.options, &mut producer, &mut materializer);
        assert!(fixture.store.loaded.borrow().is_empty());
    }

    #[test]
    fn unchanged_products_skip_per_write_guards_for_loaded_and_resident_publications() {
        let fixture = Fixture::new();
        let mut products = fixture.products();
        assert_eq!(fixture.publish(&products), 103);
        let before = fixture.manifest(&fixture.store.root);
        // The resident path has no disk-loaded map; actual candidate equality
        // must still skip both the per-WRITE guard and the filesystem rewrite.
        assert!(fixture.store.loaded.borrow().is_empty());
        assert_eq!(fixture.publish(&products), 0);
        assert_eq!(fixture.manifest(&fixture.store.root), before);
        *fixture.store.loaded.borrow_mut() = products.modules.clone();
        assert_eq!(fixture.publish(&products), 0);
        products
            .modules
            .insert(ModuleId(7), Arc::from(&b"one changed module"[..]));
        assert_eq!(
            fixture.publish(&products),
            2,
            "one product plus the final manifest"
        );
        assert_ne!(fixture.manifest(&fixture.store.root), before);
    }

    #[test]
    fn cancelled_manifest_keeps_previous_bytes_and_checksum_misses_only_the_changed_product() {
        let fixture = Fixture::new();
        let original = fixture.products();
        fixture.publish(&original);
        *fixture.store.loaded.borrow_mut() = original.modules.clone();
        let before = fixture.manifest(&fixture.store.root);
        let mut changed = original;
        changed
            .modules
            .insert(ModuleId(7), Arc::from(&b"changed raw bytes"[..]));
        let mut options = fixture.options.clone();
        options.cancellation = CancellationToken::default();
        let mut writes = 0;
        assert!(
            fixture
                .store
                .publish_products(
                    &changed,
                    &BTreeMap::new(),
                    &options,
                    &fixture.artifacts,
                    &mut || {
                        writes += 1;
                        if writes == 2 {
                            options.cancellation.cancel();
                        }
                    }
                )
                .is_none()
        );
        assert_eq!(writes, 2);
        assert_eq!(fixture.manifest(&fixture.store.root), before);
        let mut producer = zil_expand::ExpandSession::default();
        let mut materializer =
            super::super::super::ct_artifact::ScalarMaterializationSession::default();
        // The previous manifest cannot authenticate the changed stable slot.
        // Unaffected actual bytes are still usable as raw recovery candidates.
        let mut retry_options = fixture.options.clone();
        retry_options.cancellation = CancellationToken::default();
        fixture
            .store
            .load(&retry_options, &mut producer, &mut materializer);
        assert_eq!(fixture.store.loaded.borrow().len(), 101);
        assert!(!fixture.store.loaded.borrow().contains_key(&ModuleId(7)));
        assert_eq!(
            fixture.publish(&changed),
            1,
            "retry writes only the missing manifest"
        );
        assert_ne!(fixture.manifest(&fixture.store.root), before);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn replaced_namespace_cannot_receive_a_product_or_manifest_after_unchanged_skips() {
        for manifest_only in [false, true] {
            let fixture = Fixture::new();
            let original = fixture.products();
            fixture.publish(&original);
            *fixture.store.loaded.borrow_mut() = original.modules.clone();
            let before = fixture.manifest(&fixture.store.root);
            let mut changed = original;
            if manifest_only {
                changed.context.push(1);
            } else {
                changed
                    .modules
                    .insert(ModuleId(101), Arc::from(&b"changed module"[..]));
            }
            let held = fixture.directory.join("held-cache");
            let mut writes = 0;
            assert!(
                fixture
                    .store
                    .publish_products(
                        &changed,
                        &BTreeMap::new(),
                        &fixture.options,
                        &fixture.artifacts,
                        &mut || {
                            writes += 1;
                            fs::rename(&fixture.store.root, &held).unwrap();
                            fs::create_dir(&fixture.store.root).unwrap();
                        }
                    )
                    .is_none()
            );
            assert_eq!(writes, 1);
            assert_eq!(fixture.manifest(&held), before);
            assert_eq!(fs::read_dir(&fixture.store.root).unwrap().count(), 0);
        }
    }
}

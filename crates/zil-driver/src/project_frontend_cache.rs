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
            if r.take(8)? != b"ZILFEC01" {
                return None;
            }
            let context = r.blob()?.to_vec();
            let count = r.number()?;
            if count > options.limits.max_functions || count > r.0.len() / 44 {
                return None;
            }
            let mut total = manifest.len();
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
                let ct_bytes: Arc<[u8]> = r.blob()?.into();
                total = total.checked_add(length)?;
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
                if !ct_bytes.is_empty() {
                    ct.insert(id, ct_bytes);
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
        let publish = || -> Option<()> {
            if options.cancellation.is_cancelled() {
                return None;
            }
            let products = producer.encode_frontend_products(&options.limits)?;
            let ct = materializer.encode_raw_receipts(&options.limits);
            let mut manifest = Vec::new();
            manifest.extend_from_slice(b"ZILFEC01");
            blob(&mut manifest, &products.context)?;
            number(&mut manifest, products.modules.len())?;
            let mut total = products.context.len();
            for (id, bytes) in &products.modules {
                number(&mut manifest, id.0 as usize)?;
                manifest.extend_from_slice(&Fingerprint::of(bytes).0);
                number(&mut manifest, bytes.len())?;
                blob(&mut manifest, ct.get(id).map_or(&[], |b| b.as_ref()))?;
                total = total.checked_add(bytes.len())?;
                if total.checked_add(manifest.len())? > self.maximum {
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
                if options.cancellation.is_cancelled() || artifacts.validate().is_err() {
                    return None;
                }
                if self
                    .loaded
                    .borrow()
                    .get(id)
                    .is_some_and(|old| old.as_ref() == bytes.as_ref())
                {
                    continue;
                }
                // Stable per-module slots bound retained disk volume. A crash
                // during replacement makes the old manifest miss by checksum;
                // the manifest is committed only after every product succeeds.
                namespace.validate_write().ok()?;
                // A resident request constructs a new Store without disk load.
                // Compare complete current candidate bytes before overwriting;
                // neither the slot nor its checksum alone grants this reuse.
                if let Ok(CacheRead::Candidate(existing)) = cache.inspect_candidate(self.slot(*id))
                    && existing.as_slice() == bytes.as_ref()
                {
                    continue;
                }
                cache
                    .write_candidate_cancellable(self.slot(*id), bytes, &options.cancellation)
                    .ok()?;
            }
            if options.cancellation.is_cancelled() || artifacts.validate().is_err() {
                return None;
            }
            namespace.validate().ok()?;
            if !matches!(cache.inspect_candidate(self.key), Ok(CacheRead::Candidate(ref old)) if old == &manifest)
            {
                cache
                    .write_candidate_cancellable(self.key, &manifest, &options.cancellation)
                    .ok()?;
            }
            Some(())
        };
        let _ = publish();
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

//! Bounded, trusted-local import discovery inputs; never verification authority.
//! Every restored file is read now and every import is resolved now.
use super::*;
use std::sync::OnceLock;
use zil_db::{ArtifactCache, CacheRead};

const MAGIC: &[u8; 8] = b"ZILDISC1";
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_RECORDS: usize = 65_536;
static COMPILER: OnceLock<Option<Fingerprint>> = OnceLock::new();

struct Store {
    root: PathBuf,
    key: Fingerprint,
    maximum: usize,
}
impl Store {
    fn new(source: &SourceUnit, options: &CompileOptions) -> Option<Self> {
        if options.profile != ExecutionProfile::Native
            || options.closed_executable
            || options.stop_after < Stage::Verify
            || options.emit_dir.is_some()
            || options.cache_dir.is_some()
            || ["LD_PRELOAD", "LD_AUDIT"]
                .into_iter()
                .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        {
            return None;
        }
        let root = options.semantic_cache.as_ref()?.join("discovery-v1");
        let compiler = COMPILER
            .get_or_init(|| zil_backend_llvm::installed_compiler_identity().ok())
            .as_ref()?;
        let mut identity = b"ZIL-TRUSTED-DISCOVERY-v1\0".to_vec();
        identity.extend_from_slice(
            &query_key(source, options, Phase::Types, true)
                .fingerprint()
                .0,
        );
        identity.extend_from_slice(&compiler.0);
        Some(Self {
            root,
            key: Fingerprint::of(&identity),
            maximum: MAX_BYTES.min(options.limits.max_source_bytes),
        })
    }
    fn path(&self) -> PathBuf {
        self.root.join(format!("{}.candidate", self.key))
    }
    fn bind(&mut self, path: &Path) -> Option<()> {
        self.root = path.parent()?.to_path_buf();
        Some(())
    }
}

struct Import {
    path: Vec<String>,
    span: Span,
}
struct Module {
    path: String,
    fingerprint: Fingerprint,
    tokens: usize,
    imports: Vec<Import>,
}
struct Receipt {
    modules: Vec<Module>,
}

pub(super) struct Pending {
    store: Store,
    bytes: Vec<u8>,
}
impl Pending {
    pub(super) fn new(
        project: &ProjectSources,
        syntax: &[LoadedSyntax],
        options: &CompileOptions,
    ) -> Option<Self> {
        // Single import-free files already use the protocol-free Stage14 path.
        if project.modules.len() < 2 || project.modules.len() != syntax.len() {
            return None;
        }
        let root = project.modules.first()?;
        let store = Store::new(root, options)?;
        let mut writer = Writer {
            bytes: Vec::new(),
            maximum: store.maximum,
        };
        writer.put(MAGIC)?;
        writer.number(project.modules.len())?;
        if project.modules.len() > MAX_RECORDS {
            return None;
        }
        let mut imports_left = MAX_RECORDS.min(options.limits.max_nodes);
        for (index, (source, syntax)) in project.modules.iter().zip(syntax).enumerate() {
            if source.module_id.0 as usize != index || source.file_id.0 as usize != index {
                return None;
            }
            // Lossy SourceUnit path spellings cannot stand for a different path.
            let canonical = fs::canonicalize(&source.path).ok()?;
            if canonical.to_str()? != source.path {
                return None;
            }
            writer.string(&source.path)?;
            writer.put(&source.fingerprint().0)?;
            writer.number(syntax.parsed.tokens.len())?;
            let imports = syntax
                .parsed
                .module
                .items
                .iter()
                .filter_map(|item| match &item.kind {
                    ast::ItemKind::Import(import) => Some((item, import)),
                    _ => None,
                });
            let count = imports.clone().count();
            imports_left = imports_left.checked_sub(count)?;
            writer.number(count)?;
            for (item, import) in imports {
                if unsupported_import(item, import).is_err() || !item.span.is_valid_for(source) {
                    return None;
                }
                writer.number(item.span.start)?;
                writer.number(item.span.end)?;
                writer.number(import.path.len())?;
                for segment in &import.path {
                    writer.string(segment)?;
                }
            }
        }
        Some(Self {
            store,
            bytes: writer.bytes,
        })
    }

    pub(super) fn publish(mut self, project: &ProjectSources, options: &CompileOptions) {
        let mut publish = || -> Option<()> {
            if options.cancellation.is_cancelled() {
                return None;
            }
            let root = validate_sources(project, &options.limits).ok()?;
            let key = project_key(
                root,
                options,
                project_fingerprint(project),
                Phase::Types,
                true,
            );
            // A source shape too large/unsupported for the existing semantic
            // cache gains nothing from a receipt that can never restore it.
            let semantic = super::super::semantic_cache::Store::new(options, &key)?;
            let metadata = fs::symlink_metadata(semantic.path()).ok()?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return None;
            }
            let mut plan = ArtifactPlan::project(root, &project.modules, options).ok()?;
            self.store
                .bind(&plan.semantic_cache(&self.store.path()).ok()?)?;
            if !private_directory(&self.store.root, true)
                || options.cancellation.is_cancelled()
                || plan.validate().is_err()
            {
                return None;
            }
            ArtifactCache::with_limit(&self.store.root, self.store.maximum)
                .write_candidate_cancellable(self.store.key, &self.bytes, &options.cancellation)
                .ok()?;
            Some(())
        };
        let _ = publish();
    }
}

pub(super) struct Attempt {
    pub(super) project: ProjectSources,
    pub(super) complete: bool,
}

pub(super) fn load(
    project: &ProjectSources,
    root_path: &Path,
    options: &CompileOptions,
    session: &CompilationSession,
) -> CompileResult<Option<Attempt>> {
    if session.active_query_count() != 0 || root_path.to_str().is_none() {
        return Ok(None);
    }
    let Some(mut store) = Store::new(&project.modules[0], options) else {
        return Ok(None);
    };
    let receipt = (|| {
        let root = validate_sources(project, &options.limits).ok()?;
        let mut plan = ArtifactPlan::project(root, &project.modules, options).ok()?;
        store.bind(&plan.semantic_cache(&store.path()).ok()?)?;
        if !private_directory(&store.root, false) {
            return None;
        }
        let path = store.path();
        let metadata = fs::symlink_metadata(&path).ok()?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return None;
        }
        let CacheRead::Candidate(bytes) = ArtifactCache::with_limit(&store.root, store.maximum)
            .inspect_candidate(store.key)
            .ok()?
        else {
            return None;
        };
        decode(&bytes, &options.limits)
    })();
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    let mut recovered = project.clone();
    let complete = replay(&receipt, &mut recovered, root_path, options).is_some();
    // Optional mismatches fall back, but cancellation must not restart parsing.
    options
        .cancellation
        .check(Phase::Parse)
        .map_err(|error| vec![error])?;
    Ok(Some(Attempt {
        project: recovered,
        complete,
    }))
}

fn replay(
    receipt: &Receipt,
    project: &mut ProjectSources,
    root_path: &Path,
    options: &CompileOptions,
) -> Option<()> {
    let directory = root_path.parent()?;
    let mut remaining = options.limits.clone();
    remaining.max_source_bytes = remaining
        .max_source_bytes
        .checked_sub(project.modules[0].text.len())?;
    let mut known = BTreeMap::from([(root_path.to_path_buf(), ModuleId(0))]);
    for (index, cached) in receipt.modules.iter().enumerate() {
        options.cancellation.check(Phase::Parse).ok()?;
        let source = project.modules.get(index)?;
        if source.path != cached.path || source.fingerprint() != cached.fingerprint {
            return None;
        }
        remaining.max_tokens = remaining.max_tokens.checked_sub(cached.tokens)?;
        remaining.max_nodes = remaining.max_nodes.checked_sub(cached.tokens)?;
        for import in &cached.imports {
            if !import.span.is_valid_for(&project.modules[index]) {
                return None;
            }
            // Resolve every edge now, including paths/aliases seen earlier.
            let canonical = resolve_file_module(directory, &import.path, import.span).ok()?;
            let canonical_text = canonical.to_str()?;
            let id = if let Some(&id) = known.get(&canonical) {
                id
            } else {
                if project.modules.len() >= options.limits.max_nodes {
                    return None;
                }
                let next = project.modules.len();
                if receipt.modules.get(next)?.path != canonical_text {
                    return None;
                }
                let mut read_options = options.clone();
                read_options.limits.max_source_bytes = remaining.max_source_bytes;
                let mut source = read_source(&canonical, &read_options).ok()?;
                remaining.max_source_bytes =
                    remaining.max_source_bytes.checked_sub(source.text.len())?;
                source.file_id = FileId(u32::try_from(next).ok()?);
                source.module_id = ModuleId(u32::try_from(next).ok()?);
                let id = source.module_id;
                known.insert(canonical, id);
                // Retain every successful immutable read for ordinary fallback,
                // even if its content no longer matches the old receipt.
                project.modules.push(source);
                id
            };
            project.paths.insert(import.path.clone(), id);
        }
    }
    (project.modules.len() == receipt.modules.len()).then_some(())
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
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return false;
        }
    }
    true
}

struct Writer {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Writer {
    fn put(&mut self, bytes: &[u8]) -> Option<()> {
        if self.bytes.len().checked_add(bytes.len())? > self.maximum {
            return None;
        }
        self.bytes.extend_from_slice(bytes);
        Some(())
    }
    fn number(&mut self, number: usize) -> Option<()> {
        self.put(&u64::try_from(number).ok()?.to_le_bytes())
    }
    fn string(&mut self, text: &str) -> Option<()> {
        self.number(text.len())?;
        self.put(text.as_bytes())
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, size: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.bytes.split_at_checked(size)?;
        self.bytes = tail;
        Some(head)
    }
    fn number(&mut self) -> Option<usize> {
        usize::try_from(u64::from_le_bytes(self.take(8)?.try_into().ok()?)).ok()
    }
    fn count(&mut self, maximum: usize, minimum_bytes: usize) -> Option<usize> {
        let count = self.number()?;
        (count <= maximum && count <= self.bytes.len() / minimum_bytes).then_some(count)
    }
    fn string(&mut self) -> Option<String> {
        let length = self.number()?;
        Some(std::str::from_utf8(self.take(length)?).ok()?.to_owned())
    }
}
fn decode(bytes: &[u8], limits: &Limits) -> Option<Receipt> {
    if bytes.len() > MAX_BYTES.min(limits.max_source_bytes) {
        return None;
    }
    let mut reader = Reader { bytes };
    if reader.take(8)? != MAGIC {
        return None;
    }
    let count = reader.count(MAX_RECORDS.min(limits.max_nodes), 56)?;
    if count < 2 {
        return None;
    }
    let mut modules = Vec::with_capacity(count);
    let mut imports_left = MAX_RECORDS.min(limits.max_nodes);
    let mut tokens_left = limits.max_tokens.min(limits.max_nodes);
    for index in 0..count {
        let path = reader.string()?;
        if !Path::new(&path).is_absolute() {
            return None;
        }
        let fingerprint = Fingerprint(reader.take(32)?.try_into().ok()?);
        let tokens = reader.number()?;
        if tokens == 0 {
            return None;
        }
        tokens_left = tokens_left.checked_sub(tokens)?;
        let imports_count = reader.count(imports_left, 32)?;
        imports_left -= imports_count;
        let mut imports = Vec::with_capacity(imports_count);
        for _ in 0..imports_count {
            let start = reader.number()?;
            let end = reader.number()?;
            if start > end || end > limits.max_source_bytes {
                return None;
            }
            let segments = reader.count(limits.max_call_depth.min(MAX_RECORDS), 9)?;
            if segments == 0 {
                return None;
            }
            let mut logical = Vec::with_capacity(segments);
            for _ in 0..segments {
                let segment = reader.string()?;
                if !valid_segment(&segment) {
                    return None;
                }
                logical.push(segment);
            }
            imports.push(Import {
                path: logical,
                span: Span::new(FileId(u32::try_from(index).ok()?), start, end),
            });
        }
        modules.push(Module {
            path,
            fingerprint,
            tokens,
            imports,
        });
    }
    reader.bytes.is_empty().then_some(Receipt { modules })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn exact_multifile_receipt_returns_semantic_variant_without_discovery_parse() {
        let directory = std::env::temp_dir().join(format!(
            "zil-discovery-mechanism-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("main.zil");
        fs::write(&path, "import leaf\nfn main()->i32=>leaf.answer()").unwrap();
        fs::write(directory.join("leaf.zil"), "pub fn answer()->i32=>42").unwrap();
        let options = CompileOptions {
            semantic_cache: Some(directory.join("cache")),
            ..Default::default()
        };
        let cold = CompilationSession::default().compile_path(&path, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        let mut project = ProjectSources {
            root: ModuleId(0),
            paths: BTreeMap::new(),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "placeholder", "")],
        };
        let loaded = load_sources(
            &mut project,
            &path,
            &options,
            &CompilationSession::default(),
        )
        .unwrap();
        assert_eq!(project.modules.len(), 2);
        assert!(
            matches!(loaded, LoadedProject::Semantic(_)),
            "the ordinary loader would return parsed syntax"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn discovery_decoder_rejects_lengths_before_allocation_and_keeps_exact_bounds() {
        let limits = Limits::default();
        let mut excessive = MAGIC.to_vec();
        excessive.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode(&excessive, &limits).is_none());
        let mut short = MAGIC.to_vec();
        short.extend_from_slice(&2u64.to_le_bytes());
        short.extend_from_slice(&u64::MAX.to_le_bytes());
        short.resize(160, 0);
        assert!(decode(&short, &limits).is_none());
        let source = SourceUnit::new(FileId(0), ModuleId(0), "/source.zil", "fn main()->i32=>0");
        let module = |writer: &mut Writer| {
            writer.string(&source.path).unwrap();
            writer.put(&source.fingerprint().0).unwrap();
            writer.number(1).unwrap();
            writer.number(0).unwrap();
        };
        let mut writer = Writer {
            bytes: Vec::new(),
            maximum: 1024,
        };
        writer.put(MAGIC).unwrap();
        writer.number(2).unwrap();
        module(&mut writer);
        module(&mut writer);
        assert!(decode(&writer.bytes, &limits).is_some());
        let mut low = limits.clone();
        low.max_nodes = 1;
        assert!(decode(&writer.bytes, &low).is_none());
        low = limits;
        low.max_source_bytes = writer.bytes.len() - 1;
        assert!(decode(&writer.bytes, &low).is_none());
        writer.bytes.push(0);
        assert!(decode(&writer.bytes, &Limits::default()).is_none());
    }
}

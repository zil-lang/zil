//! Strict local project manifests. A lock binds exact manifest and source bytes;
//! the compiler still reads and verifies each imported source as a SourceUnit.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use zil_base::FingerprintBuilder;

#[derive(Clone, Debug)]
pub struct ProjectManifest {
    pub path: PathBuf,
    pub entry: PathBuf,
    pub tests: Vec<PathBuf>,
    pub fingerprint: Fingerprint,
    packages: Vec<Package>,
    locked_sources: BTreeMap<PathBuf, Fingerprint>,
}
#[derive(Clone, Debug)]
struct Package {
    directory: PathBuf,
    entry: PathBuf,
    dependencies: BTreeMap<String, usize>,
    manifest: Vec<u8>,
}
fn error(code: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Driver, code, None, message)]
}
fn read_bounded(path: &Path, remaining: &mut usize, code: &str) -> CompileResult<Vec<u8>> {
    let metadata =
        fs::metadata(path).map_err(|e| error(code, format!("{}: {e}", path.display())))?;
    if metadata.len() > *remaining as u64 {
        return Err(vec![Diagnostic::limit(
            Phase::Driver,
            "project manifest/dependency byte budget exceeded",
            None,
        )]);
    }
    let file = File::open(path).map_err(|e| error(code, format!("{}: {e}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(remaining.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| error(code, format!("{}: {e}", path.display())))?;
    *remaining = remaining.checked_sub(bytes.len()).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Driver,
            "project manifest/dependency byte budget exceeded",
            None,
        )]
    })?;
    Ok(bytes)
}
fn quoted(value: &str) -> Option<&str> {
    value
        .strip_prefix('"')?
        .strip_suffix('"')
        .filter(|s| !s.contains(['"', '\\', '\n']))
}
fn local_file(directory: &Path, value: &str) -> CompileResult<PathBuf> {
    let relative = Path::new(value);
    if relative.is_absolute() || value.is_empty() {
        return Err(error(
            "E-PROJECT-PATH",
            "entry and test files must be relative paths",
        ));
    }
    let path = fs::canonicalize(directory.join(relative))
        .map_err(|e| error("E-PROJECT-PATH", format!("{value}: {e}")))?;
    if !path.starts_with(directory)
        || !path.is_file()
        || path.extension().is_none_or(|s| s != "zil")
    {
        return Err(error(
            "E-PROJECT-PATH",
            format!(
                "{} must be a .zil file inside {}",
                path.display(),
                directory.display()
            ),
        ));
    }
    Ok(path)
}
impl ProjectManifest {
    pub fn open(path: &Path, limits: &Limits) -> CompileResult<Self> {
        let manifest = if path.is_dir() {
            path.join("Zil.toml")
        } else {
            path.to_path_buf()
        };
        let manifest = fs::canonicalize(&manifest)
            .map_err(|e| error("E-PROJECT-MANIFEST", format!("{}: {e}", manifest.display())))?;
        if manifest.file_name().is_none_or(|name| name != "Zil.toml") {
            return Err(error(
                "E-PROJECT-MANIFEST",
                "project manifest must be named Zil.toml",
            ));
        }
        let mut packages = Vec::new();
        let mut known = BTreeMap::new();
        let mut active = BTreeSet::new();
        let mut remaining = limits.max_source_bytes;
        Self::load_package(
            &manifest,
            limits,
            &mut remaining,
            &mut packages,
            &mut known,
            &mut active,
        )?;
        let text = std::str::from_utf8(&packages[0].manifest)
            .map_err(|_| error("E-PROJECT-MANIFEST", "manifest is not UTF-8"))?;
        let (_, test_names, _) = parse_manifest(text)?;
        let tests = test_names
            .iter()
            .map(|name| local_file(&packages[0].directory, name))
            .collect::<CompileResult<Vec<_>>>()?;
        let mut unique_tests = BTreeSet::new();
        for source in &tests {
            if source == &packages[0].entry || !unique_tests.insert(source) {
                return Err(error(
                    "E-PROJECT-MANIFEST",
                    "test sources must be distinct from each other and the root",
                ));
            }
        }
        let (fingerprint, locked_sources) =
            Self::content_fingerprint(&packages, limits, remaining)?;
        Ok(Self {
            path: manifest,
            entry: packages[0].entry.clone(),
            tests,
            fingerprint,
            packages,
            locked_sources,
        })
    }
    fn load_package(
        path: &Path,
        limits: &Limits,
        remaining: &mut usize,
        packages: &mut Vec<Package>,
        known: &mut BTreeMap<PathBuf, usize>,
        active: &mut BTreeSet<PathBuf>,
    ) -> CompileResult<usize> {
        let path = fs::canonicalize(path)
            .map_err(|e| error("E-PROJECT-DEPENDENCY", format!("{}: {e}", path.display())))?;
        if active.contains(&path) {
            return Err(error(
                "E-PROJECT-CYCLE",
                format!("dependency cycle at {}", path.display()),
            ));
        }
        if let Some(&index) = known.get(&path) {
            return Ok(index);
        }
        if packages.len() >= limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Driver,
                "project dependency depth budget exceeded",
                None,
            )]);
        }
        let manifest = read_bounded(&path, remaining, "E-PROJECT-MANIFEST")?;
        let text = std::str::from_utf8(&manifest)
            .map_err(|_| error("E-PROJECT-MANIFEST", "manifest is not UTF-8"))?;
        let (entry_name, _, dependencies) = parse_manifest(text)?;
        let directory = path
            .parent()
            .ok_or_else(|| error("E-PROJECT-PATH", "manifest has no parent"))?
            .to_path_buf();
        let entry = local_file(&directory, &entry_name)?;
        let index = packages.len();
        packages.push(Package {
            directory: directory.clone(),
            entry,
            dependencies: BTreeMap::new(),
            manifest,
        });
        known.insert(path.clone(), index);
        active.insert(path.clone());
        for (alias, relative) in dependencies {
            if alias == "std" || !super::project::valid_segment(&alias) {
                return Err(error(
                    "E-PROJECT-DEPENDENCY",
                    format!("invalid dependency alias {alias}"),
                ));
            }
            let candidate = directory.join(relative);
            let dependency_dir = fs::canonicalize(&candidate).map_err(|e| {
                error(
                    "E-PROJECT-DEPENDENCY",
                    format!("{}: {e}", candidate.display()),
                )
            })?;
            if !dependency_dir.is_dir() {
                return Err(error(
                    "E-PROJECT-DEPENDENCY",
                    format!("{} is not a directory", dependency_dir.display()),
                ));
            }
            let dependency = Self::load_package(
                &dependency_dir.join("Zil.toml"),
                limits,
                remaining,
                packages,
                known,
                active,
            )?;
            if packages[index]
                .dependencies
                .insert(alias.clone(), dependency)
                .is_some()
            {
                return Err(error(
                    "E-PROJECT-DEPENDENCY",
                    format!("duplicate dependency alias {alias}"),
                ));
            }
        }
        active.remove(&path);
        Ok(index)
    }
    fn content_fingerprint(
        packages: &[Package],
        limits: &Limits,
        mut remaining: usize,
    ) -> CompileResult<(Fingerprint, BTreeMap<PathBuf, Fingerprint>)> {
        let mut builder = FingerprintBuilder::new();
        builder.update(b"zil-project-lock-v1\0");
        let mut locked_sources = BTreeMap::new();
        let mut count = 0usize;
        for (index, package) in packages.iter().enumerate() {
            builder.update(&(index as u64).to_le_bytes());
            builder.update(&(package.manifest.len() as u64).to_le_bytes());
            builder.update(&package.manifest);
            // The root package is the developer's working source. Its bytes
            // stay in the compiler's source graph/cache key, not the dependency
            // lock that must survive ordinary edits to main and tests.
            if index == 0 {
                continue;
            }
            let mut pending = vec![package.directory.clone()];
            while let Some(directory) = pending.pop() {
                let directory_entries = fs::read_dir(&directory).map_err(|e| {
                    error("E-PROJECT-READ", format!("{}: {e}", directory.display()))
                })?;
                let mut entries: Vec<fs::DirEntry> = Vec::new();
                for entry in directory_entries {
                    count = count.checked_add(1).ok_or_else(|| {
                        vec![Diagnostic::limit(
                            Phase::Driver,
                            "project file budget exceeded",
                            None,
                        )]
                    })?;
                    if count > limits.max_nodes {
                        return Err(vec![Diagnostic::limit(
                            Phase::Driver,
                            "project file budget exceeded",
                            None,
                        )]);
                    }
                    entries.push(entry.map_err(|e| error("E-PROJECT-READ", e.to_string()))?);
                }
                entries.sort_by_key(|entry| entry.file_name());
                for entry in entries {
                    let kind = entry
                        .file_type()
                        .map_err(|e| error("E-PROJECT-READ", e.to_string()))?;
                    if kind.is_symlink() {
                        return Err(error(
                            "E-PROJECT-PATH",
                            format!("symlink in project source tree: {}", entry.path().display()),
                        ));
                    }
                    if kind.is_dir() {
                        if entry.file_name() != ".git" {
                            pending.push(entry.path());
                        }
                    } else if kind.is_file()
                        && entry.path().extension().is_some_and(|ext| ext == "zil")
                    {
                        let entry_path = entry.path();
                        let data = read_bounded(&entry_path, &mut remaining, "E-PROJECT-READ")?;
                        let relative = entry_path
                            .strip_prefix(&package.directory)
                            .expect("tree entry");
                        let relative = relative.to_str().ok_or_else(|| {
                            error("E-PROJECT-PATH", "project source paths must be UTF-8")
                        })?;
                        builder.update(&(relative.len() as u64).to_le_bytes());
                        builder.update(relative.as_bytes());
                        builder.update(&(data.len() as u64).to_le_bytes());
                        builder.update(&data);
                        locked_sources.insert(entry_path, Fingerprint::of(&data));
                    }
                }
            }
        }
        Ok((builder.digest(), locked_sources))
    }
    pub(super) fn check_source(&self, source: &SourceUnit) -> CompileResult<()> {
        if let Some(expected) = self.locked_sources.get(Path::new(&source.path)) {
            if source.fingerprint() != *expected {
                return Err(error(
                    "E-PROJECT-LOCK-MISMATCH",
                    format!("dependency source changed while loading: {}", source.path),
                ));
            }
        }
        Ok(())
    }
    pub(super) fn check_entry(&self, path: &Path) -> CompileResult<PathBuf> {
        let canonical = fs::canonicalize(path)
            .map_err(|e| error("E-PROJECT-PATH", format!("{}: {e}", path.display())))?;
        if canonical != self.entry && !self.tests.contains(&canonical) {
            return Err(error(
                "E-PROJECT-PATH",
                "entry is not the declared root or a listed test source",
            ));
        }
        Ok(canonical)
    }
    pub fn lock_path(&self) -> PathBuf {
        self.path.with_file_name("Zil.lock")
    }
    pub fn write_lock(&self) -> CompileResult<()> {
        let lock = self.lock_path();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = lock.with_file_name(format!(".Zil.lock.{}.{nonce}", std::process::id()));
        let result = (|| -> std::io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(format!("zil-lock-v1 {}\n", self.fingerprint).as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &lock)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|e| error("E-PROJECT-LOCK", format!("{}: {e}", lock.display())))
    }
    pub fn check_lock(&self) -> CompileResult<()> {
        for package in &self.packages {
            let path = package.directory.join("Zil.toml");
            let file = File::open(&path)
                .map_err(|e| error("E-PROJECT-LOCK", format!("{}: {e}", path.display())))?;
            let mut current = Vec::new();
            file.take(package.manifest.len().saturating_add(1) as u64)
                .read_to_end(&mut current)
                .map_err(|e| error("E-PROJECT-LOCK", format!("{}: {e}", path.display())))?;
            if current != package.manifest {
                return Err(error(
                    "E-PROJECT-LOCK-MISMATCH",
                    format!("manifest changed while loading: {}", path.display()),
                ));
            }
        }
        let mut bytes = Vec::new();
        let lock_path = self.lock_path();
        let file = File::open(&lock_path).map_err(|e| {
            error(
                "E-PROJECT-LOCK",
                format!("{}: {e}; run `zil lock`", lock_path.display()),
            )
        })?;
        file.take(129)
            .read_to_end(&mut bytes)
            .map_err(|e| error("E-PROJECT-LOCK", format!("{}: {e}", lock_path.display())))?;
        if bytes != format!("zil-lock-v1 {}\n", self.fingerprint).as_bytes() {
            return Err(error(
                "E-PROJECT-LOCK-MISMATCH",
                "project manifest or dependency source bytes changed; run `zil lock`",
            ));
        }
        Ok(())
    }
    pub(super) fn resolve(
        &self,
        importer: &Path,
        logical: &[String],
        span: zil_base::Span,
    ) -> CompileResult<PathBuf> {
        let owner = self.owner(importer)?;
        let package = &self.packages[owner];
        if let Some(&dependency) = package.dependencies.get(&logical[0]) {
            let dependency = &self.packages[dependency];
            if logical.len() == 1 {
                return Ok(dependency.entry.clone());
            }
            return super::project::resolve_file_module(&dependency.directory, &logical[1..], span);
        }
        super::project::resolve_file_module(&package.directory, logical, span)
    }
    pub(super) fn owner(&self, importer: &Path) -> CompileResult<usize> {
        self.packages
            .iter()
            .enumerate()
            .filter(|(_, package)| importer.starts_with(&package.directory))
            .max_by_key(|(_, package)| package.directory.as_os_str().len())
            .map(|(index, _)| index)
            .ok_or_else(|| {
                error(
                    "E-PROJECT-PATH",
                    "importer is outside declared project roots",
                )
            })
    }
}
fn parse_manifest(text: &str) -> CompileResult<(String, Vec<String>, BTreeMap<String, String>)> {
    let mut section = "";
    let mut root = None;
    let mut tests = Vec::new();
    let mut dependencies = BTreeMap::new();
    let mut seen_tests = false;
    for (line_number, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line == "[package]" || line == "[dependencies]" {
            section = line;
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            error(
                "E-PROJECT-MANIFEST",
                format!("line {}: expected key = value", line_number + 1),
            )
        })?;
        let key = key.trim();
        let value = value.trim();
        match (section, key) {
            ("[package]", "root") if root.is_none() => root = quoted(value).map(str::to_owned),
            ("[package]", "tests") if !seen_tests => {
                seen_tests = true;
                let inner = value
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .ok_or_else(|| {
                        error(
                            "E-PROJECT-MANIFEST",
                            "tests must be an array of quoted paths",
                        )
                    })?;
                if !inner.trim().is_empty() {
                    for path in inner.split(',') {
                        tests.push(
                            quoted(path.trim())
                                .ok_or_else(|| error("E-PROJECT-MANIFEST", "invalid test path"))?
                                .to_owned(),
                        );
                    }
                }
            }
            ("[dependencies]", alias) if !alias.is_empty() => {
                let value = value
                    .strip_prefix("{ path =")
                    .and_then(|s| s.strip_suffix('}'))
                    .map(str::trim)
                    .unwrap_or(value);
                let path = quoted(value).ok_or_else(|| {
                    error(
                        "E-PROJECT-MANIFEST",
                        format!("line {}: dependency path must be quoted", line_number + 1),
                    )
                })?;
                if dependencies
                    .insert(alias.to_owned(), path.to_owned())
                    .is_some()
                {
                    return Err(error(
                        "E-PROJECT-MANIFEST",
                        format!("duplicate dependency {alias}"),
                    ));
                }
            }
            _ => {
                return Err(error(
                    "E-PROJECT-MANIFEST",
                    format!("line {}: unknown or duplicate key", line_number + 1),
                ));
            }
        }
    }
    let root = root.ok_or_else(|| error("E-PROJECT-MANIFEST", "[package] root is required"))?;
    Ok((root, tests, dependencies))
}

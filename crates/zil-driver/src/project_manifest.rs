//! Strict local project manifests. A lock binds exact manifest and source bytes;
//! the compiler still reads and verifies each imported source as a SourceUnit.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use zil_base::FingerprintBuilder;

#[derive(Clone, Debug)]
pub struct ProjectManifest {
    pub path: PathBuf,
    pub entry: PathBuf,
    pub tests: Vec<PathBuf>,
    pub fingerprint: Fingerprint,
    packages: Vec<Package>,
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
fn quoted(value: &str) -> Option<&str> {
    value.strip_prefix('"')?.strip_suffix('"').filter(|s| !s.contains(['"', '\\', '\n']))
}
fn local_file(directory: &Path, value: &str) -> CompileResult<PathBuf> {
    let relative = Path::new(value);
    if relative.is_absolute() || value.is_empty() {
        return Err(error("E-PROJECT-PATH", "entry and test files must be relative paths"));
    }
    let path = fs::canonicalize(directory.join(relative)).map_err(|e| error("E-PROJECT-PATH", format!("{value}: {e}")))?;
    if !path.starts_with(directory) || !path.is_file() || path.extension().is_none_or(|s| s != "zil") {
        return Err(error("E-PROJECT-PATH", format!("{} must be a .zil file inside {}", path.display(), directory.display())));
    }
    Ok(path)
}
impl ProjectManifest {
    pub fn open(path: &Path, limits: &Limits) -> CompileResult<Self> {
        let manifest = if path.is_dir() { path.join("Zil.toml") } else { path.to_path_buf() };
        let manifest = fs::canonicalize(&manifest).map_err(|e| error("E-PROJECT-MANIFEST", format!("{}: {e}", manifest.display())))?;
        if manifest.file_name().is_none_or(|name| name != "Zil.toml") {
            return Err(error("E-PROJECT-MANIFEST", "project manifest must be named Zil.toml"));
        }
        let mut packages = Vec::new();
        let mut known = BTreeMap::new();
        let mut active = BTreeSet::new();
        Self::load_package(&manifest, limits, &mut packages, &mut known, &mut active)?;
        let text = std::str::from_utf8(&packages[0].manifest).map_err(|_| error("E-PROJECT-MANIFEST", "manifest is not UTF-8"))?;
        let (_, test_names, _) = parse_manifest(text)?;
        let tests = test_names.iter().map(|name| local_file(&packages[0].directory, name)).collect::<CompileResult<Vec<_>>>()?;
        let fingerprint = Self::content_fingerprint(&packages, limits)?;
        Ok(Self { path: manifest, entry: packages[0].entry.clone(), tests, fingerprint, packages })
    }
    fn load_package(path: &Path, limits: &Limits, packages: &mut Vec<Package>, known: &mut BTreeMap<PathBuf, usize>, active: &mut BTreeSet<PathBuf>) -> CompileResult<usize> {
        let path = fs::canonicalize(path).map_err(|e| error("E-PROJECT-DEPENDENCY", format!("{}: {e}", path.display())))?;
        if active.contains(&path) { return Err(error("E-PROJECT-CYCLE", format!("dependency cycle at {}", path.display()))); }
        if let Some(&index) = known.get(&path) { return Ok(index); }
        if packages.len() >= limits.max_call_depth { return Err(vec![Diagnostic::limit(Phase::Driver, "project dependency depth budget exceeded", None)]); }
        let manifest = fs::read(&path).map_err(|e| error("E-PROJECT-MANIFEST", format!("{}: {e}", path.display())))?;
        if manifest.len() > limits.max_source_bytes { return Err(vec![Diagnostic::limit(Phase::Driver, "project manifest byte budget exceeded", None)]); }
        let text = std::str::from_utf8(&manifest).map_err(|_| error("E-PROJECT-MANIFEST", "manifest is not UTF-8"))?;
        let (entry_name, _, dependencies) = parse_manifest(text)?;
        let directory = path.parent().ok_or_else(|| error("E-PROJECT-PATH", "manifest has no parent"))?.to_path_buf();
        let entry = local_file(&directory, &entry_name)?;
        let index = packages.len();
        packages.push(Package { directory: directory.clone(), entry, dependencies: BTreeMap::new(), manifest });
        known.insert(path.clone(), index);
        active.insert(path.clone());
        for (alias, relative) in dependencies {
            if alias == "std" || !super::project::valid_segment(&alias) { return Err(error("E-PROJECT-DEPENDENCY", format!("invalid dependency alias {alias}"))); }
            let candidate = directory.join(relative);
            let dependency_dir = fs::canonicalize(&candidate).map_err(|e| error("E-PROJECT-DEPENDENCY", format!("{}: {e}", candidate.display())))?;
            if !dependency_dir.is_dir() { return Err(error("E-PROJECT-DEPENDENCY", format!("{} is not a directory", dependency_dir.display()))); }
            let dependency = Self::load_package(&dependency_dir.join("Zil.toml"), limits, packages, known, active)?;
            if packages[index].dependencies.insert(alias.clone(), dependency).is_some() { return Err(error("E-PROJECT-DEPENDENCY", format!("duplicate dependency alias {alias}"))); }
        }
        active.remove(&path);
        Ok(index)
    }
    fn content_fingerprint(packages: &[Package], limits: &Limits) -> CompileResult<Fingerprint> {
        let mut builder = FingerprintBuilder::new();
        builder.update(b"zil-project-lock-v1\0");
        let mut remaining = limits.max_source_bytes;
        let mut count = 0usize;
        for package in packages {
            builder.update(package.directory.to_string_lossy().as_bytes());
            builder.update(&package.manifest);
            let mut pending = vec![package.directory.clone()];
            while let Some(directory) = pending.pop() {
                let entries = fs::read_dir(&directory).map_err(|e| error("E-PROJECT-READ", format!("{}: {e}", directory.display())))?;
                let mut entries = entries.collect::<Result<Vec<_>, _>>().map_err(|e| error("E-PROJECT-READ", e.to_string()))?;
                entries.sort_by_key(|entry| entry.file_name());
                for entry in entries {
                    count += 1;
                    if count > limits.max_nodes { return Err(vec![Diagnostic::limit(Phase::Driver, "project file budget exceeded", None)]); }
                    let kind = entry.file_type().map_err(|e| error("E-PROJECT-READ", e.to_string()))?;
                    if kind.is_symlink() { return Err(error("E-PROJECT-PATH", format!("symlink in project source tree: {}", entry.path().display()))); }
                    if kind.is_dir() {
                        if entry.file_name() != ".git" && entry.file_name() != "target" { pending.push(entry.path()); }
                    } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "zil") {
                        let data = fs::read(entry.path()).map_err(|e| error("E-PROJECT-READ", e.to_string()))?;
                        remaining = remaining.checked_sub(data.len()).ok_or_else(|| vec![Diagnostic::limit(Phase::Driver, "project source byte budget exceeded", None)])?;
                        builder.update(entry.path().to_string_lossy().as_bytes());
                        builder.update(&(data.len() as u64).to_le_bytes());
                        builder.update(&data);
                    }
                }
            }
        }
        Ok(builder.digest())
    }
    pub fn lock_path(&self) -> PathBuf { self.path.with_file_name("Zil.lock") }
    pub fn write_lock(&self) -> CompileResult<()> {
        fs::write(self.lock_path(), format!("zil-lock-v1 {}\n", self.fingerprint)).map_err(|e| error("E-PROJECT-LOCK", e.to_string()))
    }
    pub fn check_lock(&self) -> CompileResult<()> {
        let lock = fs::read_to_string(self.lock_path()).map_err(|e| error("E-PROJECT-LOCK", format!("{}: {e}; run `zil lock`", self.lock_path().display())))?;
        if lock != format!("zil-lock-v1 {}\n", self.fingerprint) { return Err(error("E-PROJECT-LOCK-MISMATCH", "project manifest or source bytes changed; run `zil lock`")); }
        Ok(())
    }
    pub(super) fn resolve(&self, importer: &Path, logical: &[String], span: zil_base::Span) -> CompileResult<PathBuf> {
        let owner = self.packages.iter().enumerate().filter(|(_, package)| importer.starts_with(&package.directory)).max_by_key(|(_, package)| package.directory.as_os_str().len()).map(|(index, _)| index).ok_or_else(|| error("E-PROJECT-PATH", "importer is outside declared project roots"))?;
        let package = &self.packages[owner];
        if let Some(&dependency) = package.dependencies.get(&logical[0]) {
            let dependency = &self.packages[dependency];
            if logical.len() == 1 { return Ok(dependency.entry.clone()); }
            return super::project::resolve_file_module(&dependency.directory, &logical[1..], span);
        }
        // A path that names another package without a declared edge cannot be
        // resolved through that package, even when directories happen to nest.
        super::project::resolve_file_module(&package.directory, logical, span)
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
        if line.is_empty() { continue; }
        if line == "[package]" || line == "[dependencies]" { section = line; continue; }
        let (key, value) = line.split_once('=').ok_or_else(|| error("E-PROJECT-MANIFEST", format!("line {}: expected key = value", line_number + 1)))?;
        let key = key.trim();
        let value = value.trim();
        match (section, key) {
            ("[package]", "root") if root.is_none() => root = quoted(value).map(str::to_owned),
            ("[package]", "tests") if !seen_tests => {
                seen_tests = true;
                let inner = value.strip_prefix('[').and_then(|s| s.strip_suffix(']')).ok_or_else(|| error("E-PROJECT-MANIFEST", "tests must be an array of quoted paths"))?;
                if !inner.trim().is_empty() {
                    for path in inner.split(',') {
                        tests.push(quoted(path.trim()).ok_or_else(|| error("E-PROJECT-MANIFEST", "invalid test path"))?.to_owned());
                    }
                }
            }
            ("[dependencies]", alias) if !alias.is_empty() => {
                let value = value.strip_prefix("{ path =").and_then(|s| s.strip_suffix('}')).map(str::trim).unwrap_or(value);
                let path = quoted(value).ok_or_else(|| error("E-PROJECT-MANIFEST", format!("line {}: dependency path must be quoted", line_number + 1)))?;
                if dependencies.insert(alias.to_owned(), path.to_owned()).is_some() { return Err(error("E-PROJECT-MANIFEST", format!("duplicate dependency {alias}"))); }
            }
            _ => return Err(error("E-PROJECT-MANIFEST", format!("line {}: unknown or duplicate key", line_number + 1))),
        }
    }
    let root = root.ok_or_else(|| error("E-PROJECT-MANIFEST", "[package] root is required"))?;
    Ok((root, tests, dependencies))
}

//! Opt-in trusted-local syntax memoization. Source discovery remains current;
//! only successful immutable syntax is reused. No verifier permit is stored.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufWriter, Write};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use zil_syntax::{ParseResult, cache};

const MAGIC: &[u8; 8] = b"ZILPSC01";
const HEADER: usize = 80;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default)]
pub struct ParseCacheReport {
    pub requested: bool,
    pub hits: usize,
    pub misses: usize,
    pub corrupt: usize,
    pub quota_misses: usize,
    pub stored: usize,
    pub session_hits: usize,
    pub parsed: usize,
    pub reason: Option<String>,
    pub write_reason: Option<String>,
}
impl ParseCacheReport {
    pub(super) fn to_json(&self) -> String {
        let optional = |s: &Option<String>| s.as_deref().map_or_else(|| "null".into(), json_string);
        format!(
            "{{\"requested\":{},\"hits\":{},\"misses\":{},\"corrupt\":{},\"quota_misses\":{},\"stored\":{},\"session_hits\":{},\"parsed\":{},\"reason\":{},\"write_reason\":{}}}",
            self.requested,
            self.hits,
            self.misses,
            self.corrupt,
            self.quota_misses,
            self.stored,
            self.session_hits,
            self.parsed,
            optional(&self.reason),
            optional(&self.write_reason)
        )
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn private_directory(path: &Path, create: bool) -> io::Result<()> {
    if create {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)?;
    }
    let meta = match fs::symlink_metadata(path) {
        Err(e) if !create && e.kind() == io::ErrorKind::NotFound => return Ok(()),
        result => result?,
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(invalid(
            "parse cache directory must be a real private directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(invalid("parse cache directory is group/other writable"));
        }
    }
    Ok(())
}

pub(super) struct Prepared {
    root: PathBuf,
    directory: PathBuf,
    producer: Fingerprint,
    keys: BTreeMap<ModuleId, Fingerprint>,
    loaded: BTreeSet<ModuleId>,
    write_ready: bool,
}
impl Prepared {
    /// Read-only preparation: even missing directories are not created until a
    /// complete request has passed artifact guards and finished successfully.
    pub(super) fn new(options: &CompileOptions, report: &mut ParseCacheReport) -> Option<Self> {
        let root = options.query_cache.as_ref()?;
        report.requested = true;
        let bypass = if options.cancellation.is_cancelled() {
            Some("request was already cancelled")
        } else if options.stop_after == Stage::Parse {
            Some("syntax-only entry retains the original single-file path")
        } else if !cfg!(target_os = "linux") || options.windows.is_some() {
            Some("trusted syntax cache requires a Linux host")
        } else if ["LD_PRELOAD", "LD_AUDIT"]
            .into_iter()
            .any(|n| std::env::var_os(n).is_some_and(|v| !v.is_empty()))
        {
            Some("dynamic-loader interposition prevents trusted producer identity")
        } else {
            None
        };
        if let Some(reason) = bypass {
            report.reason = Some(reason.into());
            return None;
        }
        let prepare = || -> io::Result<Self> {
            let root = if root.is_absolute() {
                root.clone()
            } else {
                std::env::current_dir()?.join(root)
            };
            private_directory(&root, false)?;
            let directory = root.join("parse-cp22-v1");
            private_directory(&directory, false)?;
            // Once per request, never once per source module.
            let producer =
                zil_backend_llvm::trusted_installed_tool_identity(&std::env::current_exe()?)?;
            Ok(Self {
                root,
                directory,
                producer,
                keys: BTreeMap::new(),
                loaded: BTreeSet::new(),
                write_ready: false,
            })
        };
        match prepare() {
            Ok(cache) => Some(cache),
            Err(e) => {
                report.reason = Some(e.to_string());
                None
            }
        }
    }
    fn key(&mut self, source: &SourceUnit, options: &CompileOptions) -> Fingerprint {
        *self.keys.entry(source.module_id).or_insert_with(|| {
            let mut bytes = b"ZIL-TRUSTED-PARSE-CP22-v1\0".to_vec();
            bytes.extend_from_slice(&self.producer.0);
            bytes.extend_from_slice(
                &super::session::parse_identity(source, options)
                    .fingerprint()
                    .0,
            );
            Fingerprint::of(&bytes)
        })
    }
    pub(super) fn load(
        &mut self,
        source: &SourceUnit,
        options: &CompileOptions,
        limits: &Limits,
        report: &mut ParseCacheReport,
    ) -> Option<Arc<ParseResult>> {
        let key = self.key(source, options);
        let path = self.directory.join(format!("{key}.psyn"));
        let load = || -> io::Result<Vec<u8>> {
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file()
                || meta.file_type().is_symlink()
                || meta.len() < HEADER as u64
                || meta.len() > (HEADER + cache::MAX_PAYLOAD) as u64
            {
                return Err(invalid("invalid parse cache file/size"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o022 != 0 {
                    return Err(invalid("parse cache file is group/other writable"));
                }
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(meta.len() as usize)
                .map_err(|_| invalid("parse cache allocation bound"))?;
            File::open(&path)?
                .take((HEADER + cache::MAX_PAYLOAD + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() < HEADER
                || bytes.len() > HEADER + cache::MAX_PAYLOAD
                || &bytes[..8] != MAGIC
                || bytes[8..40] != key.0
            {
                return Err(invalid("parse cache identity/protocol mismatch"));
            }
            let length = u64::from_le_bytes(
                bytes[72..80]
                    .try_into()
                    .map_err(|_| invalid("truncated header"))?,
            );
            if length != (bytes.len() - HEADER) as u64
                || Fingerprint::of(&bytes[HEADER..]).0 != bytes[40..72]
            {
                return Err(invalid("parse cache checksum/length mismatch"));
            }
            Ok(bytes)
        };
        match load() {
            Ok(bytes) => match cache::decode(source, &bytes[HEADER..], limits) {
                Ok(parsed) => {
                    report.hits += 1;
                    self.loaded.insert(source.module_id);
                    Some(Arc::new(parsed))
                }
                Err(cache::CacheError::Budget) => {
                    report.misses += 1;
                    report.quota_misses += 1;
                    None
                }
                Err(_) => {
                    report.misses += 1;
                    report.corrupt += 1;
                    None
                }
            },
            Err(error) => {
                report.misses += 1;
                if error.kind() != io::ErrorKind::NotFound {
                    report.corrupt += 1;
                }
                None
            }
        }
    }
    pub(super) fn store(
        &mut self,
        project: &ProjectSources,
        parsed: &[Arc<ParseResult>],
        options: &CompileOptions,
        artifacts: &BTreeMap<String, PathBuf>,
        report: &mut ParseCacheReport,
    ) {
        let protected: BTreeSet<&Path> = project
            .modules
            .iter()
            .map(|source| Path::new(&source.path))
            .chain(artifacts.values().map(PathBuf::as_path))
            .collect();
        for (source, parsed) in project.modules.iter().zip(parsed) {
            if self.loaded.contains(&source.module_id) || options.cancellation.is_cancelled() {
                continue;
            }
            let key = self.key(source, options);
            let result = (|| -> io::Result<()> {
                let payload = cache::encode(source, parsed)
                    .map_err(|_| invalid("syntax exceeds memo codec bounds"))?;
                if !self.write_ready {
                    private_directory(&self.root, true)?;
                    private_directory(&self.directory, true)?;
                    self.directory = fs::canonicalize(&self.directory)?;
                    self.write_ready = true;
                }
                let path = self.directory.join(format!("{key}.psyn"));
                if protected.contains(path.as_path()) {
                    return Err(invalid(
                        "parse cache destination overlaps request input/output",
                    ));
                }
                if options.cancellation.is_cancelled() {
                    return Err(invalid("parse cache publication cancelled"));
                }
                let temp = self.directory.join(format!(
                    ".{key}.{}.{}.tmp",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                let mut open = fs::OpenOptions::new();
                open.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    open.mode(0o600);
                }
                let file = open.open(&temp)?;
                // The cleanup guard owns only a file this request created.
                // A create_new collision must leave the existing path intact.
                let mut cleanup = Temporary(Some(temp.clone()));
                let mut file = BufWriter::new(file);
                file.write_all(MAGIC)?;
                file.write_all(&key.0)?;
                file.write_all(&Fingerprint::of(&payload).0)?;
                file.write_all(&(payload.len() as u64).to_le_bytes())?;
                file.write_all(&payload)?;
                // This memo can be rebuilt after a crash. Flush all buffered
                // bytes before close/rename; no power-loss durability is promised.
                file.flush()?;
                drop(file);
                if options.cancellation.is_cancelled() {
                    return Err(invalid("parse cache publication cancelled"));
                }
                fs::rename(&temp, &path)?;
                cleanup.0 = None;
                Ok(())
            })();
            match result {
                Ok(()) => report.stored += 1,
                Err(error) => report.write_reason = Some(error.to_string()),
            }
        }
    }
}
struct Temporary(Option<PathBuf>);
impl Drop for Temporary {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

//! Session-local final values. No verifier capability is stored here: callers
//! must first bind current verified LIR and authorize each current machine unit.
use crate::incremental::DiagnosticText;
use crate::{BuildArtifact, BuildOptions};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zil_base::{Fingerprint, FingerprintBuilder};
use zil_llvm_api::model::Module;

const MAX_PROGRAM: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    unix: [u64; 9],
}
impl Stamp {
    fn read(path: &Path) -> io::Result<Self> {
        let m = fs::symlink_metadata(path)?;
        if !m.is_file() {
            return Err(io::Error::other("artifact is not a regular file"));
        }
        Ok(Self {
            len: m.len(),
            modified: m.modified()?,
            #[cfg(unix)]
            unix: {
                use std::os::unix::fs::MetadataExt;
                [
                    m.dev(),
                    m.ino(),
                    m.mode() as u64,
                    m.uid() as u64,
                    m.gid() as u64,
                    m.nlink(),
                    m.ctime() as u64,
                    m.ctime_nsec() as u64,
                    m.mtime_nsec() as u64,
                ]
            },
        })
    }
}

/// All codegen configuration is retained, including scheduling and cache policy.
/// Only publication destinations are excluded: the isolated linker sees fixed
/// /inputs/unit-N.o and /out/program paths, independently of those destinations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Configuration {
    options: BuildOptions,
    namespace: Fingerprint,
    umask: u32,
    scoped_threads: bool,
    late_candidates: Option<Vec<Vec<String>>>,
}
impl Configuration {
    pub(crate) fn new(
        options: &BuildOptions,
        namespace: Fingerprint,
        scoped_threads: bool,
        late_candidates: Option<Vec<Vec<String>>>,
    ) -> Option<Self> {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        let mask = status
            .lines()
            .find_map(|line| line.strip_prefix("Umask:"))?;
        let umask = u32::from_str_radix(mask.trim(), 8).ok()?;
        let mut options = options.clone();
        options.output = PathBuf::new();
        options.published_paths = None;
        Some(Self {
            options,
            namespace,
            umask,
            scoped_threads,
            late_candidates,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Published {
    executable: PathBuf,
    llvm_ir: PathBuf,
    program_stamp: Stamp,
    diagnostic_stamp: Stamp,
}
impl Published {
    fn current(&self, executable: &Path, llvm_ir: &Path) -> bool {
        self.executable == executable
            && self.llvm_ir == llvm_ir
            && Stamp::read(executable).ok().as_ref() == Some(&self.program_stamp)
            && Stamp::read(llvm_ir).ok().as_ref() == Some(&self.diagnostic_stamp)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Product {
    configuration: Configuration,
    profile: Arc<crate::native_link::Profile>,
    units: Vec<Module>,
    object_value: Fingerprint,
    program: Arc<Vec<u8>>,
    #[cfg(unix)]
    mode: u32,
    diagnostic: DiagnosticText,
    published: Option<Published>,
}
impl Product {
    pub(crate) fn matches(
        &self,
        config: &Configuration,
        profile: &crate::native_link::Profile,
        units: &[Module],
    ) -> bool {
        &self.configuration == config
            && self.profile.id == profile.id
            && self.units.len() == units.len()
            && self
                .units
                .iter()
                .zip(units)
                .all(|(old, now)| old.triple == now.triple && old.items == now.items)
            && self.profile.current_manifest()
    }
    pub(crate) fn same_objects(
        &self,
        config: &Configuration,
        profile: &crate::native_link::Profile,
        object_value: Fingerprint,
    ) -> bool {
        &self.configuration == config
            && self.profile.id == profile.id
            && self.object_value == object_value
            && self.profile.current_manifest()
    }
    pub(crate) fn capture(
        configuration: Configuration,
        profile: Arc<crate::native_link::Profile>,
        units: Vec<Module>,
        object_value: Fingerprint,
        executable: &Path,
        diagnostic: DiagnosticText,
    ) -> io::Result<Self> {
        let before = Stamp::read(executable)?;
        if before.len > MAX_PROGRAM {
            return Err(io::Error::other("final program retention limit"));
        }
        let program = fs::read(executable)?;
        if Stamp::read(executable)? != before {
            return Err(io::Error::other("final program changed during capture"));
        }
        Ok(Self {
            configuration,
            profile,
            units,
            object_value,
            program: Arc::new(program),
            #[cfg(unix)]
            mode: before.unix[2] as u32 & 0o7777,
            diagnostic,
            published: None,
        })
    }
    pub(crate) fn refreshed(&self, units: Vec<Module>, diagnostic: DiagnosticText) -> Self {
        let mut next = self.clone();
        next.units = units;
        if next.diagnostic != diagnostic {
            next.published = None;
        }
        next.diagnostic = diagnostic;
        next
    }
    pub(crate) fn restore_program(&self, path: &Path) -> io::Result<()> {
        if !self.profile.current_manifest() {
            return Err(io::Error::other(
                "native profile changed before artifact reuse",
            ));
        }
        let mut file = File::create(path)?;
        file.write_all(&self.program)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(self.mode))?;
        }
        Ok(())
    }
    pub(crate) fn token(
        self: &Arc<Self>,
        destinations: Option<&(PathBuf, PathBuf)>,
    ) -> ArtifactReuse {
        let retained = destinations
            .and_then(|(exe, ir)| self.published.as_ref().filter(|p| p.current(exe, ir)))
            .cloned();
        ArtifactReuse {
            product: self.clone(),
            retained,
        }
    }
    fn record_published(&self, executable: &Path, llvm_ir: &Path) -> Option<Self> {
        if !self.profile.current_manifest() {
            return None;
        }
        let program_stamp = Stamp::read(executable).ok()?;
        let diagnostic_stamp = Stamp::read(llvm_ir).ok()?;
        #[cfg(unix)]
        if diagnostic_stamp.unix[2] & 0o7777 != u64::from(0o666 & !self.configuration.umask) {
            return None;
        }
        #[cfg(unix)]
        if program_stamp.unix[2] & 0o7777 != u64::from(self.mode) {
            return None;
        }
        if let Some(published) = &self.published
            && published.current(executable, llvm_ir)
        {
            return Some(self.clone());
        }
        if program_stamp.len != self.program.len() as u64 {
            return None;
        }
        let mut program = File::open(executable).ok()?;
        let mut buffer = [0u8; 65536];
        let mut position = 0usize;
        loop {
            let count = program.read(&mut buffer).ok()?;
            if count == 0 {
                break;
            }
            if self.program.get(position..position.checked_add(count)?)? != &buffer[..count] {
                return None;
            }
            position += count;
        }
        if position != self.program.len() {
            return None;
        }
        // Stream the retained diagnostic through a comparator. No aggregate
        // copy, digest shortcut or file alias can grant a publication lease.
        let mut comparator = CompareWriter {
            input: File::open(llvm_ir).ok()?,
        };
        self.diagnostic.write_to(&mut comparator).ok()?;
        if comparator.input.read(&mut buffer[..1]).ok()? != 0
            || Stamp::read(executable).ok()? != program_stamp
            || Stamp::read(llvm_ir).ok()? != diagnostic_stamp
        {
            return None;
        }
        let mut next = self.clone();
        next.published = Some(Published {
            executable: executable.into(),
            llvm_ir: llvm_ir.into(),
            program_stamp,
            diagnostic_stamp,
        });
        Some(next)
    }
}
struct CompareWriter {
    input: File,
}
impl Write for CompareWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut buffer = [0u8; 65536];
        for chunk in bytes.chunks(buffer.len()) {
            self.input.read_exact(&mut buffer[..chunk.len()])?;
            if &buffer[..chunk.len()] != chunk {
                return Err(io::Error::other(
                    "published diagnostic differs from current producer",
                ));
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Opaque current-build publication evidence. It contains no language verifier
/// capability and is never imported from a file or a caller-supplied digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactReuse {
    product: Arc<Product>,
    retained: Option<Published>,
}
impl ArtifactReuse {
    pub fn retained_paths(&self) -> Option<(&Path, &Path)> {
        self.retained
            .as_ref()
            .map(|p| (p.executable.as_path(), p.llvm_ir.as_path()))
    }
    pub fn current_inputs(&self) -> bool {
        self.product.profile.current_manifest()
    }
    pub fn current_for(&self, executable: &Path, llvm_ir: &Path) -> bool {
        self.current_inputs()
            && self
                .retained
                .as_ref()
                .is_some_and(|p| p.current(executable, llvm_ir))
    }
    pub fn restore(&self, executable: &Path, llvm_ir: &Path) -> io::Result<()> {
        self.product.restore_program(executable)?;
        self.product.diagnostic.write(llvm_ir)?;
        if !self.current_inputs() {
            return Err(io::Error::other(
                "native profile changed during artifact restoration",
            ));
        }
        Ok(())
    }
    pub fn diagnostic_text(&self) -> String {
        self.product.diagnostic.to_text()
    }
}
impl crate::BackendSession {
    /// Called by the driver only after its two-file publication transaction.
    /// A raced/mutated output merely declines its optional file lease.
    pub fn record_published(&mut self, artifact: &BuildArtifact) {
        if let Some(reuse) = &artifact.artifact_reuse {
            self.final_product = reuse
                .product
                .record_published(&artifact.executable, &artifact.llvm_ir)
                .map(Arc::new)
                .or_else(|| Some(reuse.product.clone()));
        }
    }
}

pub(crate) fn object_value(objects: &[PathBuf]) -> io::Result<Fingerprint> {
    let mut state = FingerprintBuilder::new();
    state.update(b"ZIL final ordered native object values v1");
    state.update(&(objects.len() as u64).to_le_bytes());
    let mut buffer = [0u8; 65536];
    for path in objects {
        let before = Stamp::read(path)?;
        state.update(&before.len.to_le_bytes());
        let mut file = File::open(path)?;
        let mut count = 0u64;
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            state.update(&buffer[..size]);
            count += size as u64;
        }
        if count != before.len || Stamp::read(path)? != before {
            return Err(io::Error::other(
                "native object changed during final value observation",
            ));
        }
    }
    Ok(state.digest())
}

/// Actual work performed by final-value reuse. No object-cache hit, native LLVM
/// verifier execution or subprocess is fabricated for this path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub reused_program: bool,
    pub retained_files: bool,
    pub current_preflight_checked_units: Option<usize>,
    pub current_preflight_reused_units: Option<usize>,
    pub before_codegen: bool,
    pub profile_id: String,
}
impl Summary {
    pub fn to_json(&self) -> String {
        let checked = self
            .current_preflight_checked_units
            .map_or_else(|| "null".into(), |v| v.to_string());
        let reused = self
            .current_preflight_reused_units
            .map_or_else(|| "null".into(), |v| v.to_string());
        format!(
            "{{\"reused_program\":{},\"retained_files\":{},\"before_codegen\":{},\"current_preflight_checked_units\":{},\"current_preflight_reused_units\":{},\"profile_id\":{},\"llvm_verification_scope\":\"retained-producer\",\"current_llvm_verifier_executed\":{}}}",
            self.reused_program,
            self.retained_files,
            self.before_codegen,
            checked,
            reused,
            zil_base::json_string(&self.profile_id),
            if self.before_codegen { "false" } else { "null" }
        )
    }
}

/// Current authorization is separate from the retained value match. The sealed
/// checker receipt binds every triple and item; diagnostic fingerprints are
/// excluded by the checker's own input projection, not by this cache.
pub(crate) fn authorize(
    units: &[Module],
    previous: &[Arc<crate::incremental::PreflightProduct>],
) -> Result<(Vec<Arc<crate::incremental::PreflightProduct>>, usize), zil_llvm_api::Error> {
    let mut products = Vec::with_capacity(units.len());
    let mut reused = 0;
    for module in units {
        let old = previous
            .iter()
            .find(|old| old.module.triple == module.triple && old.module.items == module.items);
        let (receipt, hit) = zil_llvm_api::validate_module_with_receipt(
            module,
            old.map(|old| old.receipt.as_ref()),
        )?;
        reused += usize::from(hit);
        products.push(Arc::new(crate::incremental::PreflightProduct {
            module: module.clone(),
            receipt: Arc::new(receipt),
        }));
    }
    Ok((products, reused))
}

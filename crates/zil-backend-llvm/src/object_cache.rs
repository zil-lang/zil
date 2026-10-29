//! Opt-in object reuse inside a trusted, private build directory.
//!
//! The key describes machine inputs; it is NOT a VerifiedModule capability.
//! Only the current verified request reaches this module. A hit still receives
//! current machine preflight. New entries are committed after final publication.
//! ELF build IDs identify trusted installed tools, not hostile replacements;
//! SHA-256 detects accidental cache damage, not an attacker replacing both fields.
use crate::Optimization;
use std::ffi::CStr;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, UNIX_EPOCH};
use zil_base::{Fingerprint, FingerprintBuilder, TargetProfile, json_string};
use zil_llvm_api::model::Module;

const MAGIC: &[u8; 8] = b"ZILOBJ01";
const HEADER: usize = 8 + 32 + 32 + 8;
const MAX_OBJECT: u64 = 128 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectCacheSummary {
    pub requested: bool,
    pub enabled: bool,
    pub hits: usize,
    pub misses: usize,
    pub corrupt: usize,
    pub stored: usize,
    pub write_failures: usize,
    pub key_ns: u64,
    pub lookup_ns: u64,
    pub current_preflight_ns: u64,
    pub disabled_reason: Option<String>,
}
impl ObjectCacheSummary {
    pub(crate) fn to_json(&self) -> String {
        format!(
            "{{\"requested\":{},\"enabled\":{},\"hits\":{},\"misses\":{},\"corrupt\":{},\"stored\":{},\"write_failures\":{},\"key_ns\":{},\"lookup_ns\":{},\"current_preflight_ns\":{},\"disabled_reason\":{}}}",
            self.requested,
            self.enabled,
            self.hits,
            self.misses,
            self.corrupt,
            self.stored,
            self.write_failures,
            self.key_ns,
            self.lookup_ns,
            self.current_preflight_ns,
            self.disabled_reason
                .as_deref()
                .map_or_else(|| "null".into(), json_string)
        )
    }
    pub(crate) fn add(&mut self, access: &Access) {
        self.hits += usize::from(access.hit);
        self.misses += usize::from(!access.hit);
        self.corrupt += usize::from(access.corrupt);
        self.key_ns += access.key_ns;
        self.lookup_ns += access.lookup_ns;
        self.current_preflight_ns += access.preflight_ns;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingObjectCache {
    pub(crate) directory: Option<PathBuf>,
    pub(crate) objects: Vec<PendingObject>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingObject {
    pub(crate) key: Fingerprint,
    pub(crate) bytes: Vec<u8>,
}
impl PendingObjectCache {
    /// Best effort after a successful request. Failed/cancelled callers drop the
    /// pending bytes instead. Cache I/O cannot revoke an already-published result.
    pub(crate) fn commit(self, summary: &mut ObjectCacheSummary) {
        if let Some(directory) = self.directory {
            for object in self.objects {
                match store(&directory, object.key, &object.bytes) {
                    Ok(()) => summary.stored += 1,
                    Err(_) => summary.write_failures += 1,
                }
            }
        }
    }
}

pub(crate) struct PreparedCache {
    pub(crate) directory: PathBuf,
    pub(crate) llvm_version: String,
    namespace: Fingerprint,
}
pub(crate) struct Access {
    pub(crate) key: Fingerprint,
    pub(crate) hit: bool,
    pub(crate) corrupt: bool,
    pub(crate) key_ns: u64,
    pub(crate) lookup_ns: u64,
    pub(crate) preflight_ns: u64,
}
impl Access {
    pub(crate) fn to_json(&self) -> String {
        format!(
            "{{\"state\":{},\"action_key\":{},\"corrupt\":{},\"key_ns\":{},\"lookup_ns\":{},\"current_preflight_ns\":{}}}",
            json_string(if self.hit { "hit" } else { "miss" }),
            json_string(&self.key.to_hex()),
            self.corrupt,
            self.key_ns,
            self.lookup_ns,
            self.preflight_ns
        )
    }
}
pub(crate) fn elapsed_ns(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
impl PreparedCache {
    pub(crate) fn new(
        directory: &Path,
        target: &TargetProfile,
        opt: Optimization,
    ) -> Result<Self, String> {
        // Interposed LLVM functions may come from another DSO. Do not reuse
        // objects under an incomplete tool identity in that environment.
        if ["LD_PRELOAD", "LD_AUDIT"]
            .into_iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        {
            return Err("object cache disabled with dynamic-loader interposition".into());
        }
        let config = zil_llvm_api::Config {
            triple: &target.triple,
            optimization: match opt {
                Optimization::O0 => zil_llvm_api::Optimization::O0,
                Optimization::O2 => zil_llvm_api::Optimization::O2,
            },
        };
        let (version, library) = zil_llvm_api::cache_runtime(&config).map_err(|e| e.to_string())?;
        let compiler = std::env::current_exe().map_err(|e| e.to_string())?;
        let namespace = action_namespace(
            installed_identity(&compiler).map_err(|e| e.to_string())?,
            installed_identity(&library).map_err(|e| e.to_string())?,
            &version,
            target,
            opt,
            config.optimization.pipeline(),
        );
        private_directory(directory).map_err(|e| e.to_string())?;
        let directory = fs::canonicalize(directory)
            .map_err(|e| e.to_string())?
            .join("objects-v1")
            .join(namespace.to_hex());
        private_directory(&directory).map_err(|e| e.to_string())?;
        Ok(Self {
            directory,
            llvm_version: version,
            namespace,
        })
    }
    pub(crate) fn key(&self, module: &Module) -> Fingerprint {
        machine_key(self.namespace, module)
    }
    pub(crate) fn lookup(&self, module: &Module) -> (Access, Option<Vec<u8>>) {
        let t = Instant::now();
        let key = self.key(module);
        let key_ns = elapsed_ns(t);
        let t = Instant::now();
        let (bytes, corrupt) = match load(&self.directory, key) {
            Ok(bytes) => (Some(bytes), false),
            Err(e) => (None, e.kind() == io::ErrorKind::InvalidData),
        };
        (
            Access {
                key,
                hit: bytes.is_some(),
                corrupt,
                key_ns,
                lookup_ns: elapsed_ns(t),
                preflight_ns: 0,
            },
            bytes,
        )
    }
}
fn action_namespace(
    compiler: Fingerprint,
    library: Fingerprint,
    version: &str,
    target: &TargetProfile,
    opt: Optimization,
    pipeline: &CStr,
) -> Fingerprint {
    let mut state = FingerprintBuilder::new();
    // Versioned exact protocol. Derived Hash stays within a compiler identity.
    // Hash the recipe actually executed, as well as the codegen level. Other
    // fixed settings and layout derivation remain bound to the native adapter.
    "ZIL-OBJECT-ACTION-212-v2".hash(&mut state);
    compiler.hash(&mut state);
    library.hash(&mut state);
    version.hash(&mut state);
    target.fingerprint().hash(&mut state);
    match opt {
        Optimization::O0 => "O0",
        Optimization::O2 => "O2",
    }
    .hash(&mut state);
    pipeline.to_bytes().hash(&mut state);
    "x86-64|features=|reloc=PIC|model=default|debug=none|lto=none|llvm-pre-post-verify"
        .hash(&mut state);
    state.digest()
}
fn machine_key(namespace: Fingerprint, module: &Module) -> Fingerprint {
    let mut state = FingerprintBuilder::new();
    namespace.hash(&mut state);
    // Exhaustive destructuring forces future Module fields to be considered.
    // fingerprint is used only by the diagnostic renderer, never by Builder.
    // Do not sort or normalize any instruction, capture, cleanup or symbol order.
    let Module {
        triple,
        fingerprint: _,
        items,
    } = module;
    triple.hash(&mut state);
    items.hash(&mut state);
    state.digest()
}

fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "cache root is not a real directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cache directory must not be group/other writable",
            ));
        }
    }
    Ok(())
}
fn entry_path(directory: &Path, key: Fingerprint) -> PathBuf {
    directory.join(format!("{key}.zobj"))
}
fn bad(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn load(directory: &Path, key: Fingerprint) -> io::Result<Vec<u8>> {
    let path = entry_path(directory, key);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.len() < HEADER as u64
        || meta.len() > MAX_OBJECT + HEADER as u64
    {
        return Err(bad("invalid cached object file"));
    }
    let mut file = File::open(path)?.take(MAX_OBJECT + HEADER as u64 + 1);
    let mut header = [0; HEADER];
    file.read_exact(&mut header)?;
    if &header[..8] != MAGIC || header[8..40] != key.0 {
        return Err(bad("cache magic or action key mismatch"));
    }
    let length = u64::from_le_bytes(header[72..80].try_into().expect("fixed header"));
    if length == 0 || length > MAX_OBJECT || meta.len() != length + HEADER as u64 {
        return Err(bad("invalid object length"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes)?;
    if bytes.len() as u64 != length || Fingerprint::of(&bytes).0 != header[40..72] {
        return Err(bad("object checksum mismatch"));
    }
    Ok(bytes)
}
fn store(directory: &Path, key: Fingerprint, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_OBJECT {
        return Err(bad("object outside cache size bound"));
    }
    private_directory(directory)?;
    let temp = directory.join(format!(
        ".pending-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp)?;
        file.write_all(MAGIC)?;
        file.write_all(&key.0)?;
        file.write_all(&Fingerprint::of(bytes).0)?;
        file.write_all(&(bytes.len() as u64).to_le_bytes())?;
        file.write_all(bytes)?;
        file.flush()?;
        drop(file);
        // Atomic same-directory replacement. Readers see old or complete new
        // bytes. A crash/disk failure can lose a cache entry, never authority.
        fs::rename(&temp, entry_path(directory, key))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Fast installed-tool identity, following the build-id approach used by mature
/// toolchains. Installed tools must not be modified while the compiler runs.
/// Full SHA-256 fallback supports tools without a GNU build ID.
fn installed_identity(path: &Path) -> io::Result<Fingerprint> {
    let mut file = File::open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(bad("tool is not a regular file"));
    }
    let mut state = FingerprintBuilder::new();
    "trusted-installed-tool-v1".hash(&mut state);
    meta.len().hash(&mut state);
    if let Ok(time) = meta
        .modified()
        .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(io::Error::other))
    {
        time.as_nanos().hash(&mut state);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.dev().hash(&mut state);
        meta.ino().hash(&mut state);
        meta.ctime().hash(&mut state);
        meta.ctime_nsec().hash(&mut state);
    }
    if let Some(id) = elf_build_id(&mut file, meta.len())? {
        "gnu-build-id".hash(&mut state);
        id.hash(&mut state);
    } else {
        "content-sha256-fallback".hash(&mut state);
        file.seek(SeekFrom::Start(0))?;
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            state.write(&buffer[..n]);
        }
    }
    let after = file.metadata()?;
    if meta.len() != after.len() || meta.modified()? != after.modified()? {
        return Err(bad("tool changed while identifying it"));
    }
    Ok(state.digest())
}
fn elf_build_id(file: &mut File, size: u64) -> io::Result<Option<Vec<u8>>> {
    if size < 64 {
        return Ok(None);
    }
    let mut header = [0; 64];
    file.read_exact(&mut header)?;
    if &header[..6] != b"\x7fELF\x02\x01" {
        return Ok(None);
    }
    let u16at = |n| u16::from_le_bytes([header[n], header[n + 1]]) as u64;
    let offset = u64::from_le_bytes(header[32..40].try_into().expect("header"));
    let width = u16at(54);
    let count = u16at(56);
    if !(56..=256).contains(&width)
        || count > 1024
        || offset.checked_add(width * count).is_none_or(|n| n > size)
    {
        return Ok(None);
    }
    for index in 0..count {
        file.seek(SeekFrom::Start(offset + index * width))?;
        let mut ph = [0; 56];
        file.read_exact(&mut ph)?;
        if u32::from_le_bytes(ph[..4].try_into().expect("header")) != 4 {
            continue;
        }
        let note_offset = u64::from_le_bytes(ph[8..16].try_into().expect("header"));
        let note_size = u64::from_le_bytes(ph[32..40].try_into().expect("header"));
        if note_size > 1024 * 1024 || note_offset.checked_add(note_size).is_none_or(|n| n > size) {
            continue;
        }
        let mut notes = vec![0; note_size as usize];
        file.seek(SeekFrom::Start(note_offset))?;
        file.read_exact(&mut notes)?;
        let mut cursor = 0usize;
        while cursor + 12 <= notes.len() {
            let n = |start| {
                u32::from_le_bytes(notes[start..start + 4].try_into().expect("note")) as usize
            };
            let names = n(cursor);
            let desc = n(cursor + 4);
            let kind = n(cursor + 8);
            cursor += 12;
            // n <= u32::MAX, using u64/usize on the checked 64-bit host.
            let padded_name = (names + 3) & !3;
            let padded_desc = (desc + 3) & !3;
            let Some(end) = cursor
                .checked_add(padded_name)
                .and_then(|x| x.checked_add(padded_desc))
            else {
                break;
            };
            if end > notes.len() {
                break;
            }
            if kind == 3
                && names == 4
                && &notes[cursor..cursor + 4] == b"GNU\0"
                && (16..=128).contains(&desc)
            {
                return Ok(Some(
                    notes[cursor + padded_name..cursor + padded_name + desc].to_vec(),
                ));
            }
            cursor = end;
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{
        Arg, Attr, Block, Function, Inst, Item, Linkage, Op, Param, Signature, Ty, Val,
    };
    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "zil-cache212-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            private_directory(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn sample() -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "current-verify".into(),
            items: vec![Item::Function(Function {
                name: "f".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), vec![Param::new(Ty::Int(32), "x")]),
                attrs: vec![],
                blocks: Some(vec![Block {
                    name: "entry".into(),
                    instructions: vec![Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(42)))),
                    )],
                }]),
            })],
        }
    }
    #[test]
    fn changing_only_the_pass_recipe_invalidates_object_identity() {
        let compiler = Fingerprint::of(b"same-compiler");
        let library = Fingerprint::of(b"same-llvm");
        let target = TargetProfile::host_llvm();
        let namespace = |pipeline| {
            action_namespace(
                compiler,
                library,
                "17.0.6",
                &target,
                Optimization::O0,
                pipeline,
            )
        };
        let old = namespace(c"default<O0>");
        let promoted = namespace(zil_llvm_api::Optimization::O0.pipeline());
        assert_ne!(old, promoted);
        let module = sample();
        assert_ne!(machine_key(old, &module), machine_key(promoted, &module));
    }
    #[test]
    fn key_binds_all_machine_changes_but_not_authorization_comment() {
        let a = sample();
        let ns = Fingerprint::of(b"compiler-and-target");
        let key = machine_key(ns, &a);
        let mut b = a.clone();
        b.fingerprint = "other-current-verify".into();
        assert_eq!(key, machine_key(ns, &b));
        assert_ne!(key, machine_key(Fingerprint::of(b"other-toolchain"), &a));
        let changes: [fn(&mut Module); 8] = [
            |m| m.triple = "x86_64-pc-windows-msvc".into(),
            |m| {
                if let Item::Function(f) = &mut m.items[0] {
                    f.name = "g".into();
                }
            },
            |m| {
                if let Item::Function(f) = &mut m.items[0] {
                    f.linkage = Linkage::Internal;
                }
            },
            |m| {
                if let Item::Function(f) = &mut m.items[0] {
                    f.attrs.push(Attr::NonLazyBind);
                }
            },
            |m| {
                if let Item::Function(f) = &mut m.items[0] {
                    f.signature.params[0].attrs.push(Attr::ZeroExt);
                }
            },
            |m| {
                if let Item::Function(f) = &mut m.items[0] {
                    f.signature.params[0].ty = Ty::Int(64);
                }
            },
            |m| {
                if let Item::Function(f) = &mut m.items[0] {
                    f.blocks.as_mut().unwrap()[0].instructions[0].op = Op::Unreachable;
                }
            },
            |m| {
                m.items
                    .push(Item::NamedType("layout".into(), vec![Ty::Int(64)]))
            },
        ];
        for change in changes {
            let mut b = a.clone();
            change(&mut b);
            assert_ne!(key, machine_key(ns, &b));
        }
        let mut b = a.clone();
        b.items.push(Item::NamedType("x".into(), vec![]));
        let first = machine_key(ns, &b);
        b.items.reverse();
        assert_ne!(first, machine_key(ns, &b));
    }
    #[test]
    fn entry_roundtrip_corruption_key_and_length_are_checked() {
        let d = Dir::new();
        let key = Fingerprint::of(b"action");
        store(&d.0, key, b"native-object").unwrap();
        assert_eq!(load(&d.0, key).unwrap(), b"native-object");
        let path = entry_path(&d.0, key);
        let good = fs::read(&path).unwrap();
        for offset in [0, 8, 40, 72, HEADER] {
            let mut bad = good.clone();
            bad[offset] ^= 1;
            fs::write(&path, &bad).unwrap();
            assert!(load(&d.0, key).is_err());
        }
        for n in [0, 8, 79, 80, good.len() - 1] {
            fs::write(&path, &good[..n]).unwrap();
            assert!(load(&d.0, key).is_err());
        }
        fs::write(&path, [good.as_slice(), b"extra"].concat()).unwrap();
        assert!(load(&d.0, key).is_err());
        store(&d.0, key, b"native-object").unwrap();
        assert_eq!(load(&d.0, key).unwrap(), b"native-object");
    }
    #[test]
    fn concurrent_writers_only_publish_complete_entries() {
        let d = Dir::new();
        let key = Fingerprint::of(b"shared");
        std::thread::scope(|s| {
            for _ in 0..8 {
                let p = &d.0;
                s.spawn(move || {
                    for _ in 0..20 {
                        store(p, key, b"same-object").unwrap();
                        if let Ok(bytes) = load(p, key) {
                            assert_eq!(bytes, b"same-object");
                        }
                    }
                });
            }
        });
        assert_eq!(load(&d.0, key).unwrap(), b"same-object");
        assert_eq!(fs::read_dir(&d.0).unwrap().count(), 1);
    }
    #[test]
    fn pending_objects_are_not_stored_until_success_commit() {
        let d = Dir::new();
        let mut summary = ObjectCacheSummary::default();
        let make = || PendingObjectCache {
            directory: Some(d.0.clone()),
            objects: vec![PendingObject {
                key: Fingerprint::ZERO,
                bytes: b"complete-object".to_vec(),
            }],
        };
        drop(make());
        assert_eq!(fs::read_dir(&d.0).unwrap().count(), 0);
        make().commit(&mut summary);
        assert_eq!(summary.stored, 1);
        assert_eq!(summary.write_failures, 0);
    }
    #[test]
    fn unwritable_cache_cannot_revoke_success() {
        let d = Dir::new();
        let file = d.0.join("file");
        fs::write(&file, b"keep").unwrap();
        let mut summary = ObjectCacheSummary::default();
        PendingObjectCache {
            directory: Some(file.clone()),
            objects: vec![PendingObject {
                key: Fingerprint::ZERO,
                bytes: b"object".to_vec(),
            }],
        }
        .commit(&mut summary);
        assert_eq!(summary.write_failures, 1);
        assert_eq!(fs::read(file).unwrap(), b"keep");
    }
    #[test]
    fn installed_tool_identity_fallback_detects_changed_content() {
        let d = Dir::new();
        let path = d.0.join("tool");
        fs::write(&path, b"no ELF build-id").unwrap();
        let a = installed_identity(&path).unwrap();
        assert_eq!(a, installed_identity(&path).unwrap());
        fs::write(&path, b"changed tool!").unwrap();
        assert_ne!(a, installed_identity(&path).unwrap());
        assert!(installed_identity(&d.0).is_err());
        assert!(installed_identity(&std::env::current_exe().unwrap()).is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_and_group_writable_cache_roots_are_rejected() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let d = Dir::new();
        let link = d.0.join("link");
        symlink(&d.0, &link).unwrap();
        assert!(private_directory(&link).is_err());
        let public = d.0.join("public");
        fs::create_dir(&public).unwrap();
        fs::set_permissions(&public, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(private_directory(&public).is_err());
        let key = Fingerprint::ZERO;
        symlink(d.0.join("missing"), entry_path(&d.0, key)).unwrap();
        assert!(load(&d.0, key).is_err());
    }
}

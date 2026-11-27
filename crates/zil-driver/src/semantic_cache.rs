//! Trusted-local semantic candidate & per-function certificate cache helpers.
//!
//! ## Trust Boundary
//! `--semantic-cache DIR` is an explicit **trusted-local build cache** (same
//! trust boundary as `--object-cache DIR` and `rustc -C incremental=DIR`).
//!
//! - **What is always enforced fresh**: Every build still runs fresh
//!   `zil_check::ct_original::verify_ct_original_definitions`,
//!   `zil_check::check_fidelity`, and `zil_check::check_module`
//!   (`verification_runs == 1`). Memory safety, borrow/protocol invariants,
//!   structural AST-to-IR fidelity, and SHA-256 integrity envelopes are NEVER
//!   bypassed; any corrupted or safety-invalid snapshot is purged and rebuilt.
//! - **What is NOT claimed**: Because `check_fidelity` verifies structural and
//!   resource/field/trait-impl fidelity rather than re-running full typeck and
//!   lowering on every pure literal/expression, `--semantic-cache DIR` does NOT
//!   defend against a local attacker with write access to the cache directory
//!   who deliberately rewrites a pure constant (e.g., `return 1` -> `return 2`),
//!   re-solves a valid safety certificate, and recomputes SHA-256. Keep cache
//!   directories in trusted local storage.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use zil_base::{Fingerprint, FingerprintBuilder, Limits, TargetProfile};
use zil_ir::body::{
    Certificate, FunctionCertificate, Module, SourceMap, WIRE_VERSION, decode_semantic_candidate,
    encode_semantic_candidate,
};
use zil_solve::{CertCacheKey, CertificateCache};

static SEM_TMP_SEQ: AtomicU64 = AtomicU64::new(1);

const MAX_DISK_SEM_SNAPSHOTS: usize = 8;
const MAX_MEM_FN_CERTS: usize = 4096;

/// Explicit typeck/lowering semantic cache ABI version. Bump whenever typeck,
/// lowering, or semantic-candidate representation changes even if `WIRE_VERSION`
/// stays unchanged.
pub const SEMANTIC_CACHE_ABI_VERSION: u32 = 1;

/// Canonical fingerprint of the complete source module graph, compiler/ABI
/// identity, target, and limits.
pub(crate) fn program_semantic_key(
    modules: &[(&str, Fingerprint)],
    target: &TargetProfile,
    limits: &Limits,
) -> Fingerprint {
    program_semantic_key_with_abi(
        modules,
        target,
        limits,
        SEMANTIC_CACHE_ABI_VERSION,
        env!("CARGO_PKG_VERSION"),
        WIRE_VERSION,
    )
}

/// Parameterized key constructor used by cross-version invalidation tests.
pub(crate) fn program_semantic_key_with_abi(
    modules: &[(&str, Fingerprint)],
    target: &TargetProfile,
    limits: &Limits,
    abi_version: u32,
    compiler_version: &str,
    wire_version: u32,
) -> Fingerprint {
    let mut h = FingerprintBuilder::new();
    h.update(b"zil-sem-cache-v2\0");
    h.update(&abi_version.to_le_bytes());
    h.update(&wire_version.to_le_bytes());
    h.update(compiler_version.as_bytes());
    h.update(b"\0");
    h.update(target.triple.as_bytes());
    h.update(b"\0");
    h.update(&target.pointer_width.to_le_bytes());
    h.update(&(limits.max_source_bytes as u64).to_le_bytes());
    h.update(&(limits.max_tokens as u64).to_le_bytes());
    h.update(&(limits.max_nodes as u64).to_le_bytes());
    h.update(&(limits.max_functions as u64).to_le_bytes());
    h.update(&(limits.max_blocks as u64).to_le_bytes());
    h.update(&(limits.max_instructions as u64).to_le_bytes());
    h.update(&(limits.max_locals as u64).to_le_bytes());
    h.update(&(limits.max_values as u64).to_le_bytes());
    h.update(&(limits.max_analysis_steps as u64).to_le_bytes());
    h.update(&limits.max_ct_steps.to_le_bytes());
    h.update(&(limits.max_call_depth as u64).to_le_bytes());
    h.update(&(modules.len() as u64).to_le_bytes());
    for (path, fp) in modules {
        h.update(path.as_bytes());
        h.update(b"\0");
        h.update(&fp.0);
    }
    h.digest()
}

pub(crate) fn sem_snapshot_path(dir: &Path, key: Fingerprint) -> PathBuf {
    dir.join(format!("sem-{}.zsem", key.to_hex()))
}

pub(crate) fn read_disk_semantic_candidate(
    dir: &Path,
    key: Fingerprint,
) -> Option<(Module, SourceMap, Certificate)> {
    let path = sem_snapshot_path(dir, key);
    let bytes = fs::read(&path).ok()?;
    match decode_semantic_candidate(&bytes) {
        Ok(decoded) => Some(decoded),
        Err(_) => {
            let _ = fs::remove_file(&path);
            None
        }
    }
}

pub(crate) fn remove_disk_semantic_candidate(dir: &Path, key: Fingerprint) {
    let _ = fs::remove_file(sem_snapshot_path(dir, key));
}

pub(crate) fn write_disk_semantic_candidate(
    dir: &Path,
    key: Fingerprint,
    module: &Module,
    source_map: &SourceMap,
    cert: &Certificate,
) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let bytes = encode_semantic_candidate(module, source_map, cert);
    let target = sem_snapshot_path(dir, key);
    let seq = SEM_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".sem.tmp.{}.{seq}", std::process::id()));
    if fs::write(&tmp, &bytes).is_ok() {
        if fs::rename(&tmp, &target).is_err() {
            let _ = fs::remove_file(&tmp);
        } else {
            prune_old_sem_snapshots(dir, &target);
        }
    }
}

fn prune_old_sem_snapshots(dir: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p == keep {
            continue;
        }
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("sem-") || !name.ends_with(".zsem") {
            continue;
        }
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        files.push((mtime, p));
    }
    if files.len() < MAX_DISK_SEM_SNAPSHOTS {
        return;
    }
    files.sort_by_key(|(t, _)| *t);
    let to_remove = files.len().saturating_sub(MAX_DISK_SEM_SNAPSHOTS - 1);
    for (_, p) in files.into_iter().take(to_remove) {
        let _ = fs::remove_file(p);
    }
}

/// Thread-safe in-memory per-function candidate certificate cache seeded from
/// either an in-RAM resident snapshot or the latest disk `.zsem` snapshot.
pub(crate) struct SessionCertCache<'a> {
    mem: &'a RwLock<HashMap<CertCacheKey, FunctionCertificate>>,
}

impl<'a> SessionCertCache<'a> {
    pub(crate) fn new(mem: &'a RwLock<HashMap<CertCacheKey, FunctionCertificate>>) -> Self {
        Self { mem }
    }

    pub(crate) fn purge_all(&self) {
        if let Ok(mut guard) = self.mem.write() {
            guard.clear();
        }
    }
}

impl CertificateCache for SessionCertCache<'_> {
    fn get(&self, key: CertCacheKey) -> Option<FunctionCertificate> {
        self.mem.read().ok()?.get(&key).cloned()
    }

    fn put(&self, key: CertCacheKey, cert: &FunctionCertificate) {
        if let Ok(mut guard) = self.mem.write() {
            if guard.len() >= MAX_MEM_FN_CERTS {
                guard.clear();
            }
            guard.insert(key, cert.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_key_invalidates_across_abi_compiler_and_wire_versions() {
        let fp = Fingerprint::of(b"fn main() -> i32 { return 1; }\n");
        let mods = [("main", fp)];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();

        let base = program_semantic_key_with_abi(
            &mods,
            &target,
            &limits,
            SEMANTIC_CACHE_ABI_VERSION,
            "1.0.0-alpha.2",
            WIRE_VERSION,
        );
        let bumped_abi = program_semantic_key_with_abi(
            &mods,
            &target,
            &limits,
            SEMANTIC_CACHE_ABI_VERSION + 1,
            "1.0.0-alpha.2",
            WIRE_VERSION,
        );
        let bumped_compiler = program_semantic_key_with_abi(
            &mods,
            &target,
            &limits,
            SEMANTIC_CACHE_ABI_VERSION,
            "1.0.0-alpha.3",
            WIRE_VERSION,
        );
        let bumped_wire = program_semantic_key_with_abi(
            &mods,
            &target,
            &limits,
            SEMANTIC_CACHE_ABI_VERSION,
            "1.0.0-alpha.2",
            WIRE_VERSION + 1,
        );

        assert_ne!(base, bumped_abi, "ABI bump must invalidate semantic key");
        assert_ne!(
            base, bumped_compiler,
            "compiler version bump must invalidate semantic key"
        );
        assert_ne!(
            base, bumped_wire,
            "wire version bump must invalidate semantic key"
        );
        assert_ne!(bumped_abi, bumped_compiler);
        assert_ne!(bumped_abi, bumped_wire);
        assert_ne!(bumped_compiler, bumped_wire);
    }
}

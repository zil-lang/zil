//! Trusted-local semantic candidate & per-function certificate cache helpers.
//!
//! All cached artifacts are strictly UNTRUSTED candidates: they can skip
//! parsing, typeck, or fixed-point solving, but NEVER grant safety authority.
//! Every build still executes fresh `zil_check::check_fidelity` and
//! `zil_check::check_module` (`verification_runs == 1`) and transparently
//! purges + rebuilds on any verification mismatch or corruption.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use zil_base::{Fingerprint, FingerprintBuilder, Limits, TargetProfile};
use zil_ir::body::{
    Certificate, FunctionCertificate, Module, SourceMap, decode_semantic_candidate,
    encode_semantic_candidate,
};
use zil_solve::{CertCacheKey, CertificateCache};

static SEM_TMP_SEQ: AtomicU64 = AtomicU64::new(1);

const MAX_DISK_SEM_SNAPSHOTS: usize = 8;
const MAX_MEM_FN_CERTS: usize = 4096;

/// Canonical fingerprint of the complete source module graph, target, and limits.
pub(crate) fn program_semantic_key(
    modules: &[(&str, Fingerprint)],
    target: &TargetProfile,
    limits: &Limits,
) -> Fingerprint {
    let mut h = FingerprintBuilder::new();
    h.update(b"zil-sem-cache-v1\0");
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

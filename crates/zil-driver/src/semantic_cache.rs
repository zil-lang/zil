//! Trusted-local semantic candidate, export-signature incremental baseline,
//! and per-function certificate cache helpers.
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
use zil_typeck::ModuleDepRecord;

static SEM_TMP_SEQ: AtomicU64 = AtomicU64::new(1);

const MAX_DISK_SEM_SNAPSHOTS: usize = 8;
const MAX_DISK_BASELINES: usize = 4;
const MAX_MEM_FN_CERTS: usize = 4096;
const BASELINE_MAGIC: &[u8; 4] = b"ZBAS";
const BASELINE_MAX_BYTES: usize = 40 * 1024 * 1024;
const BASELINE_MAX_RECORDS: usize = 16_384;

/// Explicit typeck/lowering semantic cache ABI version. Bump whenever typeck,
/// lowering, or semantic-candidate representation changes even if `WIRE_VERSION`
/// stays unchanged.
pub const SEMANTIC_CACHE_ABI_VERSION: u32 = 2;

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
    hash_limits(&mut h, limits);
    h.update(&(modules.len() as u64).to_le_bytes());
    for (path, fp) in modules {
        h.update(path.as_bytes());
        h.update(b"\0");
        h.update(&fp.0);
    }
    h.digest()
}

/// Scope key for the per-entry incremental baseline (`baseline-<key>.zbase`).
/// Binds entry path, ABI/compiler/wire versions, target, and limits so partial
/// edits across consecutive CLI invocations find their latest baseline cleanly.
pub(crate) fn baseline_scope_key(
    entry_path: &str,
    target: &TargetProfile,
    limits: &Limits,
) -> Fingerprint {
    let mut h = FingerprintBuilder::new();
    h.update(b"zil-inc-base-v1\0");
    h.update(&SEMANTIC_CACHE_ABI_VERSION.to_le_bytes());
    h.update(&WIRE_VERSION.to_le_bytes());
    h.update(env!("CARGO_PKG_VERSION").as_bytes());
    h.update(b"\0");
    h.update(target.triple.as_bytes());
    h.update(b"\0");
    h.update(&target.pointer_width.to_le_bytes());
    hash_limits(&mut h, limits);
    h.update(entry_path.as_bytes());
    h.update(b"\0");
    h.digest()
}

fn hash_limits(h: &mut FingerprintBuilder, limits: &Limits) {
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
}

pub(crate) fn sem_snapshot_path(dir: &Path, key: Fingerprint) -> PathBuf {
    dir.join(format!("sem-{}.zsem", key.to_hex()))
}

pub(crate) fn baseline_path(dir: &Path, scope_key: Fingerprint) -> PathBuf {
    dir.join(format!("base-{}.zbase", scope_key.to_hex()))
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
            prune_old_files(dir, &target, "sem-", ".zsem", MAX_DISK_SEM_SNAPSHOTS);
        }
    }
}

/// Encodes a SHA-256-protected incremental baseline bundle (`ZBAS`).
pub(crate) fn encode_incremental_baseline(
    dep_records: &[ModuleDepRecord],
    module: &Module,
    source_map: &SourceMap,
    cert: &Certificate,
) -> Vec<u8> {
    let zsem = encode_semantic_candidate(module, source_map, cert);
    let mut payload = Vec::with_capacity(zsem.len() + dep_records.len() * 128 + 16);
    payload.extend_from_slice(&(dep_records.len() as u32).to_le_bytes());
    for r in dep_records {
        let pb = r.logical_path.as_bytes();
        payload.extend_from_slice(&(pb.len() as u32).to_le_bytes());
        payload.extend_from_slice(pb);
        payload.extend_from_slice(&r.file_id.to_le_bytes());
        payload.extend_from_slice(&r.text_fp.0);
        payload.extend_from_slice(&r.export_sig_fp.0);
        payload.extend_from_slice(&r.dep_iface_fp.0);
        payload.push(u8::from(r.has_comptime));
    }
    payload.extend_from_slice(&(zsem.len() as u32).to_le_bytes());
    payload.extend_from_slice(&zsem);

    let fp = Fingerprint::of(&payload);
    let mut out = Vec::with_capacity(4 + 4 + 4 + 32 + payload.len());
    out.extend_from_slice(BASELINE_MAGIC);
    out.extend_from_slice(&SEMANTIC_CACHE_ABI_VERSION.to_le_bytes());
    out.extend_from_slice(&WIRE_VERSION.to_le_bytes());
    out.extend_from_slice(&fp.0);
    out.extend_from_slice(&payload);
    out
}

/// Decodes and SHA-256-verifies an incremental baseline bundle (`ZBAS`).
pub(crate) fn decode_incremental_baseline(
    bytes: &[u8],
) -> Option<(Vec<ModuleDepRecord>, Module, SourceMap, Certificate)> {
    if bytes.len() < 44 || bytes.len() > BASELINE_MAX_BYTES {
        return None;
    }
    if &bytes[0..4] != BASELINE_MAGIC {
        return None;
    }
    let abi = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let wire = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    if abi != SEMANTIC_CACHE_ABI_VERSION || wire != WIRE_VERSION {
        return None;
    }
    let expected_fp = &bytes[12..44];
    let payload = &bytes[44..];
    if Fingerprint::of(payload).0.as_slice() != expected_fp {
        return None;
    }

    let mut pos = 0usize;
    let take = |pos: &mut usize, n: usize| -> Option<&[u8]> {
        let end = pos.checked_add(n)?;
        let slice = payload.get(*pos..end)?;
        *pos = end;
        Some(slice)
    };
    let count = u32::from_le_bytes(take(&mut pos, 4)?.try_into().ok()?) as usize;
    if count > BASELINE_MAX_RECORDS {
        return None;
    }
    let mut dep_records = Vec::with_capacity(count);
    for _ in 0..count {
        let plen = u32::from_le_bytes(take(&mut pos, 4)?.try_into().ok()?) as usize;
        let path_bytes = take(&mut pos, plen)?;
        let logical_path = std::str::from_utf8(path_bytes).ok()?.to_owned();
        let file_id = u32::from_le_bytes(take(&mut pos, 4)?.try_into().ok()?);
        let text_fp = Fingerprint(take(&mut pos, 32)?.try_into().ok()?);
        let export_sig_fp = Fingerprint(take(&mut pos, 32)?.try_into().ok()?);
        let dep_iface_fp = Fingerprint(take(&mut pos, 32)?.try_into().ok()?);
        let has_comptime = match take(&mut pos, 1)?[0] {
            0 => false,
            1 => true,
            _ => return None,
        };
        dep_records.push(ModuleDepRecord {
            logical_path,
            file_id,
            text_fp,
            export_sig_fp,
            dep_iface_fp,
            has_comptime,
        });
    }
    let zsem_len = u32::from_le_bytes(take(&mut pos, 4)?.try_into().ok()?) as usize;
    let zsem_bytes = take(&mut pos, zsem_len)?;
    if pos != payload.len() {
        return None;
    }
    let (module, source_map, cert) = decode_semantic_candidate(zsem_bytes).ok()?;
    Some((dep_records, module, source_map, cert))
}

pub(crate) fn read_disk_incremental_baseline(
    dir: &Path,
    scope_key: Fingerprint,
) -> Option<(Vec<ModuleDepRecord>, Module, SourceMap, Certificate)> {
    let path = baseline_path(dir, scope_key);
    let bytes = fs::read(&path).ok()?;
    match decode_incremental_baseline(&bytes) {
        Some(decoded) => Some(decoded),
        None => {
            let _ = fs::remove_file(&path);
            None
        }
    }
}

pub(crate) fn remove_disk_incremental_baseline(dir: &Path, scope_key: Fingerprint) {
    let _ = fs::remove_file(baseline_path(dir, scope_key));
}

pub(crate) fn write_disk_incremental_baseline(
    dir: &Path,
    scope_key: Fingerprint,
    dep_records: &[ModuleDepRecord],
    module: &Module,
    source_map: &SourceMap,
    cert: &Certificate,
) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let bytes = encode_incremental_baseline(dep_records, module, source_map, cert);
    let target = baseline_path(dir, scope_key);
    let seq = SEM_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".base.tmp.{}.{seq}", std::process::id()));
    if fs::write(&tmp, &bytes).is_ok() {
        if fs::rename(&tmp, &target).is_err() {
            let _ = fs::remove_file(&tmp);
        } else {
            prune_old_files(dir, &target, "base-", ".zbase", MAX_DISK_BASELINES);
        }
    }
}

fn prune_old_files(dir: &Path, keep: &Path, prefix: &str, suffix: &str, max_keep: usize) {
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
        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        files.push((mtime, p));
    }
    if files.len() < max_keep {
        return;
    }
    files.sort_by_key(|(t, _)| *t);
    let to_remove = files.len().saturating_sub(max_keep - 1);
    for (_, p) in files.into_iter().take(to_remove) {
        let _ = fs::remove_file(p);
    }
}

/// Thread-safe in-memory per-function candidate certificate cache seeded from
/// either an in-RAM resident snapshot or the latest disk `.zbase` baseline.
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

    #[test]
    fn incremental_baseline_roundtrip_and_tamper_rejection() {
        let records = vec![ModuleDepRecord {
            logical_path: "mod_a".into(),
            file_id: 1,
            text_fp: Fingerprint::of(b"src_a"),
            export_sig_fp: Fingerprint::of(b"exp_a"),
            dep_iface_fp: Fingerprint::of(b"dep_a"),
            has_comptime: false,
        }];
        let module = Module::default();
        let smap = SourceMap::default();
        let cert = Certificate {
            functions: Vec::new(),
        };
        let encoded = encode_incremental_baseline(&records, &module, &smap, &cert);
        let (dec_rec, _, _, _) =
            decode_incremental_baseline(&encoded).expect("valid baseline decodes");
        assert_eq!(dec_rec, records);

        let mut tampered = encoded.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xFF;
        assert!(
            decode_incremental_baseline(&tampered).is_none(),
            "tampered baseline must fail SHA-256 verification"
        );
    }
}

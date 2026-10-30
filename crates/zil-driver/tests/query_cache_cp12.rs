//! CP12 full-source native requests: cached data never carries Verify authority.
#![cfg(target_os = "linux")]
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, DiskCacheStatus, NativeBackend,
    ProjectSources, QueryDisposition, Stage, compile_path, compile_project, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const ROOT: &str =
    "import math\nimport other\nfn value()->i64=>i64(#(math.number()))+i64(other.one())\n";
const MATH: &str = "fn hidden()->i32=>41\npub fn number()->i32=>hidden()\n";
const OTHER: &str = "pub fn one()->i32=>1\n";
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "zil-cp12-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let case = Self(directory);
        case.put("main.zil", ROOT);
        case.put("math.zil", MATH);
        case.put("other.zil", OTHER);
        case
    }
    fn put(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn options(&self) -> CompileOptions {
        CompileOptions {
            backend: NativeBackend::Clang,
            stop_after: Stage::Build,
            entry: Some("value".into()),
            output: Some(self.0.join("program")),
            query_cache: Some(self.0.join("queries")),
            ..CompileOptions::default()
        }
    }
    fn build(&self, label: &str, options: &CompileOptions) -> CompileReport {
        let report = compile_path(&self.0.join("main.zil"), options);
        fs::write(self.0.join(format!("{label}.json")), report.to_json()).unwrap();
        report
    }
    fn entry(&self, report: &CompileReport) -> PathBuf {
        self.0
            .join("queries/frontend-cp12-v1")
            .join(format!("{}.qcp", report.query_cache.key.unwrap()))
    }
    fn entries(&self) -> Vec<PathBuf> {
        fs::read_dir(self.0.join("queries/frontend-cp12-v1"))
            .into_iter()
            .flatten()
            .map(|e| e.unwrap().path())
            .collect()
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("CP12 failure files retained at {}", self.0.display());
        } else {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn success(report: &CompileReport, state: &str, stdout: &[u8]) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.query_cache.state, state, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    assert_eq!(
        report
            .query_events
            .iter()
            .filter(|e| e.phase == Phase::Verify)
            .count(),
        1
    );
    let output = Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, stdout);
    assert!(output.stderr.is_empty());
}

#[test]
fn source_selection_bypasses_checkpoint_and_replays_current_ct_budget() {
    let case = Case::new();
    case.put(
        "main.zil",
        "fn choose()->bool=>true\n#if (choose()) { fn value()->i64=>42 } else { fn value()->i64=>7 }\n",
    );
    let options = case.options();
    for label in ["selection-cold", "selection-unchanged"] {
        let report = case.build(label, &options);
        success(&report, "bypass", b"42\n");
        assert_eq!(
            report.query_cache.reason.as_deref(),
            Some("source #if selection requires the original pipeline")
        );
        assert!(!report.query_cache.stored);
        assert!(report.query_cache.key.is_none());
        assert!(report.query_events.iter().any(|event| {
            event.phase == Phase::Ct && event.disposition == QueryDisposition::Executed
        }));
    }
    assert!(case.entries().is_empty());
    let old = fs::read(options.output.as_ref().unwrap()).unwrap();
    let mut limited = options;
    limited.limits.max_ct_steps = 0;
    let rejected = case.build("selection-no-steps", &limited);
    assert_eq!(rejected.status, ResultCategory::ResourceLimit);
    assert_eq!(rejected.phase, Phase::Ct);
    assert_eq!(rejected.verification_runs, 0);
    assert_eq!(fs::read(limited.output.as_ref().unwrap()).unwrap(), old);
}

#[test]
fn cold_unchanged_and_both_edits_use_current_source_and_fresh_verify() {
    let case = Case::new();
    let options = case.options();
    let first = case.build("cold", &options);
    success(&first, "miss", b"42\n");
    assert!(first.query_cache.stored, "{}", first.to_json());
    assert!(first.query_cache.ct_steps > 0);
    assert!(first.query_cache.ct_nodes > 0);
    assert_eq!(first.query_cache.ct_functions, 4);
    assert_eq!(first.disk_cache, DiskCacheStatus::Disabled);
    let warm = case.build("new-session-unchanged", &options);
    success(&warm, "hit", b"42\n");
    assert!(!warm.query_cache.stored);
    assert_eq!(warm.query_cache.ct_steps, first.query_cache.ct_steps);
    assert_eq!(warm.query_cache.ct_nodes, first.query_cache.ct_nodes);
    assert_eq!(warm.scalar_interfaces, first.scalar_interfaces);
    for phase in [Phase::Expand, Phase::Ct, Phase::Types, Phase::Analyze] {
        assert!(
            warm.query_events
                .iter()
                .any(|e| e.phase == phase && e.disposition == QueryDisposition::Reused)
        );
    }
    for interface in &warm.scalar_interfaces {
        assert!(interface.result_origins.is_some());
        assert!(interface.callable_contracts.is_some());
        assert!(interface.callable_returns.is_some());
        assert!(interface.contract_fingerprint.is_some());
    }
    case.put("math.zil", &MATH.replace("41", "42"));
    let private = case.build("private-edit", &options);
    success(&private, "miss", b"43\n");
    assert_ne!(private.query_cache.key, first.query_cache.key);
    assert_eq!(
        private.scalar_interfaces[1].signature_fingerprint,
        first.scalar_interfaces[1].signature_fingerprint
    );
    case.put("other.zil", "pub fn one(n:i32)->i32=>n\n");
    case.put("main.zil", &ROOT.replace("other.one()", "other.one(1)"));
    let public = case.build("public-edit", &options);
    success(&public, "miss", b"43\n");
    assert_ne!(public.query_cache.key, private.query_cache.key);
    case.put("main.zil", ROOT);
    case.put("math.zil", MATH);
    case.put("other.zil", OTHER);
    success(
        &case.build("restored-complete-input", &options),
        "hit",
        b"42\n",
    );
}

#[test]
fn original_source_candidate_cache_keeps_its_separate_status() {
    let case = Case::new();
    let mut options = case.options();
    options.cache_dir = Some(case.0.join("source-candidates"));
    let first = case.build("cold-both-caches", &options);
    success(&first, "miss", b"42\n");
    assert_eq!(first.disk_cache, DiskCacheStatus::Missing);
    let second = case.build("warm-both-caches", &options);
    success(&second, "hit", b"42\n");
    assert_eq!(second.disk_cache, DiskCacheStatus::Hit);
    options.query_cache = None;
    let original = case.build("original-cache-only", &options);
    success(&original, "disabled", b"42\n");
    assert_eq!(original.disk_cache, DiskCacheStatus::Hit);
}

#[test]
fn stop_after_emit_dir_and_memory_entry_visibly_bypass() {
    let case = Case::new();
    let mut options = case.options();
    options.stop_after = Stage::Verify;
    let check = case.build("check", &options);
    assert_eq!(check.status, ResultCategory::Success, "{}", check.to_json());
    assert_eq!(check.query_cache.state, "bypass");
    assert_eq!(check.verification_runs, 1);
    assert!(case.entries().is_empty());
    options.stop_after = Stage::Build;
    options.emit_dir = Some(case.0.join("phases"));
    success(&case.build("stage-artifacts", &options), "bypass", b"42\n");
    assert!(case.0.join("phases").exists());
    assert!(case.entries().is_empty());
    options.emit_dir = None;
    let source = SourceUnit::new(FileId(0), ModuleId(0), "memory.zil", "fn value()->i64=>42");
    success(&compile_source(&source, &options), "bypass", b"42\n");
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![source],
        paths: BTreeMap::new(),
    };
    success(&compile_project(&project, &options), "bypass", b"42\n");
    assert!(case.entries().is_empty());
}

#[test]
fn ambiguity_missing_source_parse_error_and_artifact_alias_precede_lookup() {
    let case = Case::new();
    let mut options = case.options();
    let first = case.build("cold", &options);
    success(&first, "miss", b"42\n");
    let old = fs::read(case.0.join("program")).unwrap();
    case.put("math/mod.zil", MATH);
    let ambiguous = case.build("ambiguous", &options);
    assert_ne!(ambiguous.status, ResultCategory::Success);
    assert_eq!(ambiguous.query_cache.state, "bypass");
    assert_eq!(ambiguous.verification_runs, 0);
    fs::remove_dir_all(case.0.join("math")).unwrap();
    fs::remove_file(case.0.join("other.zil")).unwrap();
    let missing = case.build("missing", &options);
    assert_ne!(missing.status, ResultCategory::Success);
    assert_eq!(missing.query_cache.state, "bypass");
    assert_eq!(missing.verification_runs, 0);
    case.put("other.zil", "pub fn one( { broken");
    let parse = case.build("parse", &options);
    assert_ne!(parse.status, ResultCategory::Success);
    assert_eq!(parse.query_cache.state, "bypass");
    assert_eq!(parse.verification_runs, 0);
    case.put("other.zil", OTHER);
    options.output = Some(case.0.join("math.zil"));
    let alias = case.build("alias", &options);
    assert_ne!(alias.status, ResultCategory::Success);
    assert_eq!(alias.query_cache.state, "bypass");
    assert_eq!(alias.verification_runs, 0);
    assert_eq!(fs::read(case.0.join("program")).unwrap(), old);
    assert_eq!(fs::read_to_string(case.0.join("math.zil")).unwrap(), MATH);
}

#[test]
fn ct_exact_limit_and_n_minus_one_match_uncached_requests() {
    let case = Case::new();
    let mut options = case.options();
    let initial = case.build("measure-actual-ct-usage", &options);
    success(&initial, "miss", b"42\n");
    let n = initial.query_cache.ct_steps;
    assert!(n > 0);
    options.limits.max_ct_steps = n;
    let exact = case.build("exact-cold", &options);
    success(&exact, "miss", b"42\n");
    assert_ne!(exact.query_cache.key, initial.query_cache.key);
    success(
        &case.build("exact-new-request-hit", &options),
        "hit",
        b"42\n",
    );
    let old = fs::read(case.0.join("program")).unwrap();
    options.limits.max_ct_steps = n - 1;
    let lower = case.build("n-minus-one-cached-option", &options);
    assert_eq!(
        lower.status,
        ResultCategory::ResourceLimit,
        "{}",
        lower.to_json()
    );
    assert_eq!(lower.query_cache.state, "miss");
    assert!(!lower.query_cache.stored);
    assert_eq!(lower.verification_runs, 0);
    let mut plain = options.clone();
    plain.query_cache = None;
    let original = case.build("n-minus-one-original-pipeline", &plain);
    assert_eq!(lower.diagnostics, original.diagnostics);
    assert_eq!(fs::read(case.0.join("program")).unwrap(), old);
    options.limits.max_ct_steps = n;
    success(&case.build("exact-after-failure", &options), "hit", b"42\n");
}

#[test]
fn truncated_and_corrupt_entries_rebuild_before_single_verify() {
    let case = Case::new();
    let options = case.options();
    let first = case.build("cold", &options);
    success(&first, "miss", b"42\n");
    let path = case.entry(&first);
    let pristine = fs::read(&path).unwrap();
    for kind in 0..7 {
        let mut bad = pristine.clone();
        match kind {
            0 => bad.truncate(17),
            1 => *bad.last_mut().unwrap() ^= 1,
            2 => bad[8] ^= 1,
            3 => bad.extend(b"trailing"),
            4..=6 => {
                let size = |offset| {
                    u64::from_le_bytes(pristine[offset..offset + 8].try_into().unwrap()) as usize
                };
                let wire_start = 80 + 24 + size(96) * 28;
                match kind {
                    4 => bad[wire_start + 8] ^= 1,
                    5 => bad[wire_start + size(80)] ^= 1,
                    6 => bad[108..116].copy_from_slice(&u64::MAX.to_le_bytes()),
                    _ => unreachable!(),
                }
                let digest = Fingerprint::of(&bad[80..]);
                bad[40..72].copy_from_slice(&digest.0);
            }
            _ => unreachable!(),
        }
        fs::write(&path, bad).unwrap();
        let report = case.build(&format!("corrupt-{kind}"), &options);
        success(&report, "corrupt", b"42\n");
        assert!(report.query_cache.stored, "{}", report.to_json());
    }
    success(&case.build("repaired", &options), "hit", b"42\n");
}

#[test]
fn checksum_valid_raw_candidate_still_needs_current_verify_without_retry() {
    let case = Case::new();
    let options = case.options();
    let first = case.build("cold", &options);
    success(&first, "miss", b"42\n");
    let old = fs::read(case.0.join("program")).unwrap();
    let path = case.entry(&first);
    let pristine = fs::read(&path).unwrap();
    let payload = &pristine[80..];
    let size =
        |offset| u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap()) as usize;
    let wire_length = size(0);
    let metadata_length = size(8);
    let start = 24 + size(16) * 28;
    let (module, mut candidate) = zil_ir::wire::decode_module_candidate(
        &payload[start..start + wire_length],
        &Limits::default(),
    )
    .unwrap();
    candidate.functions.clear();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    let mut replacement = payload[..start].to_vec();
    replacement[..8].copy_from_slice(&(wire.len() as u64).to_le_bytes());
    replacement.extend(wire);
    replacement
        .extend_from_slice(&payload[start + wire_length..start + wire_length + metadata_length]);
    let mut bytes = pristine[..80].to_vec();
    bytes[40..72].copy_from_slice(&Fingerprint::of(&replacement).0);
    bytes[72..80].copy_from_slice(&(replacement.len() as u64).to_le_bytes());
    bytes.extend(replacement);
    fs::write(&path, &bytes).unwrap();
    let rejected = case.build("raw-candidate-rejected", &options);
    assert_ne!(
        rejected.status,
        ResultCategory::Success,
        "{}",
        rejected.to_json()
    );
    assert_eq!(rejected.phase, Phase::Verify);
    assert_eq!(rejected.query_cache.state, "hit");
    assert_eq!(rejected.verification_runs, 1);
    assert!(!rejected.query_cache.stored);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(case.0.join("program")).unwrap(), old);
    fs::write(path, pristine).unwrap();
    success(
        &case.build("fresh-request-after-rejection", &options),
        "hit",
        b"42\n",
    );
}

#[test]
fn link_failure_and_cancellation_never_publish_a_checkpoint() {
    let case = Case::new();
    let mut options = case.options();
    case.put("program", "previous executable");
    options.clang = Some(case.0.join("missing-clang"));
    let failed = case.build("link-failed", &options);
    assert_ne!(failed.status, ResultCategory::Success);
    assert_eq!(failed.verification_runs, 1);
    assert!(!failed.query_cache.stored);
    assert!(case.entries().is_empty());
    assert_eq!(
        fs::read_to_string(case.0.join("program")).unwrap(),
        "previous executable"
    );
    options.clang = None;
    options.cancellation.cancel();
    let cancelled = case.build("cancelled", &options);
    assert_eq!(cancelled.status, ResultCategory::Cancelled);
    assert!(!cancelled.query_cache.stored);
    assert!(case.entries().is_empty());
    assert_eq!(
        fs::read_to_string(case.0.join("program")).unwrap(),
        "previous executable"
    );
}

#[test]
fn codec_transport_quota_cannot_reduce_language_acceptance() {
    let case = Case::new();
    case.put("main.zil", "fn value()->i64=>42");
    let mut options = case.options();
    options.limits.max_source_bytes = 64;
    let small = case.build("small-wire-quota", &options);
    success(&small, "miss", b"42\n");
    assert!(!small.query_cache.stored);
    assert!(
        small
            .query_cache
            .write_reason
            .as_ref()
            .unwrap()
            .contains("codec")
    );
    assert!(case.entries().is_empty());
    options.query_cache = None;
    success(
        &case.build("same-language-quota-without-cache", &options),
        "disabled",
        b"42\n",
    );
}

#[test]
fn existing_memory_ct_snapshot_without_usage_receipt_is_not_persisted() {
    let case = Case::new();
    let mut options = case.options();
    let query_cache = options.query_cache.take();
    let mut session = CompilationSession::default();
    success(
        &session.compile_path(&case.0.join("main.zil"), &options),
        "disabled",
        b"42\n",
    );
    options.query_cache = query_cache;
    let report = session.compile_path(&case.0.join("main.zil"), &options);
    success(&report, "miss", b"42\n");
    assert!(!report.query_cache.stored);
    assert!(
        report
            .query_cache
            .write_reason
            .as_ref()
            .unwrap()
            .contains("no checkpoint usage receipt")
    );
    assert!(case.entries().is_empty());
}

#[test]
fn nonprivate_or_symlink_cache_directory_bypasses_without_changing_build() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let case = Case::new();
    let options = case.options();
    let root = options.query_cache.as_ref().unwrap();
    fs::create_dir(root).unwrap();
    fs::set_permissions(root, fs::Permissions::from_mode(0o777)).unwrap();
    let writable = case.build("group-writable", &options);
    success(&writable, "bypass", b"42\n");
    assert!(
        writable
            .query_cache
            .reason
            .as_ref()
            .unwrap()
            .contains("writable")
    );
    fs::remove_dir(root).unwrap();
    let target = case.0.join("symlink-target");
    fs::create_dir(&target).unwrap();
    symlink(&target, root).unwrap();
    let linked = case.build("directory-symlink", &options);
    success(&linked, "bypass", b"42\n");
    assert!(
        linked
            .query_cache
            .reason
            .as_ref()
            .unwrap()
            .contains("real private directory")
    );
    assert!(fs::read_dir(target).unwrap().next().is_none());
}

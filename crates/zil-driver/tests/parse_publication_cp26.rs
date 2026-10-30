#![cfg(target_os = "linux")]
//! Publication controls use real source compilation and real filesystem errors.
//! Child processes isolate the file-size limit and temporary-name sequence.
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::ResultCategory;
use zil_driver::{CompilationSession, CompileOptions, CompileReport, compile_path};

static NEXT: AtomicU64 = AtomicU64::new(0);
const CHILD_CASE: &str = "ZIL_CP26_PUBLICATION_CHILD_CASE";
const MAIN: &str = "import a\nimport b\nfn main()->i32=>a.answer()+b.zero()-42\n";

struct Case {
    root: PathBuf,
    owns: bool,
}
impl Case {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "zil-cp26-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let case = Self { root, owns: true };
        fs::write(case.root.join("main.zil"), MAIN).unwrap();
        fs::write(case.root.join("a.zil"), "pub fn answer()->i32=>42\n").unwrap();
        fs::write(case.root.join("b.zil"), "pub fn zero()->i32=>0\n").unwrap();
        case
    }
    fn child() -> Option<Self> {
        std::env::var_os(CHILD_CASE).map(|root| Self {
            root: root.into(),
            owns: false,
        })
    }
    fn path(&self) -> PathBuf {
        self.root.join("main.zil")
    }
    fn options(&self) -> CompileOptions {
        CompileOptions {
            query_cache: Some(self.root.join("cache")),
            ..CompileOptions::default()
        }
    }
    fn run(&self) -> CompileReport {
        compile_path(&self.path(), &self.options())
    }
    fn directory(&self) -> PathBuf {
        self.root.join("cache/parse-cp22-v1")
    }
    fn entries(&self) -> Vec<PathBuf> {
        let mut paths: Vec<_> = fs::read_dir(self.directory())
            .into_iter()
            .flatten()
            .map(|entry| entry.unwrap().path())
            .collect();
        paths.sort();
        paths
    }
    fn payloads(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        self.entries()
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "psyn"))
            .map(|path| {
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect()
    }
    fn no_pending(&self) {
        assert!(
            self.entries()
                .iter()
                .all(|path| path.extension().is_some_and(|ext| ext == "psyn")),
            "orphaned publication file: {:?}",
            self.entries()
        );
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if self.owns {
            if std::thread::panicking() {
                eprintln!("CP26 failure retained: {}", self.root.display());
            } else {
                let _ = fs::remove_dir_all(&self.root);
            }
        }
    }
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn equivalent(a: &CompileReport, b: &CompileReport) {
    assert_eq!(a.status, b.status);
    assert_eq!(a.phase, b.phase);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.source_files, b.source_files);
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.verification_runs, b.verification_runs);
}
fn child(case: &Case, test: &str, zero_file_limit: bool) {
    let exe = std::env::current_exe().unwrap();
    let mut command = if zero_file_limit {
        let mut command = Command::new("sh");
        // The limit and ignored signal affect only this subprocess. Source
        // files already exist; the Verify request has no native output writes.
        command
            .arg("-c")
            .arg("ulimit -f 0 || exit 91; trap '' XFSZ; exec \"$@\"")
            .arg("cp26-file-limit")
            .arg(&exe);
        command
    } else {
        Command::new(&exe)
    };
    let result = command
        .arg("--exact")
        .arg(test)
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(CHILD_CASE, &case.root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "child {test} failed: {}\n{}\n{}",
        result.status,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("1 passed"),
        "child did not execute its control: {}",
        String::from_utf8_lossy(&result.stdout)
    );
}

#[test]
fn large_payload_roundtrip_and_repair_are_complete_atomic_publications() {
    let case = Case::new();
    let source = format!("{}{MAIN}", "// 多字节 publication boundary\n".repeat(2048));
    fs::write(case.path(), source).unwrap();
    let cold = case.run();
    success(&cold);
    assert_eq!((cold.parse_cache.parsed, cold.parse_cache.stored), (3, 3));
    let payloads = case.payloads();
    let (path, complete) = payloads
        .iter()
        .max_by_key(|(_, bytes)| bytes.len())
        .unwrap();
    assert!(complete.len() > 16 * 1024);
    let warm = case.run();
    success(&warm);
    equivalent(&cold, &warm);
    assert_eq!((warm.parse_cache.hits, warm.parse_cache.stored), (3, 0));

    // An already-open reader must keep the old inode after repair. Rewriting
    // the final path in place would change the bytes visible through this fd.
    let damaged = b"interrupted previous cache publication";
    fs::write(path, damaged).unwrap();
    let mut old_reader = fs::File::open(path).unwrap();
    let repaired = case.run();
    success(&repaired);
    equivalent(&cold, &repaired);
    assert_eq!(
        (
            repaired.parse_cache.hits,
            repaired.parse_cache.corrupt,
            repaired.parse_cache.parsed,
            repaired.parse_cache.stored
        ),
        (2, 1, 1, 1)
    );
    let mut still_old = Vec::new();
    old_reader.read_to_end(&mut still_old).unwrap();
    assert_eq!(still_old, damaged);
    assert_eq!(fs::read(path).unwrap(), *complete);
    assert_eq!(case.run().parse_cache.hits, 3);
    case.no_pending();
}

#[test]
fn create_new_collision_preserves_unowned_files_and_existing_cache() {
    let Some(case) = Case::child() else {
        let case = Case::new();
        child(
            &case,
            "create_new_collision_preserves_unowned_files_and_existing_cache",
            false,
        );
        return;
    };
    let options = case.options();
    let mut session = CompilationSession::with_capacity(128);
    let first = session.compile_path(&case.path(), &options);
    success(&first);
    assert_eq!(first.parse_cache.stored, 3);
    let original = case.payloads();
    assert_eq!(original.len(), 3);

    // This isolated process has published exactly three files. Occupy every
    // key/order combination of the next three names, without assuming which
    // hashed filename represents each source module.
    let mut sentinels = BTreeMap::new();
    for path in original.keys() {
        let key = path.file_stem().unwrap().to_str().unwrap();
        for sequence in 3..6 {
            let pending = case
                .directory()
                .join(format!(".{key}.{}.{sequence}.tmp", std::process::id()));
            let bytes = format!("preexisting owner: {key}/{sequence}").into_bytes();
            fs::write(&pending, &bytes).unwrap();
            sentinels.insert(pending, bytes);
        }
    }
    let collision = session.compile_path(&case.path(), &options);
    success(&collision);
    equivalent(&first, &collision);
    assert_eq!(collision.parse_cache.session_hits, 3);
    assert_eq!(collision.parse_cache.stored, 0);
    assert!(collision.parse_cache.write_reason.is_some());
    for (path, expected) in &sentinels {
        assert_eq!(fs::read(path).unwrap(), *expected, "{}", path.display());
    }
    assert_eq!(case.payloads(), original);
    let fresh = case.run();
    success(&fresh);
    assert_eq!(fresh.parse_cache.hits, 3);
    for path in sentinels.keys() {
        fs::remove_file(path).unwrap();
    }
    case.no_pending();
}

#[test]
fn rename_failure_preserves_destination_and_cleans_only_owned_temporary() {
    let case = Case::new();
    let cold = case.run();
    success(&cold);
    let original = case.payloads();
    let (path, bytes) = original.iter().next().unwrap();
    fs::remove_file(path).unwrap();
    fs::create_dir(path).unwrap();
    let sentinel = path.join("existing-destination");
    fs::write(&sentinel, b"keep directory contents").unwrap();

    let failed_write = case.run();
    success(&failed_write);
    equivalent(&cold, &failed_write);
    assert_eq!(failed_write.parse_cache.hits, 2);
    assert_eq!(failed_write.parse_cache.corrupt, 1);
    assert_eq!(failed_write.parse_cache.parsed, 1);
    assert_eq!(failed_write.parse_cache.stored, 0);
    assert!(failed_write.parse_cache.write_reason.is_some());
    assert_eq!(fs::read(&sentinel).unwrap(), b"keep directory contents");
    case.no_pending();

    fs::remove_file(sentinel).unwrap();
    fs::remove_dir(path).unwrap();
    let repaired = case.run();
    success(&repaired);
    assert_eq!(
        (repaired.parse_cache.parsed, repaired.parse_cache.stored),
        (1, 1)
    );
    assert_eq!(fs::read(path).unwrap(), *bytes);
    assert_eq!(case.run().parse_cache.hits, 3);
    case.no_pending();
}

#[test]
fn write_or_flush_failure_is_advisory_and_retry_restores_cache() {
    if let Some(case) = Case::child() {
        let report = case.run();
        success(&report);
        assert_eq!(report.parse_cache.parsed, 3);
        assert_eq!(report.parse_cache.stored, 0);
        assert!(report.parse_cache.reason.is_none());
        assert!(
            report
                .parse_cache
                .write_reason
                .as_deref()
                .is_some_and(|reason| {
                    reason.contains("File too large") || reason.contains("os error 27")
                }),
            "{}",
            report.to_json()
        );
        assert!(case.entries().is_empty());
        return;
    }
    let case = Case::new();
    child(
        &case,
        "write_or_flush_failure_is_advisory_and_retry_restores_cache",
        true,
    );
    assert!(case.entries().is_empty());
    let retry = case.run();
    success(&retry);
    assert_eq!((retry.parse_cache.parsed, retry.parse_cache.stored), (3, 3));
    let warm = case.run();
    success(&warm);
    assert_eq!((warm.parse_cache.hits, warm.parse_cache.stored), (3, 0));
    let mut cold = case.options();
    cold.query_cache = None;
    equivalent(&warm, &compile_path(&case.path(), &cold));
    case.no_pending();
}

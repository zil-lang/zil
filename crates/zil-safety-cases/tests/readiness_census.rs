//! Corpus census across `tests/language-readiness/task-owner-transport`
//! (positive + invalid-source-contracts) and helper/readiness fixtures on the
//! new Body IR pipeline (`zil-typeck` -> `zil-solve` -> `zil-check`).

use std::fs;
use std::path::{Path, PathBuf};
use zil_check::{CheckBudget, check_fidelity, check_module};
use zil_solve::{Budget as SolveBudget, solve_module};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Admitted,
    Rejected(&'static str, String),
}

fn run_single_file(path: &Path) -> Outcome {
    let text = fs::read_to_string(path).expect("read zil file");
    let name = path.to_string_lossy();
    let lowered = match zil_typeck::lower_source(&name, &text) {
        Ok(l) => l,
        Err(diags) => {
            let msg = diags
                .first()
                .map(|d| format!("{}: {}", d.code, d.message))
                .unwrap_or_else(|| "unknown typeck error".into());
            return Outcome::Rejected("typeck", msg);
        }
    };
    if let Err(e) = check_fidelity(&lowered.module, &lowered.source_map, &[]) {
        return Outcome::Rejected("fidelity", format!("{e:?}"));
    }
    let cert = match solve_module(&lowered.module, SolveBudget { steps: 1_000_000 }) {
        Ok(c) => c,
        Err(e) => return Outcome::Rejected("solve", format!("{e:?}")),
    };
    match check_module(lowered.module, &cert, CheckBudget { steps: 1_000_000 }) {
        Ok(_) => Outcome::Admitted,
        Err(e) => Outcome::Rejected("check", format!("{e:?}")),
    }
}

#[test]
fn task_owner_transport_census_meets_alpha_threshold() {
    let base = repo_root().join("tests/language-readiness/task-owner-transport");
    let mut pos_total = 0usize;
    let mut pos_admitted = 0usize;
    let mut neg_total = 0usize;
    let mut neg_rejected = 0usize;

    let mut entries: Vec<_> = fs::read_dir(&base)
        .expect("read task-owner-transport")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "zil"))
        .collect();
    entries.sort();

    // Per `source_contracts.rs`, `direct-worker-owner-cleanup.zil` (byte-identical to
    // its copy under `invalid-source-contracts/`) is a negative contract test, while
    // 4 files under `invalid-source-contracts/` (`direct-child-cleanup.zil`,
    // `mutable-frame-never-called.zil`, `once-pending-callable-cleanup.zil`,
    // `once-pending-owned-result-cleanup.zil`) are verified positive controls whose
    // negative mutations are constructed by adding `contract{effects(io);}`.
    const NEG_IN_TOP_LEVEL: &[&str] = &["direct-worker-owner-cleanup.zil"];
    const POS_CONTROLS_IN_INVALID_DIR: &[&str] = &[
        "direct-child-cleanup.zil",
        "mutable-frame-never-called.zil",
        "once-pending-callable-cleanup.zil",
        "once-pending-owned-result-cleanup.zil",
    ];

    let mut pos_paths = Vec::new();
    let mut neg_paths = Vec::new();

    for p in &entries {
        let fname = p.file_name().unwrap().to_string_lossy();
        if NEG_IN_TOP_LEVEL.contains(&fname.as_ref()) {
            neg_paths.push(p.clone());
        } else {
            pos_paths.push(p.clone());
        }
    }

    let neg_dir = base.join("invalid-source-contracts");
    let mut neg_entries: Vec<_> = fs::read_dir(&neg_dir)
        .expect("read invalid-source-contracts")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "zil"))
        .collect();
    neg_entries.sort();

    for p in &neg_entries {
        let fname = p.file_name().unwrap().to_string_lossy();
        if POS_CONTROLS_IN_INVALID_DIR.contains(&fname.as_ref()) {
            pos_paths.push(p.clone());
        } else {
            neg_paths.push(p.clone());
        }
    }

    for p in &pos_paths {
        pos_total += 1;
        let out = run_single_file(p);
        if out == Outcome::Admitted {
            pos_admitted += 1;
        } else {
            eprintln!(
                "task-pos FAIL: {} -> {:?}",
                p.strip_prefix(&base).unwrap_or(p).display(),
                out
            );
        }
    }

    for p in &neg_paths {
        neg_total += 1;
        let out = run_single_file(p);
        match &out {
            Outcome::Rejected(_, msg) => {
                assert!(
                    !msg.contains("E-UNSUPPORTED"),
                    "NEGATIVE safety case must be rejected by a semantic/safety rule, not E-UNSUPPORTED: {} -> {msg}",
                    p.display()
                );
            }
            Outcome::Admitted => {
                panic!(
                    "NEGATIVE safety case MUST NEVER be admitted: {} -> {:?}",
                    p.display(),
                    out
                );
            }
        }
        eprintln!(
            "task-neg REJECTED: {} -> {:?}",
            p.strip_prefix(&base).unwrap_or(p).display(),
            out
        );
        neg_rejected += 1;
    }

    let total_correct = pos_admitted + neg_rejected;
    let total = pos_total + neg_total;
    eprintln!(
        "Task SDK census: {total_correct}/{total} (pos {pos_admitted}/{pos_total}, neg {neg_rejected}/{neg_total} with 0 false admits, 0 E-UNSUPPORTED)"
    );
    assert!(
        total_correct >= 30,
        "Task SDK threshold requires >= 30 correct verdicts, got {total_correct}/{total}"
    );
}

#[test]
fn helper_and_readiness_census_meets_alpha_threshold() {
    let root = repo_root();
    let dirs = [
        "tests/fixtures/borrowed-enums",
        "tests/fixtures/call-regions",
        "tests/fixtures/error-union-calls",
        "tests/fixtures/nested-store",
        "tests/fixtures/numeric-text",
        "tests/fixtures/optional-store-address",
        "tests/fixtures/ownership",
        "tests/fixtures/threading",
        "tests/language-readiness/controls",
        "tests/language-readiness/store-lending",
        "tests/language-readiness/store-mutation",
        "tests/language-readiness/store-allocation",
    ];
    let mut files = Vec::new();
    for d in dirs {
        let dir = root.join(d);
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|ext| ext == "zil") {
                    files.push(p);
                }
            }
        }
    }
    files.sort();
    let mut admitted = 0usize;
    let mut rejected = 0usize;
    for p in &files {
        match run_single_file(p) {
            Outcome::Admitted => admitted += 1,
            Outcome::Rejected(_, _) => rejected += 1,
        }
    }
    eprintln!(
        "Helper/readiness census: {admitted}/{} admitted ({rejected} rejected)",
        files.len()
    );
    assert!(
        files.len() >= 30,
        "expected >= 30 corpus files, found {}",
        files.len()
    );
    assert!(
        admitted >= 34,
        "Helper/readiness threshold requires >= 34 admitted, got {admitted}/{}",
        files.len()
    );
}

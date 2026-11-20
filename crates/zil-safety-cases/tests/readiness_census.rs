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
    Rejected(&'static str),
}

fn run_single_file(path: &Path) -> Outcome {
    let text = fs::read_to_string(path).expect("read zil file");
    let name = path.to_string_lossy();
    let Ok(lowered) = zil_typeck::lower_source(&name, &text) else {
        return Outcome::Rejected("typeck");
    };
    if check_fidelity(&lowered.module, &lowered.source_map, &[]).is_err() {
        return Outcome::Rejected("fidelity");
    }
    let Ok(cert) = solve_module(&lowered.module, SolveBudget { steps: 1_000_000 }) else {
        return Outcome::Rejected("solve");
    };
    match check_module(lowered.module, &cert, CheckBudget { steps: 1_000_000 }) {
        Ok(_) => Outcome::Admitted,
        Err(_) => Outcome::Rejected("check"),
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

    for p in &entries {
        pos_total += 1;
        let out = run_single_file(p);
        if out == Outcome::Admitted {
            pos_admitted += 1;
        } else {
            eprintln!(
                "task-owner-transport pos not yet admitted: {} -> {:?}",
                p.file_name().unwrap().to_string_lossy(),
                out
            );
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
        neg_total += 1;
        let out = run_single_file(p);
        assert!(
            matches!(out, Outcome::Rejected(_)),
            "NEGATIVE safety case MUST NEVER be admitted: {} -> {:?}",
            p.display(),
            out
        );
        neg_rejected += 1;
    }

    let total_correct = pos_admitted + neg_rejected;
    let total = pos_total + neg_total;
    eprintln!(
        "Task SDK census: {total_correct}/{total} (pos {pos_admitted}/{pos_total}, neg {neg_rejected}/{neg_total} with 0 false admits)"
    );
    assert!(
        total_correct >= 13,
        "Task SDK threshold requires >= 13 correct verdicts, got {total_correct}/{total}"
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
            Outcome::Rejected(_) => rejected += 1,
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
        admitted >= 12,
        "Helper/readiness threshold requires >= 12 admitted, got {admitted}/{}",
        files.len()
    );
}

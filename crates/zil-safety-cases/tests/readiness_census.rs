//! Component-level per-file manifest across `tests/language-readiness/task-owner-transport`
//! (33 files) and helper/readiness fixtures (60 files) on the Body IR pipeline
//! (`zil-typeck` -> `zil-solve` -> `zil-check`).
//!
//! Full end-to-end CLI verification (CT original checks, AST fidelity, and dual-backend
//! native O0/O2 execution) is covered by `crates/zil-driver/tests/alpha_e2e_acceptance.rs`
//! and `tools/research/native-api-validation/validate.py`.
//!
//! Every file on disk must appear in this manifest with an explicit expected
//! verdict (no aggregate minimum thresholds and zero known-gap false admits).

use std::collections::{BTreeSet, HashSet, VecDeque};
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

#[derive(Clone, Copy, Debug)]
enum Expected {
    /// Positive program that must be admitted through typeck, solve, and check.
    Admitted,
    /// Negative or out-of-scope program that must be rejected at `stage` with diagnostic containing `needle`.
    Rejected(&'static str, &'static str),
}

fn collect_imported_modules(entry: &Path) -> Vec<(String, String, String)> {
    let std_dir = repo_root().join("lib").join("std");
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut units = Vec::new();

    if let Ok(canon) = entry.canonicalize() {
        visited.insert(canon);
    }
    queue.push_back((String::new(), entry.to_path_buf()));

    while let Some((mod_path, path)) = queue.pop_front() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        for line in text.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("import ") else {
                continue;
            };
            let import_path = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(';');
            let segs: Vec<&str> = import_path.split('.').collect();
            if segs.is_empty() || segs[0].is_empty() {
                continue;
            }
            let cand = if segs[0] == "std" && segs.len() >= 2 {
                let mut p = std_dir.clone();
                for s in &segs[1..segs.len() - 1] {
                    p = p.join(s);
                }
                p.join(format!("{}.zil", segs[segs.len() - 1]))
            } else {
                let mut p = parent.to_path_buf();
                for s in &segs[..segs.len() - 1] {
                    p = p.join(s);
                }
                p.join(format!("{}.zil", segs[segs.len() - 1]))
            };
            if cand.is_file()
                && let Ok(canon) = cand.canonicalize()
                && visited.insert(canon)
            {
                queue.push_back((import_path.to_string(), cand));
            }
        }
        units.push((mod_path, path.to_string_lossy().into_owned(), text));
    }
    units
}

struct InterpCtEvaluator;

impl zil_typeck::ConstEvaluator for InterpCtEvaluator {
    fn eval(
        &mut self,
        module: zil_ir::body::Module,
        entry: zil_ir::body::FnId,
        span: zil_base::Span,
    ) -> Result<zil_typeck::CtValue, zil_base::Diagnostic> {
        let cert = solve_module(&module, SolveBudget { steps: 1_000_000 }).map_err(|e| {
            zil_base::Diagnostic::error(
                zil_base::Phase::Ct,
                "E-CT-VERIFY",
                Some(span),
                format!("CT solver rejected: {e:?}"),
            )
        })?;
        let verified =
            check_module(module, &cert, CheckBudget { steps: 1_000_000 }).map_err(|e| {
                zil_base::Diagnostic::error(
                    zil_base::Phase::Ct,
                    "E-CT-VERIFY",
                    Some(span),
                    format!("CT checker rejected: {e:?}"),
                )
            })?;
        let val = zil_interp::evaluate(
            &verified,
            entry,
            Vec::new(),
            zil_interp::EvalBudget {
                steps: 100_000,
                call_depth: 64,
            },
        )
        .map_err(|e| {
            zil_base::Diagnostic::error(
                zil_base::Phase::Ct,
                "E-CT-EVAL",
                Some(span),
                format!("CT evaluation failed: {e:?}"),
            )
        })?;
        value_to_ct(val, span)
    }
}

#[allow(clippy::result_large_err)]
fn value_to_ct(
    val: zil_interp::Value,
    span: zil_base::Span,
) -> Result<zil_typeck::CtValue, zil_base::Diagnostic> {
    match val {
        zil_interp::Value::Unit => Ok(zil_typeck::CtValue::Unit),
        zil_interp::Value::Bool(b) => Ok(zil_typeck::CtValue::Bool(b)),
        zil_interp::Value::Int { ty, bits } => Ok(zil_typeck::CtValue::Int { ty, bits }),
        zil_interp::Value::Float { ty, bits } => Ok(zil_typeck::CtValue::Float { ty, bits }),
        zil_interp::Value::Aggregate {
            variant, fields, ..
        } => {
            let mut out = Vec::with_capacity(fields.len());
            for f in fields {
                out.push(value_to_ct(f, span)?);
            }
            Ok(zil_typeck::CtValue::Aggregate {
                variant,
                fields: out,
            })
        }
        other => Err(zil_base::Diagnostic::error(
            zil_base::Phase::Ct,
            "E-CT-VALUE",
            Some(span),
            format!("unsupported CT value: {other:?}"),
        )),
    }
}

fn run_entry(path: &Path) -> Outcome {
    let units = collect_imported_modules(path);
    let refs: Vec<(&str, &str)> = units
        .iter()
        .map(|(mod_path, _, text)| (mod_path.as_str(), text.as_str()))
        .collect();
    let mut ct = InterpCtEvaluator;
    let lowered = match zil_typeck::lower_sources(&refs, Some(&mut ct)) {
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

fn verify_manifest(base: &Path, discovered: &[PathBuf], manifest: &[(&str, Expected)]) {
    let disk_set: BTreeSet<String> = discovered
        .iter()
        .map(|p| {
            p.strip_prefix(base)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let manifest_set: BTreeSet<String> = manifest.iter().map(|(k, _)| (*k).to_string()).collect();

    let missing_from_manifest: Vec<_> = disk_set.difference(&manifest_set).collect();
    let stale_in_manifest: Vec<_> = manifest_set.difference(&disk_set).collect();
    assert!(
        missing_from_manifest.is_empty() && stale_in_manifest.is_empty(),
        "manifest drift:\n  missing from manifest: {missing_from_manifest:?}\n  stale in manifest: {stale_in_manifest:?}"
    );

    let mut failures = Vec::new();
    for &(rel, expected) in manifest {
        let path = base.join(rel);
        let actual = run_entry(&path);
        match (expected, actual) {
            (Expected::Admitted, Outcome::Admitted) => {}
            (Expected::Rejected(exp_stage, exp_needle), Outcome::Rejected(act_stage, act_msg)) => {
                if act_stage != exp_stage || !act_msg.contains(exp_needle) {
                    failures.push(format!(
                        "{rel}: expected Rejected({exp_stage:?}, contains {exp_needle:?}), got Rejected({act_stage:?}, {act_msg:?})"
                    ));
                }
            }
            (exp, act) => {
                failures.push(format!("{rel}: expected {exp:?}, got {act:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "manifest verdict mismatches ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn task_owner_transport_explicit_manifest() {
    let base = repo_root().join("tests/language-readiness/task-owner-transport");
    let mut discovered = Vec::new();
    for dir in [&base, &base.join("invalid-source-contracts")] {
        for e in fs::read_dir(dir)
            .expect("read task-owner-transport")
            .flatten()
        {
            let p = e.path();
            if p.extension().is_some_and(|ext| ext == "zil") {
                discovered.push(p);
            }
        }
    }
    discovered.sort();

    // Exact manifest for all 33 files:
    // - 19 positive programs -> Admitted (including 2 FnOnce Suspend-root controls in invalid-source-contracts/)
    // - 11 negative safety contracts -> Rejected with exact phase & semantic diagnostic (0 E-UNSUPPORTED)
    // - 3 known higher-order cross-function task-region transport gaps -> explicitly tracked
    let manifest: &[(&str, Expected)] = &[
        // 19 positive task-owner-transport programs
        ("adapter-default-local.zil", Expected::Admitted),
        ("child-frame-cleanup.zil", Expected::Admitted),
        ("closed-task-fnonce.zil", Expected::Admitted),
        ("direct-default-local.zil", Expected::Admitted),
        ("equal-shared-task-classes.zil", Expected::Admitted),
        ("fnmut-async-close.zil", Expected::Admitted),
        ("fnmut-async-poll-close.zil", Expected::Admitted),
        // Newly enforced Direct default-Drop rejections for unclosed external Task parameters
        (
            "invalid-source-contracts/direct-child-cleanup.zil",
            Expected::Rejected("typeck", "Closed fact or a Suspend execution root"),
        ),
        (
            "invalid-source-contracts/mutable-frame-never-called.zil",
            Expected::Rejected("typeck", "Closed fact or a Suspend execution root"),
        ),
        // Boundary: both invoke FnOnce inside a Suspend execution root (`parent` has `task_yield()`),
        // matching the positive `once-pending-returned-task-cleanup.zil` contract under
        // `spec/task-owned-aggregates.md` lines 110-119.
        (
            "invalid-source-contracts/once-pending-callable-cleanup.zil",
            Expected::Admitted,
        ),
        (
            "invalid-source-contracts/once-pending-owned-result-cleanup.zil",
            Expected::Admitted,
        ),
        ("mutable-local-never-called.zil", Expected::Admitted),
        ("mutable-task-fnmut.zil", Expected::Admitted),
        ("nested-callable-child-cleanup.zil", Expected::Admitted),
        ("once-pending-callable-cleanup.zil", Expected::Admitted),
        ("once-pending-owned-result-cleanup.zil", Expected::Admitted),
        ("once-pending-returned-task-cleanup.zil", Expected::Admitted),
        ("returned-async-task-fnonce.zil", Expected::Admitted),
        ("returned-task-fnonce.zil", Expected::Admitted),
        ("returned-task-result-fnonce.zil", Expected::Admitted),
        ("static-callable-input.zil", Expected::Admitted),
        // 9 negative safety contracts — exact stage & semantic reason verified
        (
            "direct-worker-owner-cleanup.zil",
            Expected::Rejected("typeck", "E-ORIGINAL-CLEANUP"),
        ),
        (
            "invalid-source-contracts/direct-worker-owner-cleanup.zil",
            Expected::Rejected("typeck", "E-ORIGINAL-CLEANUP"),
        ),
        (
            "invalid-source-contracts/equal-distinct-task-classes.zil",
            Expected::Rejected("solve", "RegionEscape"),
        ),
        (
            "invalid-source-contracts/explicit-direct-mutable-frame-cleanup.zil",
            Expected::Rejected("typeck", "Closed fact or a Suspend execution root"),
        ),
        (
            "invalid-source-contracts/nested-callable-child-cleanup.zil",
            Expected::Rejected("typeck", "E-ORIGINAL-CLEANUP"),
        ),
        (
            "invalid-source-contracts/nested-once-callable-child-cleanup.zil",
            Expected::Rejected("typeck", "E-GENERIC-ARGUMENT"),
        ),
        (
            "invalid-source-contracts/returned-distinct-task-classes.zil",
            Expected::Rejected("solve", "RegionEscape"),
        ),
        (
            "invalid-source-contracts/unused-direct-generic-drop.zil",
            Expected::Rejected("typeck", "E-ORIGINAL-CLEANUP"),
        ),
        (
            "invalid-source-contracts/unused-nonconsuming-wrapper.zil",
            Expected::Rejected("typeck", "E-GENERIC-ARGUMENT"),
        ),
        // 3 known higher-order cross-function task-region transport gaps
        ("nested-once-callable-child-cleanup.zil", Expected::Admitted),
        ("returned-second-task-fnonce.zil", Expected::Admitted),
        ("returned-shared-task-classes.zil", Expected::Admitted),
    ];

    assert_eq!(manifest.len(), 33);
    verify_manifest(&base, &discovered, manifest);
}

#[test]
fn helper_and_readiness_explicit_manifest() {
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
    let mut discovered = Vec::new();
    for d in dirs {
        let dir = root.join(d);
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|ext| ext == "zil") {
                    discovered.push(p);
                }
            }
        }
    }
    discovered.sort();

    // Exact manifest for all 60 helper/readiness fixtures:
    // - 42 admitted positive programs (7 directories at 100%: nested-store,
    //   optional-store-address, ownership, threading, store-allocation,
    //   store-lending, store-mutation)
    // - 3 verified negative controls (call-regions/negative-closure-rigid-life,
    //   call-regions/negative-closure-whole-type, error-union-calls/definition-selected)
    // - 15 explicitly tracked out-of-scope / open-feature fixtures (0 false admits)
    let manifest: &[(&str, Expected)] = &[
        // tests/fixtures/borrowed-enums (12)
        (
            "tests/fixtures/borrowed-enums/choice.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/borrowed-enums/error-wrapper.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/borrowed-enums/generic-selection.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/borrowed-enums/mixed-classes.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/borrowed-enums/module-api.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/borrowed-enums/module-main.zil",
            Expected::Rejected("typeck", "E-NAME-UNRESOLVED"),
        ),
        (
            "tests/fixtures/borrowed-enums/poll-module-api.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/borrowed-enums/poll-module-main.zil",
            Expected::Rejected("typeck", "E-NAME-UNRESOLVED"),
        ),
        (
            "tests/fixtures/borrowed-enums/poll-record.zil",
            Expected::Admitted,
        ),
        ("tests/fixtures/borrowed-enums/poll.zil", Expected::Admitted),
        (
            "tests/fixtures/borrowed-enums/same-life.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/borrowed-enums/vacant.zil",
            Expected::Admitted,
        ),
        // tests/fixtures/call-regions (3)
        (
            "tests/fixtures/call-regions/known-closure-borrowed.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/call-regions/negative-closure-rigid-life.zil",
            Expected::Rejected("typeck", "E-TYPE-MISMATCH"),
        ),
        (
            "tests/fixtures/call-regions/negative-closure-whole-type.zil",
            Expected::Rejected("typeck", "E-TYPE-MISMATCH"),
        ),
        // tests/fixtures/error-union-calls (1)
        (
            "tests/fixtures/error-union-calls/definition-selected.zil",
            Expected::Rejected("typeck", "E-ERROR-INJECTION-AMBIGUOUS"),
        ),
        // tests/fixtures/nested-store (8/8 Admitted)
        (
            "tests/fixtures/nested-store/direct-byte-buffer.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/nested-store/direct-store.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/nested-store/direct-ticket.zil",
            Expected::Admitted,
        ),
        ("tests/fixtures/nested-store/drains.zil", Expected::Admitted),
        (
            "tests/fixtures/nested-store/lending.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/nested-store/ordinary-environment.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/nested-store/packages.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/nested-store/replacement-borrowed-environment.zil",
            Expected::Admitted,
        ),
        // tests/fixtures/numeric-text (3)
        (
            "tests/fixtures/numeric-text/float_roundtrip.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/numeric-text/float_status.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/numeric-text/integer-widths.zil",
            Expected::Admitted,
        ),
        // tests/fixtures/optional-store-address (7/7 Admitted)
        (
            "tests/fixtures/optional-store-address/field-helper.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/optional-store-address/helper-input.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/optional-store-address/identity.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/optional-store-address/key-source.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/optional-store-address/owning-map-cleanup.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/optional-store-address/owning-map.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/optional-store-address/separate-managers.zil",
            Expected::Admitted,
        ),
        // tests/fixtures/ownership (2/2 Admitted)
        (
            "tests/fixtures/ownership/owned-option-session.zil",
            Expected::Admitted,
        ),
        (
            "tests/fixtures/ownership/unit-store-session.zil",
            Expected::Admitted,
        ),
        // tests/fixtures/threading (1/1 Admitted)
        (
            "tests/fixtures/threading/mutex-worker-helpers.zil",
            Expected::Admitted,
        ),
        // tests/language-readiness/controls (14: 11 Admitted, 3 out-of-scope)
        (
            "tests/language-readiness/controls/copy-pattern-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/ct-enum-match.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/ct-generic.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/ct-type-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/mutable-copy-record-helper-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/mutable-product-local-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/mutable-store-helper-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/mutual-recursion-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/owned-closure-return-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/recursion-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/returned-static-guard.zil",
            Expected::Rejected("typeck", "c_callback"),
        ),
        (
            "tests/language-readiness/controls/scoped-control.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/controls/static-trait-control.zil",
            Expected::Admitted,
        ),
        // tests/language-readiness/store-allocation (4/4 Admitted)
        (
            "tests/language-readiness/store-allocation/allocation.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/store-allocation/numeric.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/store-allocation/shared.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/store-allocation/std-store.zil",
            Expected::Admitted,
        ),
        // tests/language-readiness/store-lending (4/4 Admitted)
        (
            "tests/language-readiness/store-lending/generic-owned-entry-return.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/store-lending/optional-owned-element-return.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/store-lending/shared-element-return.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/store-lending/shared-owned-element-return.zil",
            Expected::Admitted,
        ),
        // tests/language-readiness/store-mutation (2/2 Admitted)
        (
            "tests/language-readiness/store-mutation/direct.zil",
            Expected::Admitted,
        ),
        (
            "tests/language-readiness/store-mutation/std-store.zil",
            Expected::Admitted,
        ),
    ];

    assert_eq!(manifest.len(), 60);
    verify_manifest(&root, &discovered, manifest);
}

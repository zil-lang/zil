#!/usr/bin/env python3
"""Repeat the finite 334 source audit; observations are not language acceptance."""

import argparse
import collections
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


BASELINE = "40fbcca6ef506f3cb8e1d40a4f8995c788627da0"
COMPILER_SOURCE = "f2122be4deb5d6b00abbc3847f8d144a919035fe"
COMPILER_SHA256 = "89891e131177e2fdc8d5e9febb90fbb37521a9fc815869e76b56f74ae575df67"
ROOT = Path(__file__).resolve().parents[3]
NATIVE = [
    "tests/language-readiness/pending/runtime-tuple-binding.zil",
    "tests/language-readiness/pending/ct-type-generic-function.zil",
    "tests/language-readiness/pending/ct-type-generic-record.zil",
    "tests/language-readiness/pending/mutable-product-helper-gap.zil",
    "tests/language-readiness/recovery/pending/life-parameter-return.zil",
    "tests/language-readiness/recovery/pending/trait-type-argument.zil",
    "tests/language-readiness/language-gap-replay/partial-copy-tuple.zil",
    "examples/mainline/concurrency/controls/life_trait.zil",
    "examples/mainline/concurrency/controls/parent_borrow.zil",
]


def sha_bytes(value):
    return hashlib.sha256(value).hexdigest()


def sha(path):
    return sha_bytes(path.read_bytes())


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def run(command, timeout=60):
    return subprocess.run(command, cwd=ROOT, capture_output=True, timeout=timeout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zil", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    binary = args.zil.resolve(strict=True)
    assert sha(binary) == COMPILER_SHA256, "Compiler identity needs a new explicit provenance check"
    changed = git("diff", "--name-only", COMPILER_SOURCE, BASELINE).splitlines()
    production_changes = [p for p in changed if (
        p in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]
        or (p.startswith("crates/") and ("/src/" in p or p.endswith(("/Cargo.toml", "/build.rs"))))
    )]
    assert not production_changes, production_changes
    paths = set(ROOT.glob("tests/language-readiness/recovery/*/*.zil"))
    original25 = {p.relative_to(ROOT).as_posix() for p in paths}
    assert len(original25) == 25
    paths.update(ROOT.glob("tests/language-readiness/pending/*.zil"))
    paths.update(ROOT.glob("tests/language-readiness/language-gap-replay/*.zil"))
    for name in ["copy-pattern-control", "mutable-copy-record-helper-control",
                 "mutable-product-local-control", "mutable-store-helper-control"]:
        paths.add(ROOT / f"tests/language-readiness/controls/{name}.zil")
    for name in [
        "tests/language-readiness/design-probes/returned-captured-handler.zil",
        "examples/mainline/concurrency/controls/life_trait.zil",
        "examples/mainline/concurrency/controls/parent_borrow.zil",
        "examples/mainline/concurrency/pending/parent_closure.zil",
        "examples/mainline/state-bindings/main.zil",
    ]:
        paths.add(ROOT / name)
    observations, executions, failures = [], [], []
    historical = json.loads((ROOT / "validation/results/language-readiness/recovery-capabilities.json").read_text())
    historical = {row["source"]: row for row in historical["observations"]}
    for path in sorted(paths):
        relative = path.relative_to(ROOT).as_posix()
        command = [str(binary), "check", relative, "--json"]
        process = run(command)
        actual = json.loads(process.stdout)
        row = {
            "source": relative,
            "sha256": sha(path),
            "group": "recovery25" if relative in original25 else "additional_current_sources",
            "command": command,
            "exit_code": process.returncode,
            "status": actual["status"],
            "phase": actual["phase"],
            "diagnostics": [{k: d[k] for k in ["phase", "category", "code", "message"]}
                            for d in actual["diagnostics"]],
            "verification_runs": actual["verification_runs"],
            "stdout_sha256": sha_bytes(process.stdout),
            "stderr": process.stderr.decode(),
        }
        if relative in historical:
            old = historical[relative]
            row["historical_316"] = {
                "source_unchanged": old["sha256"] == row["sha256"],
                "status": old["status"], "phase": old["phase"],
            }
        if process.returncode not in [0, 1, 3] or process.stderr:
            failures.append({"source": relative, "reason": "unexpected process result"})
        if relative in NATIVE or "/controls/" in relative:
            if row["status"] != "Success" or row["verification_runs"] != 1:
                failures.append({"source": relative, "reason": "positive control regression"})
        if "/recovery/rejects/" in relative and row["status"] != "SafeReject":
            failures.append({"source": relative, "reason": "protected reject regression"})
        observations.append(row)
    with tempfile.TemporaryDirectory(prefix="zil334-native-") as directory:
        for index, source in enumerate(NATIVE):
            for optimization in ["O0", "O2"]:
                output = Path(directory) / f"case{index}-{optimization}"
                command = [str(binary), "build", source, "--entry", "main", "--opt",
                           optimization, "--backend", "llvm-api", "-o", str(output), "--json"]
                built = run(command)
                report = json.loads(built.stdout)
                codegen = report.get("native_codegen") or {}
                assert built.returncode == 0 and report["status"] == "Success", report
                assert report["verification_runs"] == 1, report
                assert codegen.get("backend") == "llvm-api" and not codegen.get("fallback"), codegen
                object_reports = codegen["object_reports"]
                assert object_reports and all(r["pipeline"] == f"default<{optimization}>" for r in object_reports)
                executed = run([str(output)], timeout=10)
                good = executed.returncode == 0 and executed.stdout == b"0\n" and not executed.stderr
                if not good:
                    failures.append({"source": source, "reason": "native result mismatch"})
                executions.append({
                    "source": source, "source_sha256": sha(ROOT / source),
                    "command": command, "optimization": optimization,
                    "build_exit_code": built.returncode, "build_status": report["status"],
                    "verification_runs": report["verification_runs"],
                    "actual_backend": codegen["backend"], "fallback": codegen.get("fallback"),
                    "llvm_version": object_reports[0]["llvm_version"],
                    "target": object_reports[0]["triple"],
                    "executable_sha256": sha(output),
                    "llvm_sha256": sha(Path(report["artifacts"]["llvm"])),
                    "expected": {"exit_code": 0, "stdout": "0\n", "stderr": ""},
                    "actual": {"exit_code": executed.returncode, "stdout": executed.stdout.decode(),
                               "stderr": executed.stderr.decode()},
                    "result": "Pass" if good else "Fail",
                })
    normative = json.loads((ROOT / "spec/interfaces.json").read_text())["normative_inputs"]
    result = {
        "kind": "334 finite source audit; not complete language/native/platform/product acceptance",
        "collection_status": "Complete" if not failures else "FailedControls",
        "source_baseline": BASELINE, "collector_checkout": git("rev-parse", "HEAD"),
        "compiler": {"path": str(binary), "sha256": sha(binary), "observed_source_commit": COMPILER_SOURCE,
                     "receipt": "docs/validation/mainline-consumer208-20261103.json",
                     "production_diff_to_baseline": production_changes,
                     "source_reuse_basis": "Observed release CLI hash matches. Source/Cargo/native shim inputs are unchanged; changed crate files are tests only."},
        "normative_sha256": {p: sha(ROOT / p) for p in normative},
        "selected": len(observations),
        "observed_counts": dict(collections.Counter(r["status"] for r in observations)),
        "recovery25_counts": dict(collections.Counter(r["status"] for r in observations if r["source"] in original25)),
        "failures": failures, "observations": observations,
        "native": {"distinct_sources": len(NATIVE), "builds": len(executions),
                   "executions": executions},
        "limits": ["Pending failures remain future legal targets, not permanent negative tests.",
                   "Source-to-Verify and native runs are counted separately; repeated optimization runs are not new source coverage.",
                   "No full suite, Windows execution, Rust cost comparison, weak-memory proof or general large-byvalue ABI run.",
                   "329-333 remain under construction and are not incorporated into this baseline."],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: result[k] for k in ["collection_status", "selected", "observed_counts", "recovery25_counts", "failures"]}))
    print(f"native: {len(NATIVE)} sources, {len(executions)} builds/runs")
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())

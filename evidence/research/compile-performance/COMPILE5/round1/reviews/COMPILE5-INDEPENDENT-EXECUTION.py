#!/usr/bin/env python3
"""Read-only Compile5 execution and archive audit; never runs Cargo or a runner.

Run only after the artifact has finished extracting. The sole writes are new
review JSON/Markdown files outside the artifact directory. No performance
estimator, benchmark module, compiler or native output is executed/imported.
"""
from __future__ import annotations

import argparse
import ast
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tarfile
import tomllib
import zipfile

sys.dont_write_bytecode = True
RUN_ID = 41086451087
CANDIDATE = "951471d5a6e26e15de96e0bfaf91cd3e537a3dd9"
LOCAL_FREEZE = "187eae4f6bb085f0d0d42765f047b7f60add6bed"
CANDIDATE_TREE = "6ab7522524122c6d21738329ab7dd885c6bbc75c"
CANDIDATE_CRATES = "b583bc62e4aa39b206afb27f76f2f8262120fe9d"
BASELINE = "a715f828261ff4b6ef9c8125623064360745aae4"
BASELINE_CRATES = "0221fa705e8dc926c54f6b85f2dc9ef3af98559a"
MANIFEST = "tools/research/compile-performance/compile4-run.json"
ORCHESTRATOR = "tools/research/compile-performance/compile4_ci.py"
PREFIX = "tools/research/compile-performance/"
CONTROL_FILES = ("test_bench.py", "test_rust_bench.py", "test_runtime_bench.py")
PLAN_RUNNERS = {
    "o0-bridge": "bench.py",
    "rust-o0-cold": "rust_bench.py",
    "rust-o0-incremental": "rust_bench.py",
    "rust-o2-closed": "rust_bench.py",
    "o2-direct-quality": "runtime_bench.py",
}
WINDOWS = (
    ("o0-bridge", "build", "bridge-measure", "o0-bridge-source-object-no-query", "summary.json"),
    ("rust-o0-cold", "build", "rust-o0-cold-measure", "rust-o0-cold-build", "build-summary.json"),
    ("rust-o0-cold", "runtime", "rust-o0-cold-measure-runtime", "rust-o0-cold-runtime", "runtime-summary.json"),
    ("rust-o0-incremental", "build", "rust-o0-incremental-measure", "rust-o0-incremental-build", "build-summary.json"),
    ("rust-o0-incremental", "runtime", "rust-o0-incremental-measure-runtime", "rust-o0-incremental-runtime", "runtime-summary.json"),
    ("rust-o2-closed", "build", "rust-o2-closed-measure", "rust-o2-closed-build", "build-summary.json"),
    ("rust-o2-closed", "runtime", "rust-o2-closed-measure-runtime", "rust-o2-closed-runtime", "runtime-summary.json"),
    ("o2-direct-quality", "runtime", "quality-measure", "baseline-closed-o2-1x1-vs-candidate-closed-o2-8x4", "summary.json"),
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def no_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate JSON key: " + key)
        result[key] = value
    return result


def parse_json(raw):
    return json.loads(raw, object_pairs_hook=no_duplicate_keys)


def read_json(path):
    return parse_json(Path(path).read_bytes())


def digest_bytes(raw):
    return hashlib.sha256(raw).hexdigest()


def stream_digest(stream):
    checksum, size = hashlib.sha256(), 0
    while chunk := stream.read(1024 * 1024):
        checksum.update(chunk)
        size += len(chunk)
    return checksum.hexdigest(), size


def digest(path):
    """Open every requested path independently, including hardlinked paths."""
    path = Path(path)
    require(not path.is_symlink(), "Symlink is not original regular evidence: " + str(path))
    with path.open("rb") as stream:
        before = os.fstat(stream.fileno())
        require(stat.S_ISREG(before.st_mode), "Not a regular file: " + str(path))
        checksum, size = stream_digest(stream)
        after = os.fstat(stream.fileno())
    fields = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns")
    require(all(getattr(before, field) == getattr(after, field) for field in fields),
            "File changed while reading: " + str(path))
    require(size == before.st_size, "Short file read: " + str(path))
    return checksum, size


def relative_path(name):
    require(isinstance(name, str) and bool(name), "Empty/non-string evidence path")
    path = PurePosixPath(name)
    require(not path.is_absolute() and ".." not in path.parts and path.parts,
            "Unsafe relative evidence path: " + name)
    return path


def safe_local(root, name):
    relative = relative_path(name)
    path = root.joinpath(*relative.parts)
    for item in (path, *path.parents):
        if item == root:
            break
        require(not item.is_symlink(), "Symlink in evidence path: " + str(item))
    require(path != root and root in path.resolve().parents, "Path escapes evidence: " + name)
    return path


def strip_ansi(text):
    return re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)


def rust_harnesses(stdout, stderr):
    """Match Cargo targets to libtest blocks and recount individual results."""
    stdout, stderr = strip_ansi(stdout), strip_ansi(stderr)
    targets = re.findall(r"^\s*Running (unittests [^\n]+?|tests/\S+\.rs) \(([^\n]+)\)\s*$", stderr, re.M)
    starts = list(re.finditer(r"^running (\d+) tests?\s*$", stdout, re.M))
    totals = re.findall(r"^test result: .*", stdout, re.M)
    require(len(targets) == len(starts) == len(totals) and bool(targets),
            "Cargo Running targets, libtest blocks and summaries do not match")
    result = []
    for index, ((target, executable), start) in enumerate(zip(targets, starts)):
        end = starts[index + 1].start() if index + 1 < len(starts) else len(stdout)
        block = stdout[start.end():end]
        summary = re.findall(
            r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
            r"(\d+) measured; (\d+) filtered out; finished in [^\n]+$", block, re.M)
        require(len(summary) == 1, "Missing/full libtest result mismatch: " + target)
        status, *numbers = summary[0]
        passed, failed, ignored, measured, filtered = map(int, numbers)
        cases = re.findall(r"^test (.+?) \.\.\. (ok|FAILED|ignored(?:, [^\n]*)?|bench:[^\n]*)$", block, re.M)
        outcomes = Counter("ignored" if value.startswith("ignored") else
                           "measured" if value.startswith("bench:") else value for _, value in cases)
        names = [name for name, _ in cases]
        require(len(names) == len(set(names)), "Duplicate individual test result: " + target)
        require(int(start.group(1)) == passed + failed + ignored + measured == len(cases),
                "Running count differs from individual results: " + target)
        require((outcomes["ok"], outcomes["FAILED"], outcomes["ignored"], outcomes["measured"])
                == (passed, failed, ignored, measured), "Individual/full result counts differ: " + target)
        require(status == "ok" and passed > 0 and failed == ignored == measured == filtered == 0,
                "A required harness failed, was empty, ignored, measured or filtered: " + target)
        result.append({"target": target, "executable": executable, "running": int(start.group(1)),
                       "passed": passed, "failed": failed, "ignored": ignored, "measured": measured,
                       "filtered_out": filtered, "individual_ok_lines": outcomes["ok"], "test_names": names})
    return result


def python_suite(stdout, stderr):
    text = strip_ansi(stdout + "\n" + stderr)
    count = re.findall(r"^Ran (\d+) tests? in [^\n]+$", text, re.M)
    final = re.findall(r"^(OK(?: \([^\n]*\))?|FAILED[^\n]*)$", text, re.M)
    require(len(count) == len(final) == 1 and final[0] == "OK", "Python suite failed/skipped or has ambiguous totals")
    require(not re.search(r"\b(?:skipped|expected failures|unexpected successes)=", text),
            "Python suite has skipped/expected-failure cases")
    return {"passed": int(count[0]), "skipped": 0, "status": "OK"}


class Audit:
    def __init__(self, args):
        self.args = args
        self.root = args.evidence.resolve(strict=True)
        self.artifact = (args.artifact_root or self.root.parent).resolve(strict=True)
        self.repo = args.repo.resolve(strict=True)
        require(self.artifact in self.root.parents, "Evidence must be inside extracted artifact directory")
        self.result = read_json(self.root / "result.json")
        self.original = PurePosixPath(self.result["original_evidence_root"])
        require(self.original.is_absolute(), "Missing original runner evidence path")
        self.manifest = read_json(self.root / "manifest.json")
        self.index = read_json(self.root / "files.json")
        self.commands, self.by_label, self.git_cache = [], {}, {}
        self.report = {
            "schema_version": 1, "reviewer": "/root/shape_opt",
            "status": "InProgress_ExecutionAudit_NoVerdict",
            "scope": "Independent retained-artifact execution/integrity audit; no Cargo, native execution or performance arithmetic.",
            "expected_run": RUN_ID, "candidate": CANDIDATE, "local_freeze": LOCAL_FREEZE,
            "baseline": BASELINE, "phase_failures": [], "product_gates": "NotRunByThisReview",
            "performance_adoption": "NotEvaluatedByThisReview",
            "limitations": [
                "Post-run evidence review; does not rerun compilers/tests or establish full-workspace/product-gate success.",
                "Command interval ends are derived from recorded started_unix plus monotonic seconds; per-sample absolute timestamps were not recorded.",
                "Serial measurement also relies on the frozen synchronous runner control flow; no claim of physical-host exclusivity or absence of unrelated processes.",
                "External LLVM/Rust tool binaries and the original LLVM download are not retained here; their identities are cross-checked between CI receipts/plans, not independently rehashed as installed binaries.",
                "Source defaults plus exact argv establish feature selection; this is not a new Cargo feature-resolution execution.",
            ],
        }

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args])

    def source(self, name, revision=CANDIDATE):
        key = revision, name
        if key not in self.git_cache:
            self.git_cache[key] = self.git("show", revision + ":" + name)
        return self.git_cache[key]

    def local(self, name):
        path = PurePosixPath(name)
        if path.is_absolute():
            path = path.relative_to(self.original)
        return safe_local(self.root, path.as_posix())

    def log(self, label, stream):
        return self.local(self.by_label[label][stream])

    def check_file(self, path, expected_hash, expected_size=None):
        checksum, size = digest(path)
        require(checksum == expected_hash, "SHA256 mismatch: " + str(path))
        require(expected_size is None or size == expected_size, "Byte count mismatch: " + str(path))
        return {"sha256": checksum, "bytes": size}

    def source_identity(self):
        require(self.result["status"] == "Complete_Exploratory_NotAProductGate", "CI result is incomplete/failed")
        identities = {"candidate_revision": CANDIDATE, "candidate_crates_tree": CANDIDATE_CRATES,
                      "base_revision": BASELINE, "base_crates_tree": BASELINE_CRATES}
        for key, value in identities.items():
            require(self.result[key] == value, "Result source identity differs: " + key)
        for spec, expected in ((CANDIDATE + "^{tree}", CANDIDATE_TREE),
                               (LOCAL_FREEZE + "^{tree}", CANDIDATE_TREE),
                               (CANDIDATE + ":crates", CANDIDATE_CRATES),
                               (BASELINE + ":crates", BASELINE_CRATES)):
            require(self.git("rev-parse", spec).decode().strip() == expected, "Git tree differs: " + spec)
        self.git("merge-base", "--is-ancestor", BASELINE, CANDIDATE)
        source_manifest = self.source(MANIFEST)
        require((self.root / "manifest.json").read_bytes() == source_manifest,
                "Artifact manifest bytes differ from measured commit")
        require(self.manifest["run_id"] == "compile5-round1" and self.manifest["base_revision"] == BASELINE
                and self.manifest["candidate_revision"] == "HEAD"
                and self.manifest["candidate_crates_tree"] == CANDIDATE_CRATES
                and self.manifest["llvm_version"] == "23.1.4" and self.manifest["rust_version"] == "1.99.1"
                and self.manifest["blocks"] == 3 and self.manifest["seed"] == 20261030,
                "Frozen experiment identity differs")
        for key, source in (("manifest_sha256", source_manifest), ("runner_sha256", self.source(ORCHESTRATOR))):
            require(self.result[key] == digest_bytes(source), "Frozen source SHA differs: " + key)
        setup = self.artifact / "compile4-setup"
        require((setup / "commit.txt").read_text().strip() == CANDIDATE, "Setup commit differs")
        validation = read_json(setup / "manifest-validation.log")
        require(validation["status"] == "ManifestValidated_NotMeasured", "Setup validation did not pass")
        for key in (*identities, "manifest_sha256", "runner_sha256"):
            require(validation[key] == self.result[key], "Setup/result identity differs: " + key)
        self.report["source_identity"] = {"status": "Pass", **identities, "candidate_tree": CANDIDATE_TREE,
            "local_freeze_tree_equal": True, "manifest_bytes_equal": True,
            "manifest_sha256": digest_bytes(source_manifest), "runner_sha256": self.result["runner_sha256"],
            "workflow_sha256": digest_bytes(self.source(".github/workflows/compile-performance.yml")),
            "setup_commit_and_validation_equal": True}

    def inventory(self):
        require(isinstance(self.index, dict) and bool(self.index), "Empty/non-object files.json")
        actual, inodes, total, modes = set(), set(), 0, Counter()
        for folder, dirs, files in os.walk(self.root, followlinks=False):
            for name in dirs:
                require(not (Path(folder) / name).is_symlink(), "Directory symlink in artifact")
            for name in files:
                path = Path(folder) / name
                require(stat.S_ISREG(path.lstat().st_mode), "Nonregular artifact member: " + str(path))
                actual.add(path.relative_to(self.root).as_posix())
        require(actual == set(self.index) | {"files.json"},
                "Inventory path set differs; missing=" + repr(sorted(set(self.index) - actual)[:20])
                + "; unlisted=" + repr(sorted(actual - set(self.index) - {"files.json"})[:20]))
        for name, spec in self.index.items():
            require(set(spec) == {"bytes", "sha256"}, "Unexpected files.json row: " + name)
            path = self.local(name)
            require(not PurePosixPath(name).is_absolute(), "Absolute inventory path: " + name)
            self.check_file(path, spec["sha256"], spec["bytes"])
            info = path.stat()
            inodes.add((info.st_dev, info.st_ino))
            modes[oct(stat.S_IMODE(info.st_mode))] += 1
            total += info.st_size
        self.report["inventory"] = {"status": "Pass", "listed_files": len(self.index),
            "verified_files": len(self.index), "logical_bytes_hashed": total, "distinct_inodes": len(inodes),
            "observed_file_modes": dict(modes), "inventory_sha256": digest(self.root / "files.json")[0],
            "missing": [], "unlisted_except_inventory_itself": [],
            "method": "Each path independently opened and streamed; no inode/hash cache and no evidence writes."}

    def archive_modes(self):
        archive = self.args.tar or self.artifact / "compile4-evidence.tar.gz"
        archive_hash, archive_size = digest(archive)
        sidecar = self.artifact / "compile4-evidence.sha256"
        require(sidecar.read_text().split()[0] == archive_hash, "Original tar.gz sidecar differs")
        expected = dict(self.index)
        inventory_hash, inventory_size = digest(self.root / "files.json")
        expected["files.json"] = {"sha256": inventory_hash, "bytes": inventory_size}
        seen, all_members, directories, total, mode_counts = set(), set(), 0, 0, Counter()
        with tarfile.open(archive, mode="r|*") as source:
            for member in source:
                name = relative_path(member.name)
                require(name.as_posix() not in all_members, "Duplicate tar member: " + member.name)
                all_members.add(name.as_posix())
                require(name.parts[0] == self.root.name, "Unexpected tar root: " + member.name)
                if member.isdir():
                    directories += 1
                    continue
                require(member.isfile() and len(name.parts) > 1, "Nonregular tar member: " + member.name)
                relative = PurePosixPath(*name.parts[1:]).as_posix()
                require(relative in expected, "Unlisted tar member: " + relative)
                seen.add(relative)
                with source.extractfile(member) as stream:
                    checksum, size = stream_digest(stream)
                spec = expected[relative]
                require(size == member.size == spec["bytes"] and checksum == spec["sha256"],
                        "Tar member bytes/hash differ: " + relative)
                path = self.local(relative)
                require(stat.S_ISREG(path.lstat().st_mode), "Tar destination is not regular: " + relative)
                require(stat.S_IMODE(path.stat().st_mode) == member.mode & 0o7777,
                        "Tar/extracted POSIX mode differs: " + relative)
                mode_counts[oct(member.mode & 0o7777)] += 1
                total += size
        require(seen == set(expected), "Tar omitted original files: " + repr(sorted(set(expected) - seen)[:20]))
        self.report["tar_members"] = {"status": "Pass", "path": str(archive), "archive_sha256": archive_hash,
            "archive_bytes": archive_size, "regular_files": len(seen), "directories": directories,
            "logical_bytes_hashed": total, "posix_file_modes": dict(mode_counts),
            "missing": [], "other_members": [],
            "method": "Streamed original tar without extraction; all original regular members, including files.json, compared by size/SHA256/POSIX mode."}

    def commands_and_builds(self):
        for path in sorted((self.root / "logs").glob("*.json")):
            row = read_json(path)
            require(row["label"] not in self.by_label, "Duplicate command label")
            require(row["status"] == "Pass" and row["exit"] == 0 and row["timeout"] is False,
                    "Failed/unfinished CI command: " + row["label"])
            for stream in ("stdout", "stderr"):
                self.check_file(self.local(row[stream]), row[stream + "_sha256"])
            require(all(type(row[key]) in (int, float) and math.isfinite(row[key]) for key in ("started_unix", "seconds"))
                    and row["seconds"] >= 0, "Invalid command interval: " + row["label"])
            row["review_receipt"] = path.relative_to(self.root).as_posix()
            self.commands.append(row)
            self.by_label[row["label"]] = row
        expected = ["toolchain-check", "rustc-path", "cargo-path", "rustc-version", "source-tree", "formatting",
                    "test_bench", "test_rust_bench", "test_runtime_bench", "baseline-worktree", "baseline-build", "candidate-check"]
        expected += ["candidate-tests-" + str(index) for index in range(len(self.manifest["tests"]))]
        expected += ["candidate-build", "bridge-prepare", "bridge-preflight", "bridge-measure", "bridge-summarize"]
        for name in ("rust-o0-cold", "rust-o0-incremental", "rust-o2-closed"):
            expected += [name + suffix for suffix in ("-prepare", "-preflight", "-measure", "-build-summarize", "-measure-runtime", "-runtime-summarize")]
        expected += ["quality-prepare", "quality-build", "quality-measure", "quality-summarize"]
        require([row["label"] for row in self.commands] == expected, "Missing/unexpected/reordered orchestrator commands")
        for index, row in enumerate(self.commands, 1):
            require(Path(row["review_receipt"]).name == f"{index:03d}-{row['label']}.json", "Command index gap/reordering")
        cargo = self.log("cargo-path", "stdout").read_text().strip()
        require(PurePosixPath(cargo).name == "cargo" and "/1.99.1-" in cargo, "Wrong actual Cargo path")
        require(self.by_label["formatting"]["argv"] == [cargo, "fmt", "--all", "--", "--check"], "Wrong formatting argv")
        require(self.by_label["candidate-check"]["argv"] == [cargo, "check", "--locked", "--workspace"], "Wrong workspace check argv")
        source_listing = self.log("source-tree", "stdout").read_bytes()
        require(source_listing == self.git("ls-tree", "-r", CANDIDATE), "Full candidate Git tree listing differs")
        require(self.by_label["source-tree"]["argv"] == ["git", "ls-tree", "-r", "HEAD"], "Wrong tree-list command")
        self.report["source_identity"].update(full_git_ls_tree_matches=True, source_listing_sha256=digest_bytes(source_listing))
        builds = self.builds = read_json(self.root / "compiler-builds.json")
        require(set(builds) == {"baseline", "candidate"}, "Missing/extra compiler build")
        build_rows = {}
        for label, revision in (("baseline", BASELINE), ("candidate", CANDIDATE)):
            row, binary = self.by_label[label + "-build"], builds[label]
            require(row["argv"] == [cargo, "build", "--release", "--locked", "-p", "zil-cli"], "Wrong compiler build argv: " + label)
            require(binary["source_commit"] == revision, "Compiler source revision differs: " + label)
            self.check_file(self.local(binary["path"]), binary["sha256"], binary["bytes"])
            mode = stat.S_IMODE(self.local(binary["path"]).stat().st_mode)
            require(mode & 0o111, "Retained compiler is not executable: " + label)
            features = {}
            for package in ("zil-cli", "zil-driver", "zil-backend-llvm"):
                cargo_toml = tomllib.loads(self.source("crates/" + package + "/Cargo.toml", revision).decode())
                features[package] = cargo_toml.get("features", {})
            require(features["zil-cli"].get("default") == ["llvm-api"]
                    and features["zil-cli"].get("llvm-api") == ["zil-driver/llvm-api"]
                    and features["zil-driver"].get("llvm-api") == ["zil-backend-llvm/llvm-api"],
                    "CLI default LLVM API feature chain differs: " + label)
            require("timings" not in features["zil-cli"].get("default", []), "Timings entered compiler default")
            require(set(row["extra_environment"]) == {"CARGO_TARGET_DIR"}, "Unexpected compiler-build environment overrides")
            require(PurePosixPath(row["extra_environment"]["CARGO_TARGET_DIR"]).name == "target-" + label,
                    "Compiler target directories are not isolated")
            require(re.search(r"Finished `release` profile", self.log(label + "-build", "stderr").read_text()),
                    "Compiler build log has no finished release profile")
            build_rows[label] = {**binary, "posix_mode": oct(mode), "argv": row["argv"], "cwd": row["cwd"],
                                 "extra_environment": row["extra_environment"], "features_from_frozen_source": features,
                                 "receipt": row["review_receipt"], "stdout": row["stdout"], "stderr": row["stderr"]}
        base_worktree = self.by_label["baseline-worktree"]["argv"]
        require(base_worktree == ["git", "worktree", "add", "--detach", self.by_label["baseline-build"]["cwd"], BASELINE],
                "Baseline checkout command/cwd/source mismatch")
        candidate_cwd = self.by_label["source-tree"]["cwd"]
        require(self.by_label["candidate-build"]["cwd"] == candidate_cwd, "Candidate build cwd differs from tree checkout")
        self.report["compiler_builds"] = {"status": "Pass", "builds": build_rows, "actual_binary_bytes_hash_and_modes_verified": True}
        self.report["commands"] = {"status": "Pass", "count": len(self.commands), "all_exit_zero": True,
            "all_timeout_false": True, "all_stdout_stderr_hashes_match": True,
            "formatting_argv": self.by_label["formatting"]["argv"], "candidate_check_argv": self.by_label["candidate-check"]["argv"]}

    def tests(self):
        harnesses, executed = [], set()
        for index, group in enumerate(self.manifest["tests"]):
            label = "candidate-tests-" + str(index)
            command = self.by_label[label]
            cargo = self.log("cargo-path", "stdout").read_text().strip()
            expected = [cargo, "test", "--release", "--locked", "-p", group["package"]]
            if group["lib"]:
                expected.append("--lib")
            for name in group["targets"]:
                expected += ["--test", name]
            if group["features"]:
                expected += ["--features", ",".join(group["features"])]
            require(command["argv"] == expected, "Test argv/features/filters differ: " + label)
            require(command["cwd"] == self.by_label["source-tree"]["cwd"], "Test cwd differs from frozen candidate")
            require(command["extra_environment"] == self.by_label["candidate-build"]["extra_environment"], "Test target environment differs")
            rows = rust_harnesses(self.log(label, "stdout").read_text(), self.log(label, "stderr").read_text())
            expected_targets = {"tests/" + name + ".rs" for name in group["targets"]}
            if group["lib"]:
                expected_targets.add("unittests src/lib.rs")
            require(len(rows) == len(expected_targets) and {row["target"] for row in rows} == expected_targets,
                    "Actual Cargo harness set differs from manifest: " + label)
            for row in rows:
                stem = group["package"].replace("-", "_") if row["target"].startswith("unittests") else Path(row["target"]).stem
                expected_dir = PurePosixPath(command["extra_environment"]["CARGO_TARGET_DIR"]) / "release/deps"
                require(PurePosixPath(row["executable"]).parent == expected_dir
                        and PurePosixPath(row["executable"]).name.startswith(stem + "-"), "Unexpected test executable")
                row.update(package=group["package"], features=group["features"], receipt=command["review_receipt"],
                           stdout=command["stdout"], stderr=command["stderr"])
                executed.update((group["package"], name) for name in row["test_names"])
                harnesses.append(row)
        required = {}
        sources = {
            "cp18": ("zil-types", "crates/zil-types/tests/shape_resolution_cp18.rs"),
            "cp15": ("zil-driver", "crates/zil-driver/tests/borrowed_inference_cp15.rs"),
            "cp19_source": ("zil-driver", "crates/zil-driver/tests/ct_reuse_cp19.rs"),
            "cp19_type_replay": ("zil-ct", "crates/zil-ct/src/source/reuse_cp19.rs"),
        }
        for label, (package, source) in sources.items():
            names = re.findall(r"#\[test\]\s*fn\s+(\w+)\s*\(", self.source(source).decode())
            require(bool(names), "No direct controls found in frozen source: " + source)
            for name in names:
                matches = [actual for pkg, actual in executed if pkg == package and (actual == name or actual.endswith("::" + name))]
                require(len(matches) == 1, "Required named regression did not execute exactly once: " + name)
            required[label] = {"source": source, "sha256": digest_bytes(self.source(source)), "tests": names}
        named_controls = {
            "cp14_boundaries_and_fallbacks": ("zil-backend-llvm", "crates/zil-backend-llvm/src/machine_cgu.rs", [
                "cp14_small_closed_o2_obeys_total_instruction_boundary",
                "cp14_disabled_and_single_unit_paths_are_unchanged",
                "cp14_large_closed_program_preserves_cross_unit_reference"]),
            "cp14_invalid_options": ("zil-backend-llvm", "crates/zil-backend-llvm/tests/closed_executable_cp13.rs", [
                "cp14_small_closed_o2_cannot_bypass_invalid_codegen_options"]),
            "cp19_materialization": ("zil-ct", "crates/zil-ct/src/materialize.rs", [
                "lazy_identity_cursor_preserves_exact_node_depth_and_diagnostic_order"]),
            "checkpoint_budget": ("zil-driver", "crates/zil-driver/tests/query_cache_cp12.rs", [
                "source_selection_bypasses_checkpoint_and_replays_current_ct_budget",
                "ct_exact_limit_and_n_minus_one_match_uncached_requests"]),
            "selection_budget": ("zil-driver", "crates/zil-driver/tests/impl219_budget.rs", [
                "selection_and_later_constants_share_steps_nodes_and_fresh_request_recovery",
                "project_source_dependency_edits_reselect_with_fresh_independent_verification",
                "target_and_limits_invalidate_selection_but_original_no_selection_paths_keep_their_identity"]),
        }
        for label, (package, source, names) in named_controls.items():
            raw = self.source(source)
            for name in names:
                require(re.search(r"\bfn\s+" + re.escape(name) + r"\s*\(", raw.decode()), "Named regression missing from source: " + name)
                matches = [actual for pkg, actual in executed if pkg == package and (actual == name or actual.endswith("::" + name))]
                require(len(matches) == 1, "Named regression did not execute exactly once: " + name)
            required[label] = {"source": source, "sha256": digest_bytes(raw), "tests": names}
        sums = {name: sum(row[name] for row in harnesses) for name in ("passed", "failed", "ignored", "measured", "filtered_out")}
        self.report["cargo_targeted_tests"] = {"status": "Pass", "harness_count": len(harnesses), **sums,
            "harnesses": harnesses, "required_regressions": required,
            "method": "Exact argv and source binding; stderr Running targets matched to stdout running/result blocks and every individual test outcome."}
        controls = []
        for filename in CONTROL_FILES:
            label = filename[:-3]
            command = self.by_label[label]
            require(len(command["argv"]) == 2 and command["argv"][1] == command["cwd"] + "/" + PREFIX + filename,
                    "Python control runner argv differs: " + filename)
            source = self.source(PREFIX + filename)
            syntax = ast.parse(source)
            expected_names = [method.name for klass in syntax.body if isinstance(klass, ast.ClassDef)
                              for method in klass.body if isinstance(method, ast.FunctionDef) and method.name.startswith("test_")]
            counts = python_suite(self.log(label, "stdout").read_text(), self.log(label, "stderr").read_text())
            require(counts["passed"] == len(expected_names) > 0, "Python suite count differs from frozen test source")
            controls.append({"runner": filename, **counts, "declared_test_methods": expected_names,
                             "source_sha256": digest_bytes(source), "receipt": command["review_receipt"],
                             "stdout": command["stdout"], "stderr": command["stderr"]})
        self.report["python_controls"] = {"status": "Pass", "suites": controls,
            "passed": sum(row["passed"] for row in controls), "skipped": 0,
            "individual_names_in_log": False, "method": "unittest full totals and zero skip/failure matched to frozen AST method counts; no claim that dots name individual cases."}

    def toolchain(self):
        checked = read_json(self.log("toolchain-check", "stdout"))
        require(checked["status"] == "Pass" and checked["required_llvm"] == "23.1.4"
                and checked["required_rust"] == "1.99.1", "Toolchain check differs")
        install_root = self.artifact / "compile4-llvm-23.1.4"
        install = read_json(install_root / "install-receipt.json")
        require(install["status"] == "Pass" and install["release"]["version"] == "23.1.4", "LLVM installation failed/drifted")
        require(install["installer_sha256"] == digest_bytes(self.source("tools/llvm/install.py")), "Installer differs from frozen source")
        for command in install["commands"]:
            require(command["exit_code"] == 0, "Installation command failed: " + command["name"])
            for stream in ("stdout", "stderr"):
                self.check_file(safe_local(install_root, command[stream]["path"]), command[stream]["sha256"])
        identities = checked["checks"]
        require(identities["llvm"]["sha256"] == install["library"]["sha256"], "LLVM DSO receipt mismatch")
        for label, key in (("clang", "clang"), ("lld_link", "lld-link")):
            require(identities[label]["sha256"] == install["tools"][key]["sha256"], "LLVM tool receipt mismatch: " + label)
        require(all(identities[label]["version"] == "23.1.4" for label in ("llvm", "clang", "lld_link")), "Wrong LLVM family version")
        require(all(identities[label]["version"] == "1.99.1" for label in ("rustc", "cargo")), "Wrong Rust family version")
        rust_path = self.log("rustc-path", "stdout").read_text().strip()
        require(self.by_label["rustc-version"]["argv"] == [rust_path, "-Vv"], "Rust version command differs from resolved executable")
        version = self.log("rustc-version", "stdout").read_text()
        require(version.startswith("rustc 1.99.1 ") and "\nLLVM version: 23.1.1\n" in version, "Wrong actual rustc/embedded LLVM version")
        self.tool_identities = identities
        self.report["toolchain"] = {"status": "Pass", "checked_identities": identities, "rustc_version_output": version,
            "resolved_rustc_path": rust_path, "installation_commands": len(install["commands"]),
            "all_installation_log_hashes_match": True, "install_receipt_sha256": digest(install_root / "install-receipt.json")[0],
            "recorded_original_download": install["archive"], "installer_matches_source": True,
            "scope": "Receipt/log/plan identity consistency; installed external tool bytes were not retained for independent rehash."}

    def plans_and_order(self):
        ends = [row["started_unix"] + row["seconds"] for row in self.commands]
        gaps = [later["started_unix"] - end for end, later in zip(ends, self.commands[1:])]
        require(all(gap >= 0 for gap in gaps), "Recorded command intervals overlap")
        first_prepare = self.by_label["bridge-prepare"]["started_unix"]
        preparation = [row for row in self.commands if row["label"] in {"baseline-build", "candidate-build", "candidate-check", "formatting", "test_bench", "test_rust_bench", "test_runtime_bench"}
                       or row["label"].startswith("candidate-tests-")]
        last_gate = max(row["started_unix"] + row["seconds"] for row in preparation)
        require(last_gate <= first_prepare, "Builds/tests overlap first plan preparation")
        require(self.result["started_unix"] <= self.commands[0]["started_unix"]
                and ends[-1] <= self.result["finished_unix"], "Command intervals escape overall result interval")
        plans, rust_compilers = {}, set()
        for folder, runner in PLAN_RUNNERS.items():
            path = self.root / "windows" / folder / "plan/plan.json"
            plan = read_json(path)
            require(plan["runner_source_commit"] == CANDIDATE and plan["runner_sha256"] == digest_bytes(self.source(PREFIX + runner)),
                    "Frozen plan runner differs: " + folder)
            for helper, checksum in plan.get("helper_sha256", plan.get("dependencies_sha256", {})).items():
                require(digest_bytes(self.source(helper)) == checksum, "Frozen helper differs: " + helper)
            require(plan["created_unix"] >= last_gate, "Plan created before test/build completion: " + folder)
            arms = ("B",) if folder.startswith("rust-") else ("A", "B")
            for arm in arms:
                label = "baseline" if arm == "A" else "candidate"
                build = self.builds[label]
                require(plan["binaries"][arm]["path"] == build["path"] and plan["binaries"][arm]["sha256"] == build["sha256"]
                        and plan["binaries"][arm]["size"] == build["bytes"], "Plan/build binary identity differs: " + folder)
                identity = plan["source_identity"][arm]
                require(identity["commit"] == (BASELINE if arm == "A" else CANDIDATE)
                        and identity["crates_tree"] == (BASELINE_CRATES if arm == "A" else CANDIDATE_CRATES),
                        "Plan source identity differs: " + folder)
            require(plan["binaries"]["clang"]["sha256"] == self.tool_identities["clang"]["sha256"], "Plan clang differs")
            extra_hashes = {entry["sha256"] for entry in plan["extra_tools"]}
            require({self.tool_identities[key]["sha256"] for key in ("llvm", "lld_link")} <= extra_hashes,
                    "Plan LLVM/LLD differs from checked toolchain")
            if folder.startswith("rust-"):
                require(plan["binaries"]["A"]["path"] == self.log("rustc-path", "stdout").read_text().strip(), "Plan rustc path differs")
                rust = plan["binaries"]["A"]
                rust_compilers.add((rust["path"], rust["sha256"], rust["size"]))
            plans[folder] = {"path": path.relative_to(self.root).as_posix(), "sha256": digest(path)[0],
                             "runner_and_helpers_match": True, "source_and_binary_match": True,
                             "toolchain_receipts_match": True, "created_after_gates": True}
        require(len(rust_compilers) == 1, "Frozen rustc binary identity differs between Rust windows")
        rust_path, rust_sha, rust_bytes = next(iter(rust_compilers))
        self.report["toolchain"]["cross_plan_rustc_identity"] = {"path": rust_path, "sha256": rust_sha, "bytes": rust_bytes,
            "independently_rehashed_installed_binary": False}
        require([entry["name"] for entry in self.result["windows"]] == [entry[3] for entry in WINDOWS], "Window receipt sequence differs")
        windows = []
        for (folder, kind, label, name, summary_name), recorded in zip(WINDOWS, self.result["windows"]):
            command = self.by_label[label]
            require(command["started_unix"] >= last_gate and "--quiet-window" in command["argv"], "Measurement before gates or quiet flag absent")
            location = self.root / "windows" / folder
            rows_path, complete_path = location / kind / "records.jsonl", location / kind / "complete.json"
            rows = [parse_json(line) for line in rows_path.read_bytes().splitlines()]
            complete = read_json(complete_path)
            # runtime_bench's completion schema has no `pass` field; its empty
            # failures plus bound rows express completion. Other runners do.
            if folder != "o2-direct-quality":
                require(complete["pass"] is True, "Measurement completion did not pass: " + name)
            require(not complete["failures"] and complete["rows"] == len(rows) > 0,
                    "Measurement completion/row count differs: " + name)
            require(complete["records_sha256"] == digest(rows_path)[0] and complete["plan_sha256"] == plans[folder]["sha256"],
                    "Unbound complete receipt: " + name)
            require(all(row["pass"] is True and row["exit"] == 0 and row["timeout"] is False for row in rows),
                    "Failed measurement row: " + name)
            summary_path = location / summary_name
            require(self.local(recorded["summary"]) == summary_path, "Summary path differs: " + name)
            self.check_file(summary_path, recorded["sha256"])
            windows.append({"name": name, "kind": kind, "rows": len(rows), "command": label,
                            "started_unix": command["started_unix"], "derived_end_unix": command["started_unix"] + command["seconds"],
                            "records": rows_path.relative_to(self.root).as_posix(), "records_sha256": complete["records_sha256"],
                            "complete": complete_path.relative_to(self.root).as_posix(), "summary_sha256": recorded["sha256"]})
        self.report["frozen_plan_identity"] = plans
        self.report["execution_order"] = {"status": "Pass", "ordered_without_overlap": True,
            "minimum_adjacent_command_gap_seconds": min(gaps), "last_build_or_test_end_unix": last_gate,
            "first_plan_prepare_start_unix": first_prepare, "all_measurements_after_builds_tests": True,
            "all_measurement_commands_serial": True, "windows": windows,
            "clock_scope": "Recorded top-level command intervals only; sample rows lack absolute start/end timestamps.",
            "physical_host_exclusivity": "NotEstablished"}

    def github_and_zip(self):
        require(self.args.run_metadata and self.args.artifact_metadata, "GitHub run/artifact metadata files still required")
        run, artifact = read_json(self.args.run_metadata), read_json(self.args.artifact_metadata)
        require(run["id"] == RUN_ID and run["status"] == "completed" and run["conclusion"] == "success"
                and run["head_sha"] == CANDIDATE and run["head_branch"] == "research/compile-performance"
                and run["path"] == ".github/workflows/compile-performance.yml", "GitHub run identity/status differs")
        require(artifact["workflow_run"]["id"] == RUN_ID and artifact["workflow_run"]["head_sha"] == CANDIDATE
                and artifact["expired"] is False and artifact["name"] == f"zil-compile4-{RUN_ID}-{run['run_attempt']}",
                "GitHub artifact identity differs")
        observed = {"status": "Pass", "run": run, "artifact": artifact,
                    "metadata_source": "Supplied raw GitHub API snapshots; reviewer must retain independent fetch provenance.",
                    "run_metadata_sha256": digest(self.args.run_metadata)[0],
                    "artifact_metadata_sha256": digest(self.args.artifact_metadata)[0]}
        require(self.args.zip, "Original ZIP required for independent artifact digest verification")
        checksum, size = digest(self.args.zip)
        require(artifact["digest"] == "sha256:" + checksum and artifact["size_in_bytes"] == size,
                "Original ZIP differs from GitHub artifact digest/size")
        seen = set()
        with zipfile.ZipFile(self.args.zip) as archive:
            for member in archive.infolist():
                relative = relative_path(member.filename).as_posix()
                require(relative not in seen, "Duplicate ZIP member")
                seen.add(relative)
                if member.is_dir():
                    continue
                mode = member.external_attr >> 16
                require(stat.S_IFMT(mode) in (0, stat.S_IFREG), "Nonregular ZIP member")
                with archive.open(member) as stream:
                    expected, length = stream_digest(stream)
                require(length == member.file_size, "Truncated ZIP member")
                self.check_file(safe_local(self.artifact, relative), expected, length)
        observed.update(original_zip_sha256=checksum, original_zip_bytes=size,
                        all_zip_member_bytes_verified=True, zip_member_count=len(seen))
        self.report["github_and_original_zip"] = observed

    def run(self):
        for name in ("source_identity", "inventory", "archive_modes", "commands_and_builds", "tests", "toolchain", "plans_and_order", "github_and_zip"):
            try:
                getattr(self, name)()
                print(name + ": Pass", flush=True)
            except Exception as error:
                self.report["phase_failures"].append({"phase": name, "error": type(error).__name__ + ": " + str(error)})
                print(name + ": FAIL: " + str(error), file=sys.stderr, flush=True)
        self.report["status"] = "FailOrIncomplete_NoExecutionApproval" if self.report["phase_failures"] else "Pass_TargetedExecutionAndArtifactIntegrity"
        self.report["reviewed_at_utc"] = datetime.now(timezone.utc).isoformat()
        self.report["review_script_sha256"] = digest(Path(__file__))[0]
        return self.report


def markdown(report):
    lines = ["# Compile5 独立执行核验", "", "**状态：`" + report["status"] + "`。**", "",
             f"受测提交 `{CANDIDATE}`；基线 `{BASELINE}`；Actions run `{RUN_ID}`。",
             "本报告只核验保留证据中的实际执行与完整性；未执行 Cargo、编译器或性能统计。", ""]
    if report["phase_failures"]:
        lines += ["## 未通过或待补证据", ""]
        lines += ["- `" + item["phase"] + "`：" + item["error"] for item in report["phase_failures"]]
        lines.append("")
    tests = report.get("cargo_targeted_tests")
    if tests:
        lines += ["## 实际 Cargo 测试", "", "| Package | Harness | 通过 | 失败 | 忽略 | measured | 过滤 |", "| --- | --- | ---: | ---: | ---: | ---: | ---: |"]
        for row in tests["harnesses"]:
            lines.append("| " + row["package"] + " | `" + row["target"] + "` | "
                         + " | ".join(str(row[key]) for key in ("passed", "failed", "ignored", "measured", "filtered_out")) + " |")
        lines += ["", f"共 {tests['harness_count']} 个限定 harness、{tests['passed']} 个通过；每条结果与 running/result 计数一致。", ""]
    for section, label in (("inventory", "文件清单"), ("tar_members", "原 tar 字节与权限"), ("compiler_builds", "双编译器构建"),
                           ("python_controls", "Python 控制"), ("execution_order", "执行顺序"), ("github_and_original_zip", "GitHub 与原 ZIP")):
        if section in report:
            lines.append("- " + label + "：`" + report[section]["status"] + "`；详细计数与身份见同名 JSON。")
    lines += ["", "## 范围限制", ""]
    lines += ["- " + value for value in report["limitations"]]
    lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path, help="Extracted compile4-evidence directory")
    parser.add_argument("--repo", type=Path, required=True, help="Local Git object database containing frozen commits")
    parser.add_argument("--artifact-root", type=Path, help="Extracted outer ZIP directory; defaults to evidence parent")
    parser.add_argument("--tar", type=Path, help="Original evidence tar.gz; defaults to artifact-root/compile4-evidence.tar.gz")
    parser.add_argument("--zip", type=Path, help="Retained original downloaded ZIP; required for a passing final review")
    parser.add_argument("--run-metadata", type=Path, help="Raw independently fetched GitHub run JSON")
    parser.add_argument("--artifact-metadata", type=Path, help="Raw independently fetched GitHub artifact JSON")
    parser.add_argument("--out", type=Path, required=True, help="New review JSON outside artifact directory; paired .md also created")
    args = parser.parse_args()
    audit = Audit(args)
    output = args.out.resolve()
    require(output != audit.artifact and audit.artifact not in output.parents, "Review output must be outside original artifact")
    require(not output.exists() and not output.with_suffix(".md").exists(), "Refusing to overwrite a prior review receipt")
    report = audit.run()
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x") as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    with output.with_suffix(".md").open("x") as stream:
        stream.write(markdown(report))
    print(json.dumps({"status": report["status"], "review": str(output), "failed_phases": len(report["phase_failures"])}))
    return int(bool(report["phase_failures"]))


if __name__ == "__main__":
    raise SystemExit(main())
